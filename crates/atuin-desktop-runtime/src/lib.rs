use async_trait::async_trait;
use serde::Serialize;

mod blocks;
mod document;
mod events;
mod exec_log;
mod pty;
mod pty_store;
mod ssh;
mod ssh_pool;
mod workflow;

#[async_trait]
pub trait MessageChannel<M: Serialize + Send + Sync>: Send + Sync {
    async fn send(&self, message: M) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

#[async_trait]
impl<M: Serialize + Send + Sync> MessageChannel<M>
    for fn(M) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    async fn send(&self, message: M) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self(message)
    }
}

pub use blocks::handler::ExecutionHandle;
pub use blocks::mysql::decode as mysql_decode;
pub use blocks::script::ScriptOutput; // todo - should this be public? should we move it?
pub use blocks::{Block, BlockBehavior, FromDocument};
pub use document::actor::{DocumentHandle, LocalValueProvider};
pub use document::block_context::{
    BlockContext, BlockContextItem, BlockContextStorage, BlockWithContext, ContextResolver,
    ResolvedContext,
};
pub use document::bridge::{ClientPromptResult, DocumentBridgeMessage};
pub use document::Document;
pub use events::{EventBus, GCEvent};
pub use exec_log::ExecLogHandle;
pub use pty::{Pty, PtyMetadata};
pub use pty_store::PtyStoreHandle;
pub use ssh_pool::SshPoolHandle;
pub use workflow::dependency::DependencySpec;
pub use workflow::event::{WorkflowCommand, WorkflowEvent};
pub use workflow::executor::ExecutorHandle;
