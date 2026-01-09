use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::blocks::Block;
use crate::client::{
    DocumentBridgeMessage, LocalValueProvider, MessageChannel, RunbookContentLoader,
};
use crate::context::{
    BlockContext, BlockContextStorage, BlockExecutionOutput, BlockState, BlockStateUpdater,
    ContextResolver, ResolvedContext,
};
use crate::document::Document;
use crate::events::EventBus;
use crate::execution::ExecutionContext;
use crate::pty::PtyStoreHandle;
use crate::ssh::SshPoolHandle;

/// Errors that can occur during document operations
#[derive(thiserror::Error, Debug, Clone)]
pub enum DocumentError {
    #[error("Block not found: {0}")]
    BlockNotFound(Uuid),

    #[error("Failed to send command to document actor")]
    ActorSendError,

    #[error("Failed to emit event")]
    EventSendError,

    #[error("Failed to send output")]
    OutputSendError,

    #[error("Failed to evaluate passive context: {0}")]
    PassiveContextError(String),

    #[error("Invalid document structure: {0}")]
    InvalidStructure(String),

    #[error("Invalid runbook ID: {0}")]
    InvalidRunbookId(String),

    #[error("Failed to store active context: {0}")]
    StoreActiveContextError(String),

    #[error("Failed to serialize block state: {0}")]
    StateSerializationError(String),

    #[error("Failed to serialize block execution output: {0}")]
    ExecutionOutputSerializationError(String),

    #[error("Failed to downcast block execution output")]
    ExecutionOutputDowncastError,
}

impl<T> From<mpsc::error::SendError<T>> for DocumentError {
    fn from(_: mpsc::error::SendError<T>) -> Self {
        DocumentError::ActorSendError
    }
}

pub type Reply<T> = oneshot::Sender<Result<T, DocumentError>>;

/// Commands that can be sent to the document actor
pub(crate) enum DocumentCommand {
    UpdateDocument {
        document: Vec<serde_json::Value>,
        reply: Reply<()>,
    },

    /// Notify the document actor that a block's local value has changed
    BlockLocalValueChanged {
        block_id: Uuid,
        reply: Reply<()>,
    },

    /// Update the bridge channel for the document
    UpdateBridgeChannel {
        document_bridge: Arc<dyn MessageChannel<DocumentBridgeMessage>>,
        reply: Reply<()>,
    },

    /// Start execution of a block, returning a snapshot of its context
    CreateExecutionContext {
        block_id: Uuid,
        ssh_pool: Option<SshPoolHandle>,
        pty_store: Option<PtyStoreHandle>,
        extra_template_context: Option<HashMap<String, HashMap<String, String>>>,
        reply: Reply<ExecutionContext>,
    },

    /// Complete execution of a block, updating its context
    CompleteExecution {
        block_id: Uuid,
        context: BlockContext,
        reply: Reply<()>,
    },

    /// Update a block's passive context during execution
    UpdatePassiveContext {
        block_id: Uuid,
        update_fn: Box<dyn FnOnce(&mut BlockContext) + Send>,
        reply: Reply<()>,
    },

    /// Update a block's active context during execution
    UpdateActiveContext {
        block_id: Uuid,
        update_fn: Box<dyn FnOnce(&mut BlockContext) + Send>,
        reply: Reply<()>,
    },

    /// Update a block's state during execution
    UpdateBlockState {
        block_id: Uuid,
        update_fn: BlockStateUpdater,
        reply: Reply<()>,
    },

    /// Set a block's output during execution
    SetBlockExecutionOutput {
        block_id: Uuid,
        output: Box<dyn BlockExecutionOutput>,
        reply: Reply<()>,
    },

    /// Get all blocks
    GetBlocks {
        reply: Reply<Vec<Block>>,
    },

    /// Get a block by ID (for inspection/debugging)
    GetBlock {
        block_id: Uuid,
        reply: oneshot::Sender<Option<Block>>,
    },

    /// Get a flattened block context
    GetResolvedContext {
        block_id: Uuid,
        reply: oneshot::Sender<Result<ResolvedContext, DocumentError>>,
    },

    /// Get a block's state
    GetBlockState {
        block_id: Uuid,
        reply: oneshot::Sender<Result<Value, DocumentError>>,
    },

    /// Get a block's execution output
    GetBlockExecutionOutput {
        block_id: Uuid,
        reply: oneshot::Sender<Result<Option<Arc<dyn BlockExecutionOutput>>, DocumentError>>,
    },

    /// Get a block's execution output, serialized
    GetBlockExecutionOutputSerialized {
        block_id: Uuid,
        reply: oneshot::Sender<Result<Option<Value>, DocumentError>>,
    },

    ResetState {
        reply: Reply<()>,
    },

    /// Set parent context for sub-runbook execution
    SetParentContext {
        parent: Arc<ContextResolver>,
        reply: Reply<()>,
    },

    /// Get the current context resolver (includes all blocks + parent context)
    GetContextResolver {
        reply: oneshot::Sender<ContextResolver>,
    },

    /// Shutdown the document actor
    Shutdown,
}

/// Handle for interacting with a document actor
/// This is the public API for document operations
#[derive(Clone)]
pub struct DocumentHandle {
    runbook_id: String,
    command_tx: mpsc::UnboundedSender<DocumentCommand>,
    event_bus: Arc<dyn EventBus>,
    block_local_value_provider: Option<Arc<dyn LocalValueProvider>>,
}

impl DocumentHandle {
    /// Create a new document handle and spawn its actor
    pub fn new(
        runbook_id: String,
        event_bus: Arc<dyn EventBus>,
        document_bridge: Arc<dyn MessageChannel<DocumentBridgeMessage>>,
        block_local_value_provider: Option<Arc<dyn LocalValueProvider>>,
        context_storage: Option<Box<dyn BlockContextStorage>>,
        runbook_loader: Option<Arc<dyn RunbookContentLoader>>,
        workspace_root: Option<String>,
    ) -> Arc<Self> {
        let (tx, rx) = mpsc::unbounded_channel();

        let instance = Arc::new(Self {
            runbook_id: runbook_id.clone(),
            command_tx: tx.clone(),
            event_bus: event_bus.clone(),
            block_local_value_provider: block_local_value_provider.clone(),
        });

        // Spawn the document actor
        tracing::trace!(
            "Spawning document actor for runbook {runbook_id}",
            runbook_id = runbook_id
        );
        let instance_clone = instance.clone();
        tokio::spawn(async move {
            let mut actor = DocumentActor::new(
                runbook_id,
                event_bus,
                document_bridge,
                block_local_value_provider,
                context_storage,
                runbook_loader,
                workspace_root,
                instance_clone,
            )
            .await;
            actor.run(rx).await;
        });

        instance
    }

    #[cfg(test)]
    pub(crate) fn from_raw(
        runbook_id: String,
        command_tx: mpsc::UnboundedSender<DocumentCommand>,
        event_bus: Arc<dyn EventBus>,
    ) -> Arc<Self> {
        Arc::new(Self {
            runbook_id,
            command_tx,
            event_bus,
            block_local_value_provider: None,
        })
    }

    /// Get the block local value provider for sharing with sub-runbooks
    pub fn block_local_value_provider(&self) -> Option<Arc<dyn LocalValueProvider>> {
        self.block_local_value_provider.clone()
    }

    /// Get the runbook ID this document handle is for
    #[allow(unused)]
    pub fn runbook_id(&self) -> &str {
        &self.runbook_id
    }

    pub fn event_bus(&self) -> Arc<dyn EventBus> {
        self.event_bus.clone()
    }

    pub async fn update_bridge_channel(
        &self,
        document_bridge: Arc<dyn MessageChannel<DocumentBridgeMessage>>,
    ) -> Result<(), DocumentError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::UpdateBridgeChannel {
                document_bridge,
                reply: tx,
            })
            .map_err(|_| DocumentError::ActorSendError)?;
        rx.await.map_err(|_| DocumentError::ActorSendError)?
    }

    /// Update the entire document from BlockNote
    pub async fn put_document(
        &self,
        document: Vec<serde_json::Value>,
    ) -> Result<(), DocumentError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::UpdateDocument {
                document,
                reply: tx,
            })
            .map_err(|_| DocumentError::ActorSendError)?;
        rx.await.map_err(|_| DocumentError::ActorSendError)?
    }

    /// Start execution of a block, returning a snapshot of its context
    pub async fn create_execution_context(
        &self,
        block_id: Uuid,
        ssh_pool: Option<SshPoolHandle>,
        pty_store: Option<PtyStoreHandle>,
        extra_template_context: Option<HashMap<String, HashMap<String, String>>>,
    ) -> Result<ExecutionContext, DocumentError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::CreateExecutionContext {
                block_id,
                ssh_pool,
                pty_store,
                extra_template_context,
                reply: tx,
            })
            .map_err(|_| DocumentError::ActorSendError)?;
        rx.await.map_err(|_| DocumentError::ActorSendError)?
    }

    /// Complete execution of a block, updating its final context
    pub async fn complete_execution(
        &self,
        block_id: Uuid,
        context: BlockContext,
    ) -> Result<(), DocumentError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::CompleteExecution {
                block_id,
                context,
                reply: tx,
            })
            .map_err(|_| DocumentError::ActorSendError)?;
        rx.await.map_err(|_| DocumentError::ActorSendError)?
    }

    /// Update a block's passive context during execution
    pub async fn update_passive_context<F>(
        &self,
        block_id: Uuid,
        update_fn: F,
    ) -> Result<(), DocumentError>
    where
        F: FnOnce(&mut BlockContext) + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::UpdatePassiveContext {
                block_id,
                update_fn: Box::new(update_fn),
                reply: tx,
            })
            .map_err(|_| DocumentError::ActorSendError)?;
        rx.await.map_err(|_| DocumentError::ActorSendError)?
    }

    /// Update a block's active context during execution
    pub async fn update_active_context<F>(
        &self,
        block_id: Uuid,
        update_fn: F,
    ) -> Result<(), DocumentError>
    where
        F: FnOnce(&mut BlockContext) + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::UpdateActiveContext {
                block_id,
                update_fn: Box::new(update_fn),
                reply: tx,
            })
            .map_err(|_| DocumentError::ActorSendError)?;
        rx.await.map_err(|_| DocumentError::ActorSendError)?
    }

    /// Update a block's state during execution
    pub async fn update_block_state<T: BlockState, F>(
        &self,
        block_id: Uuid,
        update_fn: F,
    ) -> Result<(), DocumentError>
    where
        F: FnOnce(&mut T) + Send + 'static,
    {
        let wrapped_fn: BlockStateUpdater = Box::new(move |state| {
            if let Some(state) = state.downcast_mut::<T>() {
                update_fn(state);
            }
        });

        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::UpdateBlockState {
                block_id,
                update_fn: wrapped_fn,
                reply: tx,
            })
            .map_err(|_| DocumentError::ActorSendError)?;
        rx.await.map_err(|_| DocumentError::ActorSendError)?
    }

    /// Set a block's output during execution
    pub async fn set_block_execution_output(
        &self,
        block_id: Uuid,
        output: impl BlockExecutionOutput,
    ) -> Result<(), DocumentError> {
        self.set_block_execution_output_boxed(block_id, Box::new(output))
            .await
    }

    /// Set a block's output during execution (boxed variant)
    pub async fn set_block_execution_output_boxed(
        &self,
        block_id: Uuid,
        output: Box<dyn BlockExecutionOutput>,
    ) -> Result<(), DocumentError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::SetBlockExecutionOutput {
                block_id,
                output,
                reply: tx,
            })
            .map_err(|_| DocumentError::ActorSendError)?;
        rx.await.map_err(|_| DocumentError::ActorSendError)?
    }

    /// Get a flattened block context
    pub async fn get_resolved_context(
        &self,
        block_id: Uuid,
    ) -> Result<ResolvedContext, DocumentError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::GetResolvedContext {
                block_id,
                reply: tx,
            })
            .map_err(|_| DocumentError::ActorSendError)?;
        rx.await.map_err(|_| DocumentError::ActorSendError)?
    }

    /// Get a block's state
    pub async fn get_block_state(&self, block_id: Uuid) -> Result<Value, DocumentError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::GetBlockState {
                block_id,
                reply: tx,
            })
            .map_err(|_| DocumentError::ActorSendError)?;
        rx.await.map_err(|_| DocumentError::ActorSendError)?
    }

    /// Get a block's execution output as a specific type
    ///
    /// This call creates a clone of the block's execution output.
    pub async fn get_block_execution_output<T: BlockExecutionOutput>(
        &self,
        block_id: Uuid,
    ) -> Result<Option<Arc<T>>, DocumentError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::GetBlockExecutionOutput {
                block_id,
                reply: tx,
            })
            .map_err(|_| DocumentError::ActorSendError)?;
        match rx.await.map_err(|_| DocumentError::ActorSendError)? {
            Ok(Some(output)) => match downcast_arc::<T>(output) {
                Ok(output) => Ok(Some(output)),
                Err(_) => Err(DocumentError::ExecutionOutputDowncastError),
            },
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Get a block's execution output as a serialized value
    ///
    /// If all you want to do with the output is serialize it, this saves a clone
    /// over `get_block_execution_output`.
    pub async fn get_block_execution_output_serialized(
        &self,
        block_id: Uuid,
    ) -> Result<Option<Value>, DocumentError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::GetBlockExecutionOutputSerialized {
                block_id,
                reply: tx,
            })
            .map_err(|_| DocumentError::ActorSendError)?;
        rx.await.map_err(|_| DocumentError::ActorSendError)?
    }

    /// Get all blocks
    pub async fn blocks(&self) -> Result<Vec<Block>, DocumentError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::GetBlocks { reply: tx })
            .map_err(|_| DocumentError::ActorSendError)?;
        rx.await.map_err(|_| DocumentError::ActorSendError)?
    }

    /// Get a block by ID (for debugging/inspection)
    #[allow(unused)]
    pub async fn get_block(&self, block_id: Uuid) -> Option<Block> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::GetBlock {
                block_id,
                reply: tx,
            })
            .ok()?;
        rx.await.ok()?
    }

    /// Update the document with a new document snapshot
    pub async fn update_document(
        &self,
        document: Vec<serde_json::Value>,
    ) -> Result<(), DocumentError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::UpdateDocument {
                document,
                reply: tx,
            })
            .map_err(|_| DocumentError::ActorSendError)?;
        rx.await.map_err(|_| DocumentError::ActorSendError)?
    }

    /// Notify the document actor that a block's local value has changed
    pub async fn block_local_value_changed(&self, block_id: Uuid) -> Result<(), DocumentError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::BlockLocalValueChanged {
                block_id,
                reply: tx,
            })
            .map_err(|_| DocumentError::ActorSendError)?;
        rx.await.map_err(|_| DocumentError::ActorSendError)?
    }

    /// Shutdown the document actor
    pub fn shutdown(&self) -> Result<(), DocumentError> {
        self.command_tx
            .send(DocumentCommand::Shutdown)
            .map_err(|_| DocumentError::ActorSendError)?;
        Ok(())
    }

    /// Reset the document state
    pub async fn reset_state(&self) -> Result<(), DocumentError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::ResetState { reply: tx })
            .map_err(|_| DocumentError::ActorSendError)?;
        rx.await.map_err(|_| DocumentError::ActorSendError)?
    }

    /// Set parent context for sub-runbook execution
    /// This makes the document inherit vars, env_vars, cwd, ssh_host from the parent
    pub async fn set_parent_context(
        &self,
        parent: Arc<ContextResolver>,
    ) -> Result<(), DocumentError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::SetParentContext { parent, reply: tx })
            .map_err(|_| DocumentError::ActorSendError)?;
        rx.await.map_err(|_| DocumentError::ActorSendError)?
    }

    /// Get the current context resolver (includes all blocks + parent context)
    /// This is useful for extracting env vars after sub-runbook execution
    pub async fn get_context_resolver(&self) -> Result<ContextResolver, DocumentError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::GetContextResolver { reply: tx })
            .map_err(|_| DocumentError::ActorSendError)?;
        rx.await.map_err(|_| DocumentError::ActorSendError)
    }
}

impl Drop for DocumentHandle {
    fn drop(&mut self) {
        tracing::trace!(
            "Shutting down document actor for runbook {runbook_id}",
            runbook_id = self.runbook_id
        );
        // Send shutdown command on drop (fire and forget)
        let _ = self.shutdown();
    }
}

/// The document actor that owns the document state and processes commands
struct DocumentActor {
    document: Document,
    event_bus: Arc<dyn EventBus>,
    handle: Arc<DocumentHandle>,
}

impl DocumentActor {
    #[allow(clippy::too_many_arguments)]
    async fn new(
        runbook_id: String,
        event_bus: Arc<dyn EventBus>,
        document_bridge: Arc<dyn MessageChannel<DocumentBridgeMessage>>,
        block_local_value_provider: Option<Arc<dyn LocalValueProvider>>,
        context_storage: Option<Box<dyn BlockContextStorage>>,
        runbook_loader: Option<Arc<dyn RunbookContentLoader>>,
        workspace_root: Option<String>,
        handle: Arc<DocumentHandle>,
    ) -> Self {
        let document = Document::new(
            runbook_id,
            vec![],
            document_bridge,
            block_local_value_provider,
            context_storage,
            runbook_loader,
            workspace_root,
        )
        .await
        .unwrap();

        Self {
            document,
            event_bus,
            handle,
        }
    }

    /// Main actor loop - processes commands sequentially
    async fn run(&mut self, mut rx: mpsc::UnboundedReceiver<DocumentCommand>) {
        while let Some(cmd) = rx.recv().await {
            match cmd {
                DocumentCommand::UpdateDocument { document, reply } => {
                    let result = self.handle_update_document(document).await;
                    let _ = reply.send(result);
                }
                DocumentCommand::BlockLocalValueChanged { block_id, reply } => {
                    let result = self.handle_block_local_value_changed(block_id).await;
                    let _ = reply.send(result);
                }
                DocumentCommand::UpdateBridgeChannel {
                    document_bridge,
                    reply,
                } => {
                    self.document.update_document_bridge(document_bridge);
                    let _ = reply.send(Ok(()));
                }
                DocumentCommand::CreateExecutionContext {
                    block_id,
                    ssh_pool,
                    pty_store,
                    extra_template_context,
                    reply,
                } => {
                    let result = self
                        .handle_create_execution_context(
                            block_id,
                            ssh_pool,
                            pty_store,
                            extra_template_context,
                        )
                        .await;
                    let _ = reply.send(result);
                }
                DocumentCommand::CompleteExecution {
                    block_id,
                    context,
                    reply,
                } => {
                    let result = self.handle_complete_execution(block_id, context).await;
                    let _ = reply.send(result);
                }
                DocumentCommand::UpdatePassiveContext {
                    block_id,
                    update_fn,
                    reply,
                } => {
                    let result = self
                        .handle_update_passive_context(block_id, update_fn)
                        .await;
                    let _ = reply.send(result);
                }
                DocumentCommand::UpdateActiveContext {
                    block_id,
                    update_fn,
                    reply,
                } => {
                    let result = self.handle_update_active_context(block_id, update_fn).await;
                    let _ = reply.send(result);
                }
                DocumentCommand::UpdateBlockState {
                    block_id,
                    update_fn,
                    reply,
                } => {
                    let result = self.handle_update_block_state(block_id, update_fn).await;
                    let _ = reply.send(result);
                }
                DocumentCommand::SetBlockExecutionOutput {
                    block_id,
                    output,
                    reply,
                } => {
                    let result = self
                        .handle_set_block_execution_output(block_id, output)
                        .await;
                    let _ = reply.send(result);
                }
                DocumentCommand::GetResolvedContext { block_id, reply } => {
                    let context = self.document.get_resolved_context(&block_id);
                    let _ = reply.send(context);
                }
                DocumentCommand::GetBlockState { block_id, reply } => {
                    let state = self.document.get_block_state(&block_id);
                    let _ = reply.send(state);
                }
                DocumentCommand::GetBlockExecutionOutput { block_id, reply } => {
                    let output = self.handle_get_block_execution_output(block_id).await;
                    let _ = reply.send(output);
                }
                DocumentCommand::GetBlockExecutionOutputSerialized { block_id, reply } => {
                    let output = self
                        .handle_get_block_execution_output_serialized(block_id)
                        .await;
                    let _ = reply.send(output);
                }
                DocumentCommand::GetBlocks { reply } => {
                    let blocks = self
                        .document
                        .blocks()
                        .iter()
                        .map(|b| b.block().clone())
                        .collect();
                    let _ = reply.send(Ok(blocks));
                }
                DocumentCommand::GetBlock { block_id, reply } => {
                    let block = self
                        .document
                        .get_block(&block_id)
                        .map(|b| b.block().clone());
                    let _ = reply.send(block);
                }
                DocumentCommand::ResetState { reply } => {
                    let result = self.handle_reset_state().await;
                    let _ = reply.send(result);
                }
                DocumentCommand::SetParentContext { parent, reply } => {
                    self.document.set_parent_context(parent);
                    let _ = reply.send(Ok(()));
                }
                DocumentCommand::GetContextResolver { reply } => {
                    let resolver = self.document.get_context_resolver();
                    let _ = reply.send(resolver);
                }
                DocumentCommand::Shutdown => {
                    break;
                }
            }
        }
    }

    async fn handle_update_document(
        &mut self,
        document: Vec<serde_json::Value>,
    ) -> Result<(), DocumentError> {
        tracing::trace!("Updating document {} with new content", self.document.id);
        // Update the document using put_document, which returns the index to rebuild from
        let rebuild_from = self
            .document
            .put_document(document)
            .await
            .map_err(|e| DocumentError::InvalidStructure(e.to_string()))?;

        // Rebuild passive contexts only for affected blocks
        if let Some(start_index) = rebuild_from {
            let result = self
                .document
                .rebuild_contexts(Some(start_index), self.event_bus.clone())
                .await;

            if let Err(errors) = result {
                // Log errors but don't fail the entire operation
                for error in errors {
                    tracing::error!("Error rebuilding passive context: {:?}", error);
                }
            }
        }

        Ok(())
    }

    async fn handle_create_execution_context(
        &mut self,
        block_id: Uuid,
        ssh_pool: Option<SshPoolHandle>,
        pty_store: Option<PtyStoreHandle>,
        extra_template_context: Option<HashMap<String, HashMap<String, String>>>,
    ) -> Result<ExecutionContext, DocumentError> {
        // Build execution context from current document state
        let context = self.document.build_execution_context(
            &block_id,
            self.handle.clone(),
            self.event_bus.clone(),
            ssh_pool,
            pty_store,
            extra_template_context,
        )?;
        Ok(context)
    }

    async fn handle_complete_execution(
        &mut self,
        block_id: Uuid,
        context: BlockContext,
    ) -> Result<(), DocumentError> {
        // Update the block's context with the final execution result
        let block = self
            .document
            .get_block_mut(&block_id)
            .ok_or(DocumentError::BlockNotFound(block_id))?;

        block.replace_passive_context(context);
        Ok(())
    }

    async fn handle_update_passive_context(
        &mut self,
        block_id: Uuid,
        update_fn: Box<dyn FnOnce(&mut BlockContext) + Send>,
    ) -> Result<(), DocumentError> {
        // Apply the update function to the block's passive context
        let block_index = self
            .document
            .get_block_index(&block_id)
            .ok_or(DocumentError::BlockNotFound(block_id))?;

        let block = self
            .document
            .get_block_mut_by_index(block_index)
            .ok_or(DocumentError::BlockNotFound(block_id))?;

        update_fn(block.passive_context_mut());

        let _ = self
            .document
            .rebuild_contexts(Some(block_index), self.event_bus.clone())
            .await;

        Ok(())
    }

    async fn handle_update_active_context(
        &mut self,
        block_id: Uuid,
        update_fn: Box<dyn FnOnce(&mut BlockContext) + Send>,
    ) -> Result<(), DocumentError> {
        // Apply the update function to the block's active context
        let block_index = self
            .document
            .get_block_index(&block_id)
            .ok_or(DocumentError::BlockNotFound(block_id))?;

        let block = self
            .document
            .get_block_mut_by_index(block_index)
            .ok_or(DocumentError::BlockNotFound(block_id))?;

        update_fn(block.active_context_mut());

        self.document.store_active_context(block_id).await?;

        let _ = self
            .document
            .rebuild_contexts(Some(block_index), self.event_bus.clone())
            .await;

        Ok(())
    }

    async fn handle_update_block_state(
        &mut self,
        block_id: Uuid,
        update_fn: BlockStateUpdater,
    ) -> Result<(), DocumentError> {
        tracing::trace!(
            "Updating block state for block {block_id} in document {doc_id}",
            doc_id = self.document.id
        );

        let state = {
            let block_index = self
                .document
                .get_block_index(&block_id)
                .ok_or(DocumentError::BlockNotFound(block_id))?;

            let block = self
                .document
                .get_block_mut_by_index(block_index)
                .ok_or(DocumentError::BlockNotFound(block_id))?;

            let changed = if let Some(state) = block.state_mut() {
                update_fn(state);
                true
            } else {
                false
            };

            if changed {
                let block = self
                    .document
                    .get_block(&block_id)
                    .ok_or(DocumentError::BlockNotFound(block_id))?;

                block.state()
            } else {
                None
            }
        };

        if let Some(state) = state {
            tracing::trace!("Emitting state changed for block {block_id}");
            let _ = self.document.emit_state_changed(block_id, state).await;
        }

        Ok(())
    }

    async fn handle_set_block_execution_output(
        &mut self,
        block_id: Uuid,
        output: Box<dyn BlockExecutionOutput>,
    ) -> Result<(), DocumentError> {
        tracing::trace!("Setting block execution output for block {block_id}");

        let block_index = self
            .document
            .get_block_index(&block_id)
            .ok_or(DocumentError::BlockNotFound(block_id))?;

        let block = self
            .document
            .get_block_mut_by_index(block_index)
            .ok_or(DocumentError::BlockNotFound(block_id))?;

        block.replace_execution_output(output);
        let _ = self
            .document
            .emit_block_execution_output_changed(block_id)
            .await;

        let _ = self
            .document
            .rebuild_contexts(Some(block_index), self.event_bus.clone())
            .await;

        Ok(())
    }

    async fn handle_get_block_execution_output(
        &mut self,
        block_id: Uuid,
    ) -> Result<Option<Arc<dyn BlockExecutionOutput>>, DocumentError> {
        let block = self
            .document
            .get_block(&block_id)
            .ok_or(DocumentError::BlockNotFound(block_id))?;
        Ok(block.execution_output())
    }

    async fn handle_get_block_execution_output_serialized(
        &mut self,
        block_id: Uuid,
    ) -> Result<Option<Value>, DocumentError> {
        let block = self
            .document
            .get_block(&block_id)
            .ok_or(DocumentError::BlockNotFound(block_id))?;

        if let Some(output) = block.execution_output() {
            let mut buf = Vec::new();
            let mut serializer = serde_json::Serializer::new(&mut buf);
            let mut erased = <dyn erased_serde::Serializer>::erase(&mut serializer);
            output
                .erased_serialize(&mut erased)
                .map_err(|e| DocumentError::ExecutionOutputSerializationError(e.to_string()))?;
            Ok(Some(serde_json::from_slice(&buf).map_err(|e| {
                DocumentError::ExecutionOutputSerializationError(e.to_string())
            })?))
        } else {
            Ok(None)
        }
    }

    async fn handle_block_local_value_changed(
        &mut self,
        block_id: Uuid,
    ) -> Result<(), DocumentError> {
        tracing::trace!(
            "Block local value changed for block {block_id} in document {}",
            self.document.id
        );
        let rebuild_from = self
            .document
            .get_block_index(&block_id)
            .ok_or(DocumentError::BlockNotFound(block_id))?;
        tracing::trace!("Rebuilding document from index {rebuild_from}");

        // Rebuild passive contexts only for affected blocks
        let result = self
            .document
            .rebuild_contexts(Some(rebuild_from), self.event_bus.clone())
            .await;

        if let Err(errors) = result {
            // Log errors but don't fail the entire operation
            for error in errors {
                tracing::error!("Error rebuilding passive context: {:?}", error);
            }
        }

        Ok(())
    }

    async fn handle_reset_state(&mut self) -> Result<(), DocumentError> {
        tracing::trace!("Resetting document state for document {}", self.document.id);
        self.document.reset_state().await?;

        let result = self
            .document
            .rebuild_contexts(None, self.event_bus.clone())
            .await;

        if let Err(errors) = result {
            for error in errors {
                tracing::error!("Error rebuilding passive context: {:?}", error);
            }
        }

        Ok(())
    }
}

/// Downcast an Arc<dyn BlockExecutionOutput> to Arc<T>
fn downcast_arc<T: BlockExecutionOutput>(
    arc: Arc<dyn BlockExecutionOutput>,
) -> Result<Arc<T>, Arc<dyn BlockExecutionOutput>> {
    if arc.as_any().is::<T>() {
        // SAFETY: We verified the type matches above via `is::<T>()`.
        // This assumes `BlockExecutionOutput::as_any()` is correctly implemented
        // to return `self` for the concrete implementing type.
        // The implementation here comes from the `downcast-rs` crate.
        let ptr = Arc::into_raw(arc) as *const T;
        Ok(unsafe { Arc::from_raw(ptr) })
    } else {
        Err(arc)
    }
}
