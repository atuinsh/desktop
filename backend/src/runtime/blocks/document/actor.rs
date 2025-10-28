use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::ipc::Channel;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::runtime::blocks::document::block_context::{BlockContext, ResolvedContext};
use crate::runtime::blocks::document::bridge::DocumentBridgeMessage;
use crate::runtime::blocks::document::Document;
use crate::runtime::blocks::handler::ExecutionContext;
use crate::runtime::blocks::Block;
use crate::runtime::events::EventBus;
use crate::runtime::ClientMessageChannel;

#[async_trait]
pub trait BlockLocalValueProvider: Send + Sync {
    async fn get_block_local_value(
        &self,
        block_id: Uuid,
        property_name: &str,
    ) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>>;
}

#[allow(unused)]
pub(crate) struct MemoryBlockLocalValueProvider {
    values: HashMap<String, String>,
}

#[allow(unused)]
impl MemoryBlockLocalValueProvider {
    pub fn new(values: Vec<(String, String)>) -> Self {
        Self {
            values: values.into_iter().collect(),
        }
    }
}

#[async_trait]
impl BlockLocalValueProvider for MemoryBlockLocalValueProvider {
    async fn get_block_local_value(
        &self,
        _block_id: Uuid,
        property_name: &str,
    ) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.values.get(property_name).cloned())
    }
}

/// Errors that can occur during document operations
#[derive(thiserror::Error, Debug, Clone)]
pub enum DocumentError {
    #[error("Block not found: {0}")]
    BlockNotFound(Uuid),

    #[error("Failed to send command to document actor")]
    ActorSendError,

    #[error("Failed to evaluate passive context: {0}")]
    PassiveContextError(String),

    #[error("Invalid document structure: {0}")]
    InvalidStructure(String),
}

impl<T> From<mpsc::error::SendError<T>> for DocumentError {
    fn from(_: mpsc::error::SendError<T>) -> Self {
        DocumentError::ActorSendError
    }
}

pub type Reply<T> = oneshot::Sender<Result<T, DocumentError>>;

/// Commands that can be sent to the document actor
pub enum DocumentCommand {
    UpdateDocument {
        document: Vec<serde_json::Value>,
        reply: Reply<()>,
    },

    /// Notify the document actor that a block's local value has changed
    BlockLocalValueChanged { block_id: Uuid, reply: Reply<()> },

    /// Update the bridge channel for the document
    UpdateBridgeChannel {
        document_bridge: Box<dyn ClientMessageChannel<DocumentBridgeMessage>>,
        reply: Reply<()>,
    },

    /// Start execution of a block, returning a snapshot of its context
    StartExecution {
        block_id: Uuid,
        output_channel: Option<
            Arc<
                dyn crate::runtime::ClientMessageChannel<
                    crate::runtime::blocks::handler::BlockOutput,
                >,
            >,
        >,
        event_sender:
            tokio::sync::broadcast::Sender<crate::runtime::workflow::event::WorkflowEvent>,
        ssh_pool: Option<crate::runtime::ssh_pool::SshPoolHandle>,
        pty_store: Option<crate::runtime::pty_store::PtyStoreHandle>,
        reply: Reply<ExecutionContext>,
    },

    /// Complete execution of a block, updating its context
    CompleteExecution {
        block_id: Uuid,
        context: BlockContext,
        reply: Reply<()>,
    },

    /// Update a block's context during execution (intermediate updates)
    UpdateContext {
        block_id: Uuid,
        update_fn: Box<dyn FnOnce(&mut BlockContext) + Send>,
        reply: Reply<()>,
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

    /// Shutdown the document actor
    Shutdown,
}

/// Handle for interacting with a document actor
/// This is the public API for document operations
#[derive(Clone)]
pub struct DocumentHandle {
    runbook_id: String,
    command_tx: mpsc::UnboundedSender<DocumentCommand>,
}

impl DocumentHandle {
    /// Create a new document handle and spawn its actor
    pub fn new(
        runbook_id: String,
        event_bus: Arc<dyn EventBus>,
        document_bridge: Channel<DocumentBridgeMessage>, // todo: don't use Channel directly
        block_local_value_provider: Option<Box<dyn BlockLocalValueProvider>>,
    ) -> Self {
        let document_bridge = Box::new(document_bridge);
        let (tx, rx) = mpsc::unbounded_channel();

        // Clone tx for the actor
        let tx_clone = tx.clone();
        let runbook_id_clone = runbook_id.clone();

        // Spawn the document actor
        tokio::spawn(async move {
            let mut actor = DocumentActor::new(
                runbook_id_clone,
                event_bus,
                tx_clone,
                document_bridge,
                block_local_value_provider,
            );
            actor.run(rx).await;
        });

        Self {
            runbook_id,
            command_tx: tx,
        }
    }

    pub fn from_raw(
        runbook_id: String,
        command_tx: mpsc::UnboundedSender<DocumentCommand>,
    ) -> Self {
        Self {
            runbook_id,
            command_tx,
        }
    }

    /// Get the runbook ID this document handle is for
    #[allow(unused)]
    pub fn runbook_id(&self) -> &str {
        &self.runbook_id
    }

    pub async fn update_bridge_channel(
        &self,
        document_bridge: Box<dyn ClientMessageChannel<DocumentBridgeMessage>>,
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
    pub async fn start_execution(
        &self,
        block_id: Uuid,
        output_channel: Option<
            Arc<
                dyn crate::runtime::ClientMessageChannel<
                    crate::runtime::blocks::handler::BlockOutput,
                >,
            >,
        >,
        event_sender: tokio::sync::broadcast::Sender<
            crate::runtime::workflow::event::WorkflowEvent,
        >,
        ssh_pool: Option<crate::runtime::ssh_pool::SshPoolHandle>,
        pty_store: Option<crate::runtime::pty_store::PtyStoreHandle>,
    ) -> Result<ExecutionContext, DocumentError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::StartExecution {
                block_id,
                output_channel,
                event_sender,
                ssh_pool,
                pty_store,
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

    /// Update a block's context during execution
    pub async fn update_context<F>(&self, block_id: Uuid, update_fn: F) -> Result<(), DocumentError>
    where
        F: FnOnce(&mut BlockContext) + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::UpdateContext {
                block_id,
                update_fn: Box::new(update_fn),
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
}

impl Drop for DocumentHandle {
    fn drop(&mut self) {
        // Send shutdown command on drop (fire and forget)
        let _ = self.shutdown();
    }
}

/// The document actor that owns the document state and processes commands
struct DocumentActor {
    document: Document,
    event_bus: Arc<dyn EventBus>,
    command_tx: mpsc::UnboundedSender<DocumentCommand>,
}

impl DocumentActor {
    fn new(
        runbook_id: String,
        event_bus: Arc<dyn EventBus>,
        command_tx: mpsc::UnboundedSender<DocumentCommand>,
        document_bridge: Box<dyn ClientMessageChannel<DocumentBridgeMessage>>,
        block_local_value_provider: Option<Box<dyn BlockLocalValueProvider>>,
    ) -> Self {
        let document = Document::new(
            runbook_id,
            vec![],
            document_bridge,
            command_tx.clone(),
            block_local_value_provider,
        )
        .unwrap();

        Self {
            document,
            event_bus,
            command_tx,
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
                DocumentCommand::StartExecution {
                    block_id,
                    output_channel,
                    event_sender,
                    ssh_pool,
                    pty_store,
                    reply,
                } => {
                    let result = self
                        .handle_start_execution(
                            block_id,
                            output_channel,
                            event_sender,
                            ssh_pool,
                            pty_store,
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
                DocumentCommand::UpdateContext {
                    block_id,
                    update_fn,
                    reply,
                } => {
                    let result = self.handle_update_context(block_id, update_fn).await;
                    let _ = reply.send(result);
                }
                DocumentCommand::GetResolvedContext { block_id, reply } => {
                    let context = self.document.get_resolved_context(&block_id);
                    let _ = reply.send(context);
                }
                DocumentCommand::GetBlock { block_id, reply } => {
                    let block = self
                        .document
                        .get_block(&block_id)
                        .map(|b| b.block().clone());
                    let _ = reply.send(block);
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
        log::trace!("Updating document {} with new content", self.document.id);
        // Update the document using put_document, which returns the index to rebuild from
        let rebuild_from = self
            .document
            .put_document(document)
            .map_err(|e| DocumentError::InvalidStructure(e.to_string()))?;

        // Rebuild passive contexts only for affected blocks
        if let Some(start_index) = rebuild_from {
            let result = self
                .document
                .rebuild_passive_contexts(Some(start_index), self.event_bus.clone())
                .await;

            if let Err(errors) = result {
                // Log errors but don't fail the entire operation
                for error in errors {
                    log::error!("Error rebuilding passive context: {:?}", error);
                }
            }
        }

        Ok(())
    }

    async fn handle_start_execution(
        &mut self,
        block_id: Uuid,
        output_channel: Option<
            Arc<
                dyn crate::runtime::ClientMessageChannel<
                    crate::runtime::blocks::handler::BlockOutput,
                >,
            >,
        >,
        event_sender: tokio::sync::broadcast::Sender<
            crate::runtime::workflow::event::WorkflowEvent,
        >,
        ssh_pool: Option<crate::runtime::ssh_pool::SshPoolHandle>,
        pty_store: Option<crate::runtime::pty_store::PtyStoreHandle>,
    ) -> Result<ExecutionContext, DocumentError> {
        // Build execution context from current document state
        let context = self.document.build_execution_context(
            &block_id,
            self.command_tx.clone(),
            self.event_bus.clone(),
            output_channel,
            event_sender,
            ssh_pool,
            pty_store,
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

        block.update_context(context);
        Ok(())
    }

    async fn handle_update_context(
        &mut self,
        block_id: Uuid,
        update_fn: Box<dyn FnOnce(&mut BlockContext) + Send>,
    ) -> Result<(), DocumentError> {
        // Apply the update function to the block's context
        let block = self
            .document
            .get_block_mut(&block_id)
            .ok_or(DocumentError::BlockNotFound(block_id))?;

        update_fn(block.context_mut());
        Ok(())
    }

    async fn handle_block_local_value_changed(
        &mut self,
        block_id: Uuid,
    ) -> Result<(), DocumentError> {
        log::trace!(
            "Block local value changed for block {block_id} in document {}",
            self.document.id
        );
        let rebuild_from = self.document.block_local_value_changed(block_id)?;
        log::trace!("Rebuilding document from index {rebuild_from}");

        // Rebuild passive contexts only for affected blocks
        let result = self
            .document
            .rebuild_passive_contexts(Some(rebuild_from), self.event_bus.clone())
            .await;

        if let Err(errors) = result {
            // Log errors but don't fail the entire operation
            for error in errors {
                log::error!("Error rebuilding passive context: {:?}", error);
            }
        }

        Ok(())
    }
}
