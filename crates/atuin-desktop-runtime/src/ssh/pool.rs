use super::session::{AuthResult, Authentication, Session};
use crate::context::DocumentSshConfig;
use eyre::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::oneshot;

// A pool of ssh connections
// This avoids opening several ssh connections to the same host
// Intended to be wrapped by an actor in our runtime, so we do not use
// thread safe primitives

pub struct Pool {
    /// A map of ssh connections, host -> session
    /// Session is wrapped in Arc to be shared
    pub connections: HashMap<String, Arc<Session>>,
}

impl Default for Pool {
    fn default() -> Self {
        Pool::new()
    }
}

impl Pool {
    pub fn new() -> Self {
        Pool {
            connections: HashMap::new(),
        }
    }

    /// Connect to a host and return a session along with any authentication warnings
    /// If the session already exists, return it (with no warnings)
    /// If the existing session is dead, remove it and create a new one
    pub async fn connect(
        &mut self,
        host: &str,
        username: Option<&str>,
        auth: Option<Authentication>,
        cancellation_rx: Option<oneshot::Receiver<()>>,
    ) -> Result<(Arc<Session>, AuthResult)> {
        self.connect_with_config(host, username, auth, cancellation_rx, None)
            .await
    }

    /// Connect to a host with optional block configuration overrides
    /// If the session already exists, return it (with no warnings)
    /// If the existing session is dead, remove it and create a new one
    pub async fn connect_with_config(
        &mut self,
        host: &str,
        username: Option<&str>,
        auth: Option<Authentication>,
        cancellation_rx: Option<oneshot::Receiver<()>>,
        ssh_config_override: Option<&DocumentSshConfig>,
    ) -> Result<(Arc<Session>, AuthResult)> {
        let ssh_config = Session::resolve_ssh_config(host);

        // Determine username: block override > provided > SSH config > current user
        let username = ssh_config_override
            .and_then(|cfg| cfg.user.as_deref())
            .or(username)
            .map(|u| u.to_string())
            .or(ssh_config.username)
            .unwrap_or_else(whoami::username);

        let key = format!("{username}@{host}");

        tracing::debug!("connecting to {key}");

        if let Some(session) = self.get(host, &username) {
            tracing::debug!("found existing ssh session in pool");
            if session.send_keepalive().await {
                tracing::debug!("session keepalive success");
                // Existing connection, no new warnings
                return Ok((session, AuthResult::default()));
            } else {
                tracing::debug!("Removing dead SSH connection for {key}");
                self.connections.remove(&key);
            }
        }

        let identity_key_config = ssh_config_override.and_then(|cfg| cfg.identity_key.as_ref());
        let certificate_config = ssh_config_override.and_then(|cfg| cfg.certificate.as_ref());
        tracing::debug!(
            "Pool connect_with_config: ssh_config_override={:?}, identity_key_config={:?}, certificate_config={:?}",
            ssh_config_override,
            identity_key_config,
            certificate_config
        );

        let async_session = async {
            let mut session = Session::open_with_config(host, ssh_config_override).await?;
            let auth_result = session
                .authenticate_with_config(
                    auth,
                    Some(&username),
                    identity_key_config,
                    certificate_config,
                )
                .await?;
            Ok::<_, eyre::Report>((session, auth_result))
        };

        tracing::debug!("Creating new SSH connection for {key}");
        let (session, auth_result) = if let Some(mut cancellation_rx) = cancellation_rx {
            tokio::select! {
                result = async_session => {
                    result?
                }
                _ = &mut cancellation_rx => {
                    tracing::debug!("SSH connection {key} cancelled");
                    return Err(eyre::eyre!("SSH connection cancelled"));
                }
            }
        } else {
            async_session.await?
        };

        let session = Arc::new(session);
        self.connections.insert(key, session.clone());

        Ok((session, auth_result))
    }

    pub fn get(&self, host: &str, username: &str) -> Option<Arc<Session>> {
        self.connections.get(&format!("{username}@{host}")).cloned()
    }

    pub async fn disconnect(&mut self, host: &str, username: &str) -> Result<()> {
        let key = format!("{username}@{host}");
        if let Some(session) = self.connections.remove(&key) {
            session.disconnect().await?;
        }

        Ok(())
    }
}
