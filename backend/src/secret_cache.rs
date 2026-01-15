use std::{collections::HashMap, sync::Arc, time::Duration};

use keyring::Entry;
use sqlx::SqlitePool;
use tokio::{sync::RwLock, task::JoinHandle, time::Instant};

#[derive(Debug, thiserror::Error)]
pub enum SecretCacheError {
    #[error("lookup failed for service {service} and user {user}: {context}")]
    LookupFailed {
        service: String,
        user: String,
        context: String,
    },

    #[error("service or user is invalid for service {service} and user {user}: {context}")]
    ServiceOrUserInvalid {
        service: String,
        user: String,
        context: String,
    },

    #[error("could not delete secret for service {service} and user {user}: {context}")]
    CouldNotDelete {
        service: String,
        user: String,
        context: String,
    },

    #[error("could not set secret for service {service} and user {user}: {context}")]
    CouldNotSet {
        service: String,
        user: String,
        context: String,
    },
}

pub struct SecretCache {
    inner: Arc<RwLock<SecretCacheInner>>,
    storage: Arc<dyn SecretStorage>,
    invalidator: JoinHandle<()>,
}

/// A secret cache for storing secrets.
/// It is implemented as a write-through cache, meaning that all writes are immediately written to the
/// underlying storage, and all reads are first checked against the cache.
///
/// The cache is invalidated every minute; secrets that have not been used in the last 15 minutes are deleted
/// from the cache.
impl SecretCache {
    pub fn new(storage: Arc<dyn SecretStorage>) -> Self {
        let inner = Arc::new(RwLock::new(SecretCacheInner::new(Duration::from_mins(15))));

        let inner_clone = inner.clone();
        let invalidator = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                inner_clone.write().await.invalidate_expired();
            }
        });

        Self {
            inner,
            storage,
            invalidator,
        }
    }

    pub async fn get(&self, service: &str, user: &str) -> Result<Option<String>, SecretCacheError> {
        // Check cache first
        if let Some(secret) = self.inner.write().await.get(service, user) {
            return Ok(Some(secret.clone()));
        }

        // Cache miss - fetch from storage
        let secret = self.storage.get(service, user).await?;

        // Cache the result if found
        if let Some(ref secret) = secret {
            self.inner.write().await.insert(service, user, secret);
        }

        Ok(secret)
    }

    pub async fn delete(&self, service: &str, user: &str) -> Result<(), SecretCacheError> {
        // Delete from storage first
        self.storage.delete(service, user).await?;

        // Then remove from cache
        self.inner.write().await.remove(service, user);

        Ok(())
    }

    pub async fn set(
        &self,
        service: &str,
        user: &str,
        value: &str,
    ) -> Result<(), SecretCacheError> {
        // Write to storage first
        self.storage.set(service, user, value).await?;

        // Then update cache
        self.inner.write().await.insert(service, user, value);

        Ok(())
    }
}

impl Drop for SecretCache {
    fn drop(&mut self) {
        self.invalidator.abort();
    }
}

pub struct SecretCacheInner {
    cache_duration: Duration,
    cache: HashMap<(String, String), String>,
    last_used: HashMap<(String, String), Instant>,
}

impl SecretCacheInner {
    pub fn new(cache_duration: Duration) -> Self {
        Self {
            cache_duration,
            cache: HashMap::new(),
            last_used: HashMap::new(),
        }
    }

    pub fn insert(&mut self, service: &str, user: &str, value: &str) {
        self.cache
            .insert((service.to_string(), user.to_string()), value.to_string());
        self.last_used
            .insert((service.to_string(), user.to_string()), Instant::now());
    }

    pub fn get(&mut self, service: &str, user: &str) -> Option<&String> {
        let cached = self.cache.get(&(service.to_string(), user.to_string()));
        if let Some(cached) = cached {
            let last_used = self
                .last_used
                .entry((service.to_string(), user.to_string()))
                .or_insert(Instant::now());
            *last_used = Instant::now();
            Some(cached)
        } else {
            None
        }
    }

    pub fn remove(&mut self, service: &str, user: &str) {
        self.cache.remove(&(service.to_string(), user.to_string()));
        self.last_used
            .remove(&(service.to_string(), user.to_string()));
    }

    pub fn invalidate_expired(&mut self) {
        let mut to_delete = Vec::with_capacity(self.cache.len());

        for (service, user) in self.cache.keys() {
            let last_used = self
                .last_used
                .entry((service.clone(), user.clone()))
                .or_insert(Instant::now());

            if last_used.elapsed() > self.cache_duration {
                to_delete.push((service.clone(), user.clone()));
            } else {
                *last_used = Instant::now();
            }
        }

        for (service, user) in to_delete {
            self.cache.remove(&(service.clone(), user.clone()));
            self.last_used.remove(&(service, user));
        }
    }
}

#[async_trait::async_trait]
pub trait SecretStorage: Send + Sync {
    async fn get(&self, service: &str, user: &str) -> Result<Option<String>, SecretCacheError>;
    async fn set(&self, service: &str, user: &str, value: &str) -> Result<(), SecretCacheError>;
    async fn delete(&self, service: &str, user: &str) -> Result<(), SecretCacheError>;
}

pub struct KeychainSecretStorage;

#[async_trait::async_trait]
impl SecretStorage for KeychainSecretStorage {
    async fn get(&self, service: &str, user: &str) -> Result<Option<String>, SecretCacheError> {
        let entry =
            Entry::new(service, user).map_err(|e| SecretCacheError::ServiceOrUserInvalid {
                service: service.to_string(),
                user: user.to_string(),
                context: e.to_string(),
            })?;

        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecretCacheError::LookupFailed {
                service: service.to_string(),
                user: user.to_string(),
                context: e.to_string(),
            }),
        }
    }

    async fn set(&self, service: &str, user: &str, value: &str) -> Result<(), SecretCacheError> {
        let entry =
            Entry::new(service, user).map_err(|e| SecretCacheError::ServiceOrUserInvalid {
                service: service.to_string(),
                user: user.to_string(),
                context: e.to_string(),
            })?;

        entry
            .set_password(value)
            .map_err(|e| SecretCacheError::CouldNotSet {
                service: service.to_string(),
                user: user.to_string(),
                context: e.to_string(),
            })
    }

    async fn delete(&self, service: &str, user: &str) -> Result<(), SecretCacheError> {
        let entry =
            Entry::new(service, user).map_err(|e| SecretCacheError::ServiceOrUserInvalid {
                service: service.to_string(),
                user: user.to_string(),
                context: e.to_string(),
            })?;

        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()), // Already deleted, that's fine
            Err(e) => Err(SecretCacheError::CouldNotDelete {
                service: service.to_string(),
                user: user.to_string(),
                context: e.to_string(),
            }),
        }
    }
}

pub struct KvDbSecretStorage {
    prefix: String,
    pool: SqlitePool,
}

impl KvDbSecretStorage {
    pub fn new(prefix: String, pool: SqlitePool) -> Self {
        Self { prefix, pool }
    }

    /// Build the namespaced key for storing secrets: prefix::secrets::service::user
    fn make_key(&self, service: &str, user: &str) -> String {
        format!("{}::secrets::{}::{}", self.prefix, service, user)
    }
}

#[async_trait::async_trait]
impl SecretStorage for KvDbSecretStorage {
    async fn get(&self, service: &str, user: &str) -> Result<Option<String>, SecretCacheError> {
        let key = self.make_key(service, user);
        crate::kv::get::<String>(&self.pool, &key)
            .await
            .map_err(|e| SecretCacheError::LookupFailed {
                service: service.to_string(),
                user: user.to_string(),
                context: e,
            })
    }

    async fn set(&self, service: &str, user: &str, value: &str) -> Result<(), SecretCacheError> {
        let key = self.make_key(service, user);
        crate::kv::set(&self.pool, &key, &value.to_string())
            .await
            .map_err(|e| SecretCacheError::CouldNotSet {
                service: service.to_string(),
                user: user.to_string(),
                context: e,
            })
    }

    async fn delete(&self, service: &str, user: &str) -> Result<(), SecretCacheError> {
        let key = self.make_key(service, user);
        // Use raw SQL to delete since kv module doesn't have a delete function
        sqlx::query("DELETE FROM kv WHERE key = $1")
            .bind(&key)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|e| SecretCacheError::CouldNotDelete {
                service: service.to_string(),
                user: user.to_string(),
                context: e.to_string(),
            })
    }
}
