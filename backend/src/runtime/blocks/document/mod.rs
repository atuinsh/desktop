use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::runtime::blocks::document::block_context::{
    BlockContext, BlockWithContext, DocumentCwd, DocumentEnvVar, DocumentSshHost, DocumentVar,
};
use crate::runtime::blocks::document::document::Document;
use crate::runtime::blocks::document::document_context::DocumentExecutionView;
use crate::runtime::blocks::Block;
use crate::runtime::events::EventBus;

pub mod block_context;
pub mod document;
pub mod document_context;

/// Errors that can occur during document operations
#[derive(thiserror::Error, Debug, Clone)]
pub enum DocumentError {
    #[error("Block not found: {0}")]
    BlockNotFound(Uuid),

    #[error("Document not found: {0}")]
    DocumentNotFound(String),

    #[error("Failed to send command to document actor")]
    ActorSendError,

    #[error("Failed to parse block: {0}")]
    BlockParseError(String),

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

    /// Start execution of a block, returning a snapshot of its context
    StartExecution {
        block_id: Uuid,
        reply: Reply<DocumentExecutionView>,
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
    pub fn new(runbook_id: String, event_bus: Arc<dyn EventBus>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        // Clone tx for the actor
        let tx_clone = tx.clone();
        let runbook_id_clone = runbook_id.clone();

        // Spawn the document actor
        tokio::spawn(async move {
            let mut actor = DocumentActor::new(runbook_id_clone, event_bus, tx_clone);
            actor.run(rx).await;
        });

        Self {
            runbook_id,
            command_tx: tx,
        }
    }

    /// Get the runbook ID this document handle is for
    pub fn runbook_id(&self) -> &str {
        &self.runbook_id
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
    ) -> Result<DocumentExecutionView, DocumentError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::StartExecution {
                block_id,
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

    /// Get a block by ID (for debugging/inspection)
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

    /// Shutdown the document actor
    pub async fn shutdown(&self) -> Result<(), DocumentError> {
        self.command_tx
            .send(DocumentCommand::Shutdown)
            .map_err(|_| DocumentError::ActorSendError)?;
        Ok(())
    }
}

impl Drop for DocumentHandle {
    fn drop(&mut self) {
        // Send shutdown command on drop (fire and forget)
        let _ = self.command_tx.send(DocumentCommand::Shutdown);
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
    ) -> Self {
        let document = Document::new(runbook_id, vec![]).unwrap();

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
                DocumentCommand::StartExecution { block_id, reply } => {
                    let result = self.handle_start_execution(block_id).await;
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
        // Update the document using put_document, which returns the index to rebuild from
        let rebuild_from = self
            .document
            .put_document(document)
            .map_err(|e| DocumentError::InvalidStructure(e.to_string()))?;

        // Rebuild passive contexts only for affected blocks
        if let Some(start_index) = rebuild_from {
            let errors = self
                .document
                .rebuild_passive_contexts(Some(start_index), &self.event_bus);

            if !errors.is_empty() {
                // Log errors but don't fail the entire operation
                for error in errors {
                    eprintln!("Error rebuilding passive context: {:?}", error);
                }
            }
        }

        Ok(())
    }

    async fn handle_start_execution(
        &mut self,
        block_id: Uuid,
    ) -> Result<DocumentExecutionView, DocumentError> {
        // Build execution view from current document state
        let view = self.document.build_execution_view(
            &block_id,
            self.command_tx.clone(),
            self.event_bus.clone(),
        )?;
        Ok(view)
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
            .ok_or_else(|| DocumentError::BlockNotFound(block_id))?;

        block.replace_context(context);
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
            .ok_or_else(|| DocumentError::BlockNotFound(block_id))?;

        update_fn(block.context_mut());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_get_var_map() {
        let mut doc = Document::new(
            "test".into(),
            vec![
                json!({
                    "id": "block1",
                    "type": "var",
                    "name": "foo",
                    "value": "bar"
                }),
                json!({
                    "id": "block2",
                    "type": "var",
                    "name": "hello",
                    "value": "world"
                }),
                json!({
                    "id": "block3",
                    "type": "var",
                    "name": "foo",
                    "value": "baz" // Overwrites foo=bar
                }),
                json!({
                    "id": "block4",
                    "type": "host"
                }),
            ],
        )
        .unwrap();

        let context = doc
            .context_for(&Uuid::parse_str("block4").unwrap())
            .unwrap();
        let var_map = context.get_var_map();

        assert_eq!(var_map.len(), 2);
        assert_eq!(var_map.get("foo").unwrap(), "baz"); // Latest value
        assert_eq!(var_map.get("hello").unwrap(), "world");
    }
}
