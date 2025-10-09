use super::session::{Authentication, Session};
use eyre::Result;
use std::collections::HashMap;
use std::sync::Arc;

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

    /// Connect to a host and return a session
    /// If the session already exists, return it
    /// If the existing session is dead, remove it and create a new one
    pub async fn connect(
        &mut self,
        host: &str,
        username: Option<&str>,
        auth: Option<Authentication>,
    ) -> Result<Arc<Session>> {
        let username = username.unwrap_or("root");
        let key = format!("{username}@{host}");

        // Check if we have an existing connection
        if let Some(session) = self.get(host, username) {
            // Test if the connection is still alive
            if session.send_keepalive().await {
                return Ok(session);
            } else {
                // Connection is dead, remove it from the pool
                log::debug!("Removing dead SSH connection for {key}");
                self.connections.remove(&key);
            }
        }

        // Create a new connection
        log::debug!("Creating new SSH connection for {key}");
        let mut session = Session::open(host).await?;
        session.authenticate(auth, Some(username)).await?;

        let session = Arc::new(session);
        self.connections.insert(key, session.clone());

        Ok(session)
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
