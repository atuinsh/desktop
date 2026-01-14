//! Block execution context and lifecycle management
//!
//! This module provides the execution context that blocks use to interact with
//! the runtime environment, emit events, update context, and control their lifecycle.
//!
//! Key types:
//! - [`ExecutionContext`]: Provides access to runtime resources and utilities
//! - [`ExecutionHandle`]: Tracks execution state and provides cancellation
//! - [`BlockOutput`]: Represents output from block execution

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{oneshot, watch, Mutex, RwLock};
use ts_rs::TS;
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::client::{
    ClientPrompt, ClientPromptResult, DocumentBridgeMessage, LocalValueProvider, MessageChannel,
    RunbookContentLoader,
};
use crate::context::{BlockContext, BlockExecutionOutput, BlockState, ContextResolver};
use crate::document::{DocumentError, DocumentHandle};
use crate::events::{EventBus, GCEvent};
use crate::pty::PtyStoreHandle;
use crate::ssh::SshPoolHandle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionResult {
    Success,
    Failure,
    Cancelled,
    Paused,
}

/// Context provided to blocks during execution
///
/// This context gives blocks access to:
/// - Document state and context resolution
/// - Output channels for sending messages to the client
/// - Resource pools (SSH connections, PTYs)
/// - Event buses for monitoring
/// - Lifecycle management methods
#[derive(TypedBuilder, Clone)]
pub struct ExecutionContext {
    pub(crate) block_id: Uuid,
    pub(crate) runbook_id: Uuid,
    document_handle: Arc<DocumentHandle>,
    pub context_resolver: Arc<ContextResolver>,
    #[builder(default, setter(strip_option(fallback = output_channel_opt)))]
    output_channel: Option<Arc<dyn MessageChannel<DocumentBridgeMessage>>>,
    #[builder(default, setter(strip_option(fallback = ssh_pool_opt)))]
    pub(crate) ssh_pool: Option<SshPoolHandle>,
    #[builder(default, setter(strip_option(fallback = pty_store_opt)))]
    pub(crate) pty_store: Option<PtyStoreHandle>,
    #[builder(default, setter(strip_option(fallback = event_bus_opt)))]
    pub(crate) gc_event_bus: Option<Arc<dyn EventBus>>,
    handle: ExecutionHandle,
    /// Stack of runbook IDs currently being executed (for sub-runbook recursion detection)
    #[builder(default)]
    execution_stack: Vec<String>,
    /// Loader for sub-runbook content (optional - sub-runbooks won't work without this)
    #[builder(default, setter(strip_option(fallback = runbook_loader_opt)))]
    runbook_loader: Option<Arc<dyn RunbookContentLoader>>,
}

impl std::fmt::Debug for ExecutionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionContext")
            .field("runbook_id", &self.runbook_id)
            .field("context_resolver", &self.context_resolver)
            .finish()
    }
}

impl ExecutionContext {
    /// Get the execution handle
    pub fn handle(&self) -> ExecutionHandle {
        self.handle.clone()
    }

    /// Subscribe to the finish event
    pub fn finished_channel(&self) -> watch::Receiver<Option<ExecutionResult>> {
        self.handle().finished_channel()
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
    pub async fn update_passive_context<F>(
        &self,
        block_id: Uuid,
        update_fn: F,
    ) -> Result<(), DocumentError>
    where
        F: FnOnce(&mut BlockContext) + Send + 'static,
    {
        self.document_handle
            .update_passive_context(block_id, update_fn)
            .await
    }

    /// Update the active context for a block
    pub async fn update_active_context<F>(
        &self,
        block_id: Uuid,
        update_fn: F,
    ) -> Result<(), DocumentError>
    where
        F: FnOnce(&mut BlockContext) + Send + 'static,
    {
        self.document_handle
            .update_active_context(block_id, update_fn)
            .await
    }

    /// Update the private block state
    pub async fn update_block_state<T: BlockState, F>(
        &self,
        block_id: Uuid,
        update_fn: F,
    ) -> Result<(), DocumentError>
    where
        F: FnOnce(&mut T) + Send + 'static,
    {
        self.document_handle
            .update_block_state::<T, _>(block_id, update_fn)
            .await
    }

    /// Set the block output
    pub async fn set_block_output(
        &self,
        output: impl BlockExecutionOutput,
    ) -> Result<(), DocumentError> {
        self.document_handle
            .set_block_execution_output(self.block_id, output)
            .await
    }

    /// Set the block output (boxed variant)
    pub async fn set_block_output_boxed(
        &self,
        output: Box<dyn BlockExecutionOutput>,
    ) -> Result<(), DocumentError> {
        self.document_handle
            .set_block_execution_output_boxed(self.block_id, output)
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
    async fn emit_block_started(&self) -> Result<(), DocumentError> {
        self.emit_gc_event(GCEvent::BlockStarted {
            block_id: self.block_id,
            runbook_id: self.runbook_id,
        })
        .await
    }

    /// Emit a BlockFinished event via Grand Central
    async fn emit_block_finished(&self, success: bool) -> Result<(), DocumentError> {
        self.emit_gc_event(GCEvent::BlockFinished {
            block_id: self.block_id,
            runbook_id: self.runbook_id,
            success,
        })
        .await
    }

    /// Emit a BlockFailed event via Grand Central
    async fn emit_block_failed(&self, error: String) -> Result<(), DocumentError> {
        self.emit_gc_event(GCEvent::BlockFailed {
            block_id: self.block_id,
            runbook_id: self.runbook_id,
            error,
        })
        .await
    }

    /// Emit a BlockCancelled event via Grand Central
    async fn emit_block_cancelled(&self) -> Result<(), DocumentError> {
        self.emit_gc_event(GCEvent::BlockCancelled {
            block_id: self.block_id,
            runbook_id: self.runbook_id,
        })
        .await
    }

    /// Mark a block as started
    /// Sends appropriate events to Grand Central and the output channel
    pub async fn block_started(&self) -> Result<(), DocumentError> {
        let _ = self.handle().set_running().await;
        let _ = self.emit_block_started().await;

        let _ = self
            .send_output(
                StreamingBlockOutput::builder()
                    .block_id(self.block_id)
                    .lifecycle(BlockLifecycleEvent::Started(self.handle.id))
                    .build(),
            )
            .await;
        Ok(())
    }

    /// Mark a block as finished
    /// Sends appropriate events to Grand Central and the output channel
    pub async fn block_finished(
        &self,
        exit_code: Option<i32>,
        success: bool,
    ) -> Result<(), DocumentError> {
        let _ = self.handle().set_success().await;
        let _ = self.emit_block_finished(success).await;
        let _ = self
            .send_output(
                StreamingBlockOutput::builder()
                    .block_id(self.block_id)
                    .lifecycle(BlockLifecycleEvent::Finished(BlockFinishedData {
                        exit_code,
                        success,
                    }))
                    .build(),
            )
            .await;
        let _ = self
            .handle()
            .on_finish
            .0
            .send(Some(ExecutionResult::Success));
        Ok(())
    }

    /// Mark a block as failed
    /// Sends appropriate events to Grand Central and the output channel
    pub async fn block_failed(&self, error: String) -> Result<(), DocumentError> {
        let _ = self.handle().set_failed(error.clone()).await;
        let _ = self.emit_block_failed(error.clone()).await;
        let _ = self
            .send_output(
                StreamingBlockOutput::builder()
                    .block_id(self.block_id)
                    .lifecycle(BlockLifecycleEvent::Error(BlockErrorData {
                        message: error,
                    }))
                    .build(),
            )
            .await;
        let _ = self
            .handle()
            .on_finish
            .0
            .send(Some(ExecutionResult::Failure));
        Ok(())
    }

    /// Mark a block as cancelled
    /// Sends appropriate events to Grand Central and the output channel
    pub async fn block_cancelled(&self) -> Result<(), DocumentError> {
        let _ = self.handle().set_cancelled().await;
        let _ = self.emit_block_cancelled().await;
        let _ = self
            .send_output(
                StreamingBlockOutput::builder()
                    .block_id(self.block_id)
                    .lifecycle(BlockLifecycleEvent::Cancelled)
                    .build(),
            )
            .await;
        let _ = self
            .handle()
            .on_finish
            .0
            .send(Some(ExecutionResult::Cancelled));
        Ok(())
    }

    /// Mark a block as paused
    /// This stops the serial execution at this block and signals the frontend
    /// Sends appropriate events to Grand Central and the output channel
    pub async fn block_paused(&self) -> Result<(), DocumentError> {
        let _ = self.handle().set_success().await;
        let _ = self
            .emit_gc_event(GCEvent::SerialExecutionPaused {
                runbook_id: self.runbook_id,
                block_id: self.block_id,
            })
            .await;
        let _ = self
            .send_output(
                StreamingBlockOutput::builder()
                    .block_id(self.block_id)
                    .lifecycle(BlockLifecycleEvent::Paused)
                    .build(),
            )
            .await;
        let _ = self
            .handle()
            .on_finish
            .0
            .send(Some(ExecutionResult::Paused));
        Ok(())
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.handle().cancellation_token.clone()
    }

    pub fn cancellation_receiver(&self) -> Option<oneshot::Receiver<()>> {
        self.handle().cancellation_token.clone().take_receiver()
    }

    pub async fn prompt_client(
        &self,
        prompt: ClientPrompt,
    ) -> Result<ClientPromptResult, DocumentError> {
        let prompt_id = Uuid::new_v4();
        let (sender, receiver) = oneshot::channel();

        self.handle()
            .prompt_callbacks
            .lock()
            .await
            .insert(prompt_id, sender);

        self.send_output(DocumentBridgeMessage::ClientPrompt {
            execution_id: self.handle().id,
            prompt_id,
            prompt,
        })
        .await
        .map_err(|_| DocumentError::OutputSendError)?;

        let result = receiver.await.map_err(|_| DocumentError::EventSendError)?;

        Ok(result)
    }

    /// Check if a runbook is already in the execution stack (recursion detection)
    pub fn is_in_execution_stack(&self, runbook_id: &str) -> bool {
        self.execution_stack.contains(&runbook_id.to_string())
    }

    /// Get the current execution stack (for error reporting)
    pub fn execution_stack(&self) -> &[String] {
        &self.execution_stack
    }

    /// Create a new execution context for a sub-runbook with the current runbook pushed onto the stack
    ///
    /// Returns an error if the sub-runbook ID is already in the execution stack (recursion detected)
    pub fn with_sub_runbook(
        &self,
        sub_runbook_id: String,
        sub_runbook_block_id: Uuid,
        sub_context_resolver: Arc<ContextResolver>,
    ) -> Result<Self, SubRunbookRecursionError> {
        if self.execution_stack.contains(&sub_runbook_id) {
            return Err(SubRunbookRecursionError {
                runbook_id: sub_runbook_id,
                stack: self.execution_stack.clone(),
            });
        }

        let mut new_stack = self.execution_stack.clone();
        new_stack.push(sub_runbook_id.clone());

        Ok(Self {
            block_id: sub_runbook_block_id,
            runbook_id: self.runbook_id, // Keep parent runbook_id for event tracking
            document_handle: self.document_handle.clone(),
            context_resolver: sub_context_resolver,
            output_channel: self.output_channel.clone(),
            ssh_pool: self.ssh_pool.clone(),
            pty_store: self.pty_store.clone(),
            gc_event_bus: self.gc_event_bus.clone(),
            handle: ExecutionHandle::new(sub_runbook_block_id),
            execution_stack: new_stack,
            runbook_loader: self.runbook_loader.clone(),
        })
    }

    /// Configure this context for sub-runbook execution, inheriting output/events from parent
    /// but keeping this context's document_handle (for proper context isolation).
    ///
    /// Unlike `with_sub_runbook`, this preserves the document_handle so that
    /// active context updates go to the correct document.
    pub fn configure_for_sub_runbook(
        mut self,
        parent: &ExecutionContext,
        sub_runbook_id: String,
    ) -> Result<Self, SubRunbookRecursionError> {
        if parent.execution_stack.contains(&sub_runbook_id) {
            return Err(SubRunbookRecursionError {
                runbook_id: sub_runbook_id,
                stack: parent.execution_stack.clone(),
            });
        }

        let mut new_stack = parent.execution_stack.clone();
        new_stack.push(sub_runbook_id);

        self.output_channel = parent.output_channel.clone();
        self.gc_event_bus = parent.gc_event_bus.clone();
        self.execution_stack = new_stack;
        self.runbook_loader = parent.runbook_loader.clone();

        Ok(self)
    }

    /// Get the runbook content loader (if available)
    pub fn runbook_loader(&self) -> Option<&Arc<dyn RunbookContentLoader>> {
        self.runbook_loader.as_ref()
    }

    /// Get the SSH pool (if available)
    pub fn ssh_pool(&self) -> Option<SshPoolHandle> {
        self.ssh_pool.clone()
    }

    /// Get the PTY store (if available)
    pub fn pty_store(&self) -> Option<PtyStoreHandle> {
        self.pty_store.clone()
    }

    /// Get the block local value provider (for sharing with sub-runbooks)
    pub fn block_local_value_provider(&self) -> Option<Arc<dyn LocalValueProvider>> {
        self.document_handle.block_local_value_provider()
    }

    /// Create a new context with different resource handles
    pub fn with_resources(
        mut self,
        ssh_pool: Option<SshPoolHandle>,
        pty_store: Option<PtyStoreHandle>,
    ) -> Self {
        if let Some(pool) = ssh_pool {
            self.ssh_pool = Some(pool);
        }
        if let Some(store) = pty_store {
            self.pty_store = Some(store);
        }
        self
    }
}

/// Error when recursion is detected in sub-runbook execution
#[derive(Debug, Clone)]
pub struct SubRunbookRecursionError {
    pub runbook_id: String,
    pub stack: Vec<String>,
}

impl std::fmt::Display for SubRunbookRecursionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Recursion detected: runbook '{}' is already in execution stack: {:?}",
            self.runbook_id, self.stack
        )
    }
}

impl std::error::Error for SubRunbookRecursionError {}

/// Token for cancelling block execution
///
/// Provides a one-time channel-based cancellation mechanism.
/// When cancelled, the receiver end will be notified.
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
    /// Create a new cancellation token
    pub fn new() -> Self {
        Self::default()
    }

    /// Cancel the execution
    ///
    /// Sends a signal to the receiver end. This can only be called once.
    pub fn cancel(&self) {
        if let Ok(mut sender_guard) = self.sender.lock() {
            if let Some(sender) = sender_guard.take() {
                let _ = sender.send(()); // Ignore error if receiver already dropped
            }
        }
    }

    /// Take the receiver end of the cancellation token
    ///
    /// This can only be called once. Returns None if already taken.
    pub fn take_receiver(&self) -> Option<oneshot::Receiver<()>> {
        if let Ok(mut receiver_guard) = self.receiver.lock() {
            receiver_guard.take()
        } else {
            None
        }
    }
}

/// Handle for managing block execution lifecycle
///
/// Provides methods for tracking execution state, cancellation,
/// and prompt interactions.
#[derive(Clone)]
pub struct ExecutionHandle {
    /// Unique execution ID
    pub id: Uuid,
    /// ID of the block being executed
    #[allow(dead_code)] // Used for tracking but not currently accessed
    pub block_id: Uuid,
    /// Token for cancelling this execution
    pub cancellation_token: CancellationToken,
    /// Current execution status
    pub status: Arc<RwLock<ExecutionStatus>>,
    /// Optional output variable name
    pub output_variable: Option<String>,
    /// Callbacks for client prompt responses
    pub prompt_callbacks: Arc<Mutex<HashMap<Uuid, oneshot::Sender<ClientPromptResult>>>>,
    pub on_finish: (
        watch::Sender<Option<ExecutionResult>>,
        watch::Receiver<Option<ExecutionResult>>,
    ),
}

impl ExecutionHandle {
    /// Create a new execution handle for a block
    pub fn new(block_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            block_id,
            cancellation_token: CancellationToken::new(),
            status: Arc::new(RwLock::new(ExecutionStatus::Running)),
            output_variable: None,
            prompt_callbacks: Arc::new(Mutex::new(HashMap::new())),
            on_finish: watch::channel(None),
        }
    }

    pub async fn set_running(&self) {
        *self.status.write().await = ExecutionStatus::Running;
    }

    pub async fn set_success(&self) {
        *self.status.write().await = ExecutionStatus::Success;
    }

    pub async fn set_failed(&self, error: impl Into<String>) {
        *self.status.write().await = ExecutionStatus::Failed(error.into());
    }

    pub async fn set_cancelled(&self) {
        *self.status.write().await = ExecutionStatus::Cancelled;
    }

    pub fn finished_channel(&self) -> watch::Receiver<Option<ExecutionResult>> {
        self.on_finish.1.clone()
    }

    /// Wait for execution to complete and return the result
    ///
    /// This helper encapsulates the common pattern of waiting on a watch channel
    /// for execution completion. Returns `Success` if the channel closes unexpectedly.
    pub async fn wait_for_completion(&self) -> ExecutionResult {
        let mut finished_channel = self.finished_channel();
        loop {
            if finished_channel.changed().await.is_err() {
                // Channel closed without sending a result - treat as success
                return ExecutionResult::Success;
            }
            if let Some(result) = *finished_channel.borrow_and_update() {
                return result;
            }
        }
    }
}

/// Current status of block execution
#[derive(TS, Clone, Debug, Serialize, Deserialize, PartialEq)]
#[ts(tag = "type", content = "data", export)]
pub enum ExecutionStatus {
    Running,
    Success, // The output value
    #[allow(dead_code)] // Error message is used but compiler doesn't see reads
    Failed(String), // Error message
    #[allow(dead_code)] // Used for cancellation but not currently constructed in tests
    Cancelled,
}

/// Output from block execution
///
/// Can contain text output (stdout/stderr), binary data (for terminals),
/// or structured JSON objects.
#[derive(TS, Debug, Clone, Serialize, Deserialize, TypedBuilder)]
#[ts(export)]
pub struct StreamingBlockOutput {
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

/// Data for block finished lifecycle event
#[derive(TS, Debug, Clone, Serialize, Deserialize)]
#[ts(export)]
pub struct BlockFinishedData {
    pub exit_code: Option<i32>,
    pub success: bool,
}

/// Data for block error lifecycle event
#[derive(TS, Debug, Clone, Serialize, Deserialize)]
#[ts(export)]
pub struct BlockErrorData {
    pub message: String,
}

/// Block lifecycle events
///
/// Indicates state transitions during block execution.
#[derive(TS, Debug, Clone, Serialize, Deserialize)]
#[ts(tag = "type", content = "data", export)]
#[serde(rename_all = "camelCase", tag = "type", content = "data")]
pub enum BlockLifecycleEvent {
    Started(Uuid),
    Finished(BlockFinishedData),
    Cancelled,
    Error(BlockErrorData),
    Paused,
}
