use async_trait::async_trait;
use serde::Serialize;

pub(crate) mod blocks;
pub(crate) mod events;
pub(crate) mod exec_log;
pub(crate) mod pty_store;
pub(crate) mod ssh;
pub(crate) mod ssh_pool;
pub(crate) mod workflow;

#[async_trait]
pub trait ClientMessageChannel<M: Serialize + Send + Sync>: Send + Sync {
    async fn send(&self, message: M) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}
