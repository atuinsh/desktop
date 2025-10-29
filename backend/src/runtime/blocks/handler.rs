use crate::runtime::blocks::document::actor::DocumentError;
use crate::runtime::blocks::document::block_context::{BlockContext, ContextResolver};
use crate::runtime::blocks::document::{actor::DocumentHandle, bridge::DocumentBridgeMessage};
use crate::runtime::events::EventBus;
use crate::runtime::pty_store::PtyStoreHandle;
use crate::runtime::ssh_pool::SshPoolHandle;
use crate::runtime::workflow::event::WorkflowEvent;
use crate::runtime::ClientMessageChannel;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot, RwLock};
use ts_rs::TS;
use uuid::Uuid;

pub struct ExecutionContext {
    pub runbook_id: Uuid,
    pub document_handle: Arc<DocumentHandle>,
    pub context_resolver: ContextResolver,
    pub output_channel: Option<Arc<dyn ClientMessageChannel<DocumentBridgeMessage>>>,
    pub event_sender: broadcast::Sender<WorkflowEvent>,
    pub ssh_pool: Option<SshPoolHandle>,
    pub pty_store: Option<PtyStoreHandle>,
    pub event_bus: Option<Arc<dyn EventBus>>,
}

impl Clone for ExecutionContext {
    fn clone(&self) -> Self {
        Self {
            runbook_id: self.runbook_id,
            document_handle: self.document_handle.clone(),
            context_resolver: self.context_resolver.clone(),
            output_channel: self.output_channel.clone(),
            event_sender: self.event_sender.clone(),
            ssh_pool: self.ssh_pool.clone(),
            pty_store: self.pty_store.clone(),
            event_bus: self.event_bus.clone(),
        }
    }
}

impl Debug for ExecutionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionContext")
            .field("runbook_id", &self.runbook_id)
            .field("context_resolver", &self.context_resolver)
            .finish()
    }
}

impl ExecutionContext {
    pub async fn send_output(&self, message: DocumentBridgeMessage) -> Result<(), DocumentError> {
        if let Some(chan) = &self.output_channel {
            chan.send(message)
                .await
                .map_err(|_| DocumentError::ActorSendError)?;
        }
        Ok(())
    }

    pub async fn clear_active_context(&self, block_id: Uuid) -> Result<(), DocumentError> {
        self.document_handle
            .update_active_context(block_id, |ctx| *ctx = BlockContext::new())
            .await
    }

    pub async fn update_passive_context(
        &self,
        block_id: Uuid,
        update_fn: Box<dyn FnOnce(&mut BlockContext) + Send>,
    ) -> Result<(), DocumentError> {
        self.document_handle
            .update_passive_context(block_id, update_fn)
            .await
    }

    pub async fn update_active_context(
        &self,
        block_id: Uuid,
        update_fn: Box<dyn FnOnce(&mut BlockContext) + Send>,
    ) -> Result<(), DocumentError> {
        self.document_handle
            .update_active_context(block_id, update_fn)
            .await
    }
}

// Channel-based cancellation token
#[derive(Clone)]
pub struct CancellationToken {
    sender: Arc<std::sync::Mutex<Option<oneshot::Sender<()>>>>,
    receiver: Arc<std::sync::Mutex<Option<oneshot::Receiver<()>>>>,
}

impl Default for CancellationToken {
    fn default() -> Self {
        let (sender, receiver) = oneshot::channel();
        Self {
            sender: Arc::new(std::sync::Mutex::new(Some(sender))),
            receiver: Arc::new(std::sync::Mutex::new(Some(receiver))),
        }
    }
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        if let Ok(mut sender_guard) = self.sender.lock() {
            if let Some(sender) = sender_guard.take() {
                let _ = sender.send(()); // Ignore error if receiver already dropped
            }
        }
    }

    pub fn take_receiver(&self) -> Option<oneshot::Receiver<()>> {
        if let Ok(mut receiver_guard) = self.receiver.lock() {
            receiver_guard.take()
        } else {
            None
        }
    }
}

#[derive(Clone)]
pub struct ExecutionHandle {
    pub id: Uuid,
    #[allow(dead_code)] // Used for tracking but not currently accessed
    pub block_id: Uuid,
    pub cancellation_token: CancellationToken,
    pub status: Arc<RwLock<ExecutionStatus>>,
    pub output_variable: Option<String>,
}

#[derive(TS, Clone, Debug, Serialize, Deserialize)]
#[ts(tag = "type", content = "data", export)]
pub enum ExecutionStatus {
    Running,
    Success(String), // The output value
    #[allow(dead_code)] // Error message is used but compiler doesn't see reads
    Failed(String), // Error message
    #[allow(dead_code)] // Used for cancellation but not currently constructed in tests
    Cancelled,
}

#[derive(TS, Debug, Clone, Serialize, Deserialize)]
#[ts(export)]
pub struct BlockOutput {
    pub block_id: Uuid,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub lifecycle: Option<BlockLifecycleEvent>,
    pub binary: Option<Vec<u8>>,           // For terminal raw data
    pub object: Option<serde_json::Value>, // For structured JSON data
}

#[derive(TS, Debug, Clone, Serialize, Deserialize)]
#[ts(export)]
pub struct BlockFinishedData {
    pub exit_code: Option<i32>,
    pub success: bool,
}

#[derive(TS, Debug, Clone, Serialize, Deserialize)]
#[ts(export)]
pub struct BlockErrorData {
    pub message: String,
}

#[derive(TS, Debug, Clone, Serialize, Deserialize)]
#[ts(tag = "type", content = "data", export)]
#[serde(rename_all = "camelCase", tag = "type", content = "data")]
pub enum BlockLifecycleEvent {
    Started,
    Finished(BlockFinishedData),
    Cancelled,
    Error(BlockErrorData),
}

// BlockHandler and ContextProvider traits removed - using BlockBehavior::execute() instead
