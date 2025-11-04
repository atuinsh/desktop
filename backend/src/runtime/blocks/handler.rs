use crate::commands::agent::AgentSessionRegistry;
use crate::runtime::blocks::document::actor::DocumentError;
use crate::runtime::blocks::document::block_context::{BlockContext, ContextResolver};
use crate::runtime::blocks::document::{actor::DocumentHandle, bridge::DocumentBridgeMessage};
use crate::runtime::config::RuntimeConfig;
use crate::runtime::events::{EventBus, GCEvent};
use crate::runtime::pty_store::PtyStoreHandle;
use crate::runtime::ssh_pool::SshPoolHandle;
use crate::runtime::workflow::event::WorkflowEvent;
use crate::runtime::ClientMessageChannel;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot, RwLock};
use ts_rs::TS;
use typed_builder::TypedBuilder;
use uuid::Uuid;

#[derive(TypedBuilder, Clone)]
pub struct ExecutionContext {
    pub block_id: Uuid,
    pub runbook_id: Uuid,
    pub document_handle: Arc<DocumentHandle>,
    pub context_resolver: Arc<ContextResolver>,
    #[builder(default, setter(strip_option(fallback = output_channel_opt)))]
    output_channel: Option<Arc<dyn ClientMessageChannel<DocumentBridgeMessage>>>,
    workflow_event_sender: broadcast::Sender<WorkflowEvent>,
    #[builder(default, setter(strip_option(fallback = ssh_pool_opt)))]
    pub ssh_pool: Option<SshPoolHandle>,
    #[builder(default, setter(strip_option(fallback = pty_store_opt)))]
    pub pty_store: Option<PtyStoreHandle>,
    #[builder(default, setter(strip_option(fallback = event_bus_opt)))]
    pub gc_event_bus: Option<Arc<dyn EventBus>>,
    #[builder(default)]
    pub runtime_config: Arc<RuntimeConfig>,
    #[builder(default, setter(strip_option(fallback = agent_session_registry_opt)))]
    pub agent_session_registry: Option<Arc<AgentSessionRegistry>>,
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
    /// Emit a Workflow event
    pub fn emit_workflow_event(&self, event: WorkflowEvent) -> Result<(), DocumentError> {
        self.workflow_event_sender
            .send(event)
            .map_err(|_| DocumentError::EventSendError)?;
        Ok(())
    }

    /// Send a message to the output channel
    pub async fn send_output(
        &self,
        message: impl Into<DocumentBridgeMessage>,
    ) -> Result<(), DocumentError> {
        if let Some(chan) = &self.output_channel {
            chan.send(message.into())
                .await
                .map_err(|_| DocumentError::OutputSendError)?;
        }
        Ok(())
    }

    /// Clear the active context for a block
    pub async fn clear_active_context(&self, block_id: Uuid) -> Result<(), DocumentError> {
        self.document_handle
            .update_active_context(block_id, |ctx| *ctx = BlockContext::new())
            .await
    }

    /// Update the passive context for a block
    pub async fn update_passive_context(
        &self,
        block_id: Uuid,
        update_fn: Box<dyn FnOnce(&mut BlockContext) + Send>,
    ) -> Result<(), DocumentError> {
        self.document_handle
            .update_passive_context(block_id, update_fn)
            .await
    }

    /// Update the active context for a block
    pub async fn update_active_context(
        &self,
        block_id: Uuid,
        update_fn: Box<dyn FnOnce(&mut BlockContext) + Send>,
    ) -> Result<(), DocumentError> {
        self.document_handle
            .update_active_context(block_id, update_fn)
            .await
    }

    /// Emit a Grand Central event
    pub async fn emit_gc_event(&self, event: GCEvent) -> Result<(), DocumentError> {
        if let Some(event_bus) = &self.gc_event_bus {
            let _ = event_bus.emit(event).await;
        }
        Ok(())
    }

    /// Emit a BlockStarted event via Grand Central
    pub async fn emit_block_started(&self) -> Result<(), DocumentError> {
        self.emit_gc_event(GCEvent::BlockStarted {
            block_id: self.block_id,
            runbook_id: self.runbook_id,
        })
        .await
    }

    /// Emit a BlockFinished event via Grand Central
    pub async fn emit_block_finished(&self, success: bool) -> Result<(), DocumentError> {
        self.emit_gc_event(GCEvent::BlockFinished {
            block_id: self.block_id,
            runbook_id: self.runbook_id,
            success,
        })
        .await
    }

    /// Emit a BlockFailed event via Grand Central
    pub async fn emit_block_failed(&self, error: String) -> Result<(), DocumentError> {
        self.emit_gc_event(GCEvent::BlockFailed {
            block_id: self.block_id,
            runbook_id: self.runbook_id,
            error,
        })
        .await
    }

    /// Emit a BlockCancelled event via Grand Central
    pub async fn emit_block_cancelled(&self) -> Result<(), DocumentError> {
        self.emit_gc_event(GCEvent::BlockCancelled {
            block_id: self.block_id,
            runbook_id: self.runbook_id,
        })
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

#[derive(TS, Debug, Clone, Serialize, Deserialize, TypedBuilder)]
#[ts(export)]
pub struct BlockOutput {
    pub block_id: Uuid,
    #[builder(default, setter(strip_option(fallback = stdout_opt)))]
    pub stdout: Option<String>,
    #[builder(default, setter(strip_option(fallback = stderr_opt)))]
    pub stderr: Option<String>,
    #[builder(default, setter(strip_option(fallback = lifecycle_opt)))]
    pub lifecycle: Option<BlockLifecycleEvent>,
    #[builder(default, setter(strip_option(fallback = binary_opt)))]
    pub binary: Option<Vec<u8>>, // For terminal raw data
    #[builder(default, setter(strip_option(fallback = object_opt)))]
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
