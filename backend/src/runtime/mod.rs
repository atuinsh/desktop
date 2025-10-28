use serde::Serialize;

pub(crate) mod blocks;
pub(crate) mod events;
pub(crate) mod exec_log;
pub(crate) mod pty_store;
pub(crate) mod ssh;
pub(crate) mod ssh_pool;
pub(crate) mod workflow;

// TODO: this may need to be async, but Tauri channels are sync and are used in sync code
// (see Document::rebuild_passive_contexts)
pub trait ClientMessageChannel<M: Serialize + Send + Sync>: Send + Sync {
    fn send(&self, message: M) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

impl<M: Serialize + Send + Sync> ClientMessageChannel<M>
    for fn(M) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    fn send(&self, message: M) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self(message)
    }
}
