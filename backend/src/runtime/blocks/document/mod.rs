use minijinja::{Environment, Value};
use serde::{Deserialize, Serialize};
use std::any::{Any, TypeId};
use std::collections::{HashMap, HashSet};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::runtime::blocks::Block;
use crate::runtime::events::{EventBus, GCEvent};

/// A single block's context - can store multiple typed values
#[derive(Default)]
pub struct BlockContext {
    entries: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl BlockContext {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Insert a typed value into this block's context
    pub fn insert<T: Any + Send + Sync>(&mut self, value: T) {
        self.entries.insert(TypeId::of::<T>(), Box::new(value));
    }

    /// Get a typed value from this block's context
    pub fn get<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.entries
            .get(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast_ref::<T>())
    }

    /// Get a mutable typed value from this block's context
    pub fn get_mut<T: Any + Send + Sync>(&mut self) -> Option<&mut T> {
        self.entries
            .get_mut(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast_mut::<T>())
    }

    /// Remove a typed value from this block's context
    pub fn remove<T: Any + Send + Sync>(&mut self) -> Option<T> {
        self.entries
            .remove(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast::<T>().ok())
            .map(|boxed| *boxed)
    }

    /// Check if this block's context contains a value of type T
    pub fn contains<T: Any + Send + Sync>(&self) -> bool {
        self.entries.contains_key(&TypeId::of::<T>())
    }
}

pub struct BlockWithContext {
    block: Block,
    context: BlockContext,
}

impl BlockWithContext {
    pub fn new(block: Block, context: BlockContext) -> Self {
        Self { block, context }
    }

    pub fn id(&self) -> Uuid {
        self.block.id()
    }

    pub fn context(&self) -> &BlockContext {
        &self.context
    }

    pub fn context_mut(&mut self) -> &mut BlockContext {
        &mut self.context
    }

    pub fn replace_context(&mut self, context: BlockContext) {
        self.context = context;
    }
}

impl Deref for BlockWithContext {
    type Target = Block;

    fn deref(&self) -> &Self::Target {
        &self.block
    }
}

impl DerefMut for BlockWithContext {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.block
    }
}

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
            .send(DocumentCommand::UpdateDocument { document, reply: tx })
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
        let document = Document {
            id: runbook_id,
            blocks: Vec::new(),
        };

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
                    let block = self.document.get_block(&block_id).map(|b| b.block.clone());
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
        let rebuild_from = self.document
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

/// Document-level context containing all block contexts
/// This is the internal state owned by the DocumentActor
pub struct Document {
    id: String,
    blocks: Vec<BlockWithContext>,
}

impl Document {
    pub fn new(
        id: String,
        document: Vec<serde_json::Value>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut doc = Self { id, blocks: vec![] };
        doc.put_document(document)?;

        // let blocks = Self::flatten_document(&document)?;

        // Ok(Self { id, blocks })

        Ok(doc)
    }

    pub fn put_document(
        &mut self,
        document: Vec<serde_json::Value>,
    ) -> Result<Option<usize>, Box<dyn std::error::Error>> {
        let new_blocks = Self::flatten_document(&document)?;

        // Capture old state for change detection
        let old_block_ids: Vec<Uuid> = self.blocks.iter().map(|b| b.id()).collect();

        // Build a map of existing blocks by ID for quick lookup
        let mut existing_blocks_map: HashMap<Uuid, BlockWithContext> = self.blocks
            .drain(..)
            .map(|b| (b.id(), b))
            .collect();

        // Track which blocks need context rebuild
        let mut rebuild_from_index: Option<usize> = None;

        // Single pass: Build the final block list in the correct order
        let mut updated_blocks = Vec::with_capacity(new_blocks.len());

        for (new_index, new_block) in new_blocks.iter().enumerate() {
            if let Some(mut existing) = existing_blocks_map.remove(&new_block.id()) {
                // Block exists - check if content changed or position moved
                let content_changed = &existing.block != new_block;
                let old_index = old_block_ids.iter().position(|id| id == &new_block.id());
                let position_changed = old_index != Some(new_index);

                if content_changed {
                    existing.block = new_block.clone();
                }

                if content_changed || position_changed {
                    rebuild_from_index = Some(match rebuild_from_index {
                        Some(existing_idx) => std::cmp::min(existing_idx, new_index),
                        None => new_index,
                    });
                }

                updated_blocks.push(existing);
            } else {
                // New block - create it
                let block_with_context = BlockWithContext::new(new_block.clone(), BlockContext::new());
                updated_blocks.push(block_with_context);

                // Mark rebuild from this position
                rebuild_from_index = Some(match rebuild_from_index {
                    Some(existing) => std::cmp::min(existing, new_index),
                    None => new_index,
                });
            }
        }

        // Any remaining blocks in existing_blocks_map were deleted
        if !existing_blocks_map.is_empty() {
            // Find the minimum position where a deletion occurred
            for deleted_id in existing_blocks_map.keys() {
                if let Some(old_index) = old_block_ids.iter().position(|id| id == deleted_id) {
                    rebuild_from_index = Some(match rebuild_from_index {
                        Some(existing) => std::cmp::min(existing, old_index),
                        None => old_index,
                    });
                }
            }
        }

        self.blocks = updated_blocks;

        // Return the index from which to rebuild contexts
        Ok(rebuild_from_index)
    }

    /// Flatten the nested document structure into a flat list
    fn flatten_document(
        document: &[serde_json::Value],
    ) -> Result<Vec<Block>, Box<dyn std::error::Error>> {
        let mut doc_blocks = Vec::new();
        Self::flatten_recursive(document, &mut doc_blocks)?;
        let blocks = doc_blocks
            .iter()
            .filter_map(|value| Block::from_document(value).ok())
            .collect();
        Ok(blocks)
    }

    fn flatten_recursive(
        nodes: &[serde_json::Value],
        out: &mut Vec<serde_json::Value>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for node in nodes {
            out.push(node.clone());

            if let Some(children) = node.get("children").and_then(|v| v.as_array()) {
                Self::flatten_recursive(children, out)?;
            }
        }

        Ok(())
    }

    pub fn context_for(&mut self, block_id: &Uuid) -> Option<DocumentContext> {
        if self.has_block(block_id) {
            return Some(DocumentContext::new(self, *block_id));
        }

        None
    }

    /// Check if the document contains a block with the given ID
    pub fn has_block(&self, block_id: &Uuid) -> bool {
        self.blocks.iter().any(|block| &block.id() == block_id)
    }

    /// Get a block's context
    pub fn get_block(&self, block_id: &Uuid) -> Option<&BlockWithContext> {
        self.blocks.iter().find(|block| &block.id() == block_id)
    }

    /// Get a mutable reference to a block
    pub fn get_block_mut(&mut self, block_id: &Uuid) -> Option<&mut BlockWithContext> {
        self.blocks.iter_mut().find(|block| &block.id() == block_id)
    }

    /// Get a value of type T from the first block above current_block_id that has it
    ///
    /// This searches backwards from the current block through all blocks above it
    /// until it finds one that contains a value of type T.
    pub fn get_context_above<T: Any + Send + Sync>(&self, current_block_id: &Uuid) -> Option<&T> {
        let current_idx = self
            .blocks
            .iter()
            .position(|block| &block.id() == current_block_id)?;

        // Iterate through blocks above current block (in reverse order, closest first)
        for block in self.blocks[..current_idx].iter().rev() {
            if let Some(value) = block.context().get::<T>() {
                return Some(value);
            }
        }

        None
    }

    /// Get all values of type T from blocks above current_block_id
    ///
    /// Returns values in order from closest to furthest from current block.
    pub fn get_all_context_above<T: Any + Send + Sync>(&self, current_block_id: &Uuid) -> Vec<&T> {
        let current_idx = match self
            .blocks
            .iter()
            .position(|block| &block.id() == current_block_id)
        {
            Some(idx) => idx,
            None => return Vec::new(),
        };

        let mut results = Vec::new();

        // Iterate through blocks above current block (in reverse order, closest first)
        for block in self.blocks[..current_idx].iter().rev() {
            if let Some(value) = block.context().get::<T>() {
                results.push(value);
            }
        }

        results
    }

    /// Collect context from all blocks above current_block_id, merging them
    ///
    /// This is useful for types that can be merged (like variables or environment maps).
    /// The collector function receives values from furthest to closest, allowing
    /// closer blocks to override values from further blocks.
    pub fn collect_context_above<T, R, F>(
        &self,
        current_block_id: &Uuid,
        init: R,
        mut collector: F,
    ) -> R
    where
        T: Any + Send + Sync,
        F: FnMut(R, &T) -> R,
    {
        // Get all context values in order from closest to furthest
        let values = self.get_all_context_above::<T>(current_block_id);

        // Reverse to process furthest first, then fold with collector
        values
            .into_iter()
            .rev()
            .fold(init, |acc, value| collector(acc, value))
    }

    /// Build an execution view for a block, capturing all context from blocks above it
    pub fn build_execution_view(
        &self,
        block_id: &Uuid,
        command_tx: mpsc::UnboundedSender<DocumentCommand>,
        event_bus: Arc<dyn EventBus>,
    ) -> Result<DocumentExecutionView, DocumentError> {
        let block = self
            .get_block(block_id)
            .ok_or_else(|| DocumentError::BlockNotFound(*block_id))?;

        // Collect variables
        let vars = self.collect_context_above::<DocumentVar, HashMap<String, String>, _>(
            block_id,
            HashMap::new(),
            |mut acc, var| {
                acc.insert(var.0.clone(), var.1.clone());
                acc
            },
        );

        // Get current working directory (use last one set, or default)
        let cwd = self
            .get_context_above::<DocumentCwd>(block_id)
            .map(|cwd| cwd.0.clone())
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            });

        // Collect environment variables
        let env_vars = self.collect_context_above::<DocumentEnvVar, HashMap<String, String>, _>(
            block_id,
            HashMap::new(),
            |mut acc, env| {
                acc.insert(env.0.clone(), env.1.clone());
                acc
            },
        );

        // Get SSH host (use most recent)
        let ssh_host = self
            .get_context_above::<DocumentSshHost>(block_id)
            .and_then(|host| host.0.clone());

        Ok(DocumentExecutionView {
            block_id: *block_id,
            block: block.block.clone(),
            runbook_id: Uuid::parse_str(&self.id).unwrap_or_else(|_| Uuid::new_v4()),
            vars,
            cwd,
            env_vars,
            ssh_host,
            command_tx,
            event_bus,
        })
    }

    /// Rebuild passive contexts for all blocks or blocks starting from a given index
    /// This should be called after document structure changes
    pub fn rebuild_passive_contexts(
        &mut self,
        start_index: Option<usize>,
        event_bus: &Arc<dyn EventBus>,
    ) -> Vec<DocumentError> {
        let mut errors = Vec::new();
        let start = start_index.unwrap_or(0);

        for i in start..self.blocks.len() {
            let block_id = self.blocks[i].id();

            // Build resolver from all blocks ABOVE this one
            let resolver = ContextResolver::from_blocks(&self.blocks[..i]);

            // Evaluate passive context for this block with the resolver
            match self.blocks[i].block.passive_context(&resolver) {
                Ok(Some(new_context)) => {
                    self.blocks[i].replace_context(new_context);
                }
                Ok(None) => {
                    // Block has no passive context, that's fine
                }
                Err(e) => {
                    let error_msg = format!("Failed to evaluate passive context: {}", e);
                    errors.push(DocumentError::PassiveContextError(error_msg.clone()));

                    // Emit Grand Central event for the error asynchronously
                    let event_bus = event_bus.clone();
                    let runbook_id = Uuid::parse_str(&self.id)
                        .unwrap_or_else(|_| Uuid::new_v4());
                    tokio::spawn(async move {
                        let _ = event_bus
                            .emit(GCEvent::BlockFailed {
                                block_id,
                                runbook_id,
                                error: error_msg,
                            })
                            .await;
                    });
                }
            }
        }

        errors
    }
}

pub struct DocumentContext<'document> {
    document: &'document mut Document,
    block_id: Uuid,
}

impl<'document> DocumentContext<'document> {
    pub fn new(document: &'document mut Document, block_id: Uuid) -> Self {
        Self { document, block_id }
    }

    pub fn insert<T: Any + Send + Sync>(&mut self, value: T) {
        self.document
            .get_block_mut(&self.block_id)
            .unwrap()
            .context_mut()
            .insert(value);
    }

    pub fn get_all_context_above<T: Any + Send + Sync>(&self) -> Vec<&T> {
        self.document.get_all_context_above::<T>(&self.block_id)
    }

    pub fn collect_context_above<T, R, F>(&self, init: R, collector: F) -> R
    where
        T: Any + Send + Sync,
        F: FnMut(R, &T) -> R,
    {
        self.document
            .collect_context_above::<T, R, F>(&self.block_id, init, collector)
    }

    pub fn get_var_map(&self) -> HashMap<String, String> {
        self.collect_context_above::<DocumentVar, HashMap<String, String>, _>(
            HashMap::new(),
            |mut acc, value| {
                acc.insert(value.0.clone(), value.1.clone());
                acc
            },
        )
    }
}

/// Variables defined blocks that can set multiple variables
#[derive(Clone, Debug, Default)]
pub struct DocumentVariableMap(pub HashMap<String, String>);

/// Variables defined by Var blocks
#[derive(Clone, Debug)]
pub struct DocumentVar(pub String, pub String);

/// Current working directory set by Directory blocks
#[derive(Clone, Debug)]
pub struct DocumentCwd(pub String);

/// Environment variables set by Environment blocks
#[derive(Clone, Debug)]
pub struct DocumentEnvVar(pub String, pub String);

/// SSH connection information from SshConnect blocks
#[derive(Clone, Debug)]
pub struct DocumentSshHost(pub Option<String>);

/// Provides read-only access to resolved context for template evaluation
/// This is built from blocks and passed to passive_context during rebuild
#[derive(Clone, Debug)]
pub struct ContextResolver {
    vars: HashMap<String, String>,
    cwd: String,
    env_vars: HashMap<String, String>,
    ssh_host: Option<String>,
}

impl ContextResolver {
    /// Create an empty context resolver
    pub fn new() -> Self {
        Self {
            vars: HashMap::new(),
            cwd: std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            env_vars: HashMap::new(),
            ssh_host: None,
        }
    }

    /// Build a resolver from blocks (typically all blocks above the current one)
    pub fn from_blocks(blocks: &[BlockWithContext]) -> Self {
        let mut vars = HashMap::new();
        let mut env_vars = HashMap::new();
        let mut cwd = std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let mut ssh_host = None;

        // Process blocks in order (earlier blocks can be overridden by later ones)
        for block in blocks {
            // Collect variables
            if let Some(var) = block.context().get::<DocumentVar>() {
                vars.insert(var.0.clone(), var.1.clone());
            }

            // Get current working directory (last one wins)
            if let Some(dir) = block.context().get::<DocumentCwd>() {
                cwd = dir.0.clone();
            }

            // Collect environment variables
            if let Some(env) = block.context().get::<DocumentEnvVar>() {
                env_vars.insert(env.0.clone(), env.1.clone());
            }

            // Get SSH host (last one wins)
            if let Some(host) = block.context().get::<DocumentSshHost>() {
                ssh_host = host.0.clone();
            }
        }

        Self {
            vars,
            cwd,
            env_vars,
            ssh_host,
        }
    }

    /// Resolve a template string using minijinja
    pub fn resolve_template(&self, template: &str) -> Result<String, Box<dyn std::error::Error>> {
        // If the string doesn't contain template markers, return it as-is
        if !template.contains("{{") && !template.contains("{%") {
            return Ok(template.to_string());
        }

        // Create a minijinja environment
        let env = Environment::new();

        // Build the context object for template rendering
        let mut context = HashMap::new();

        // // Add variables under "var" key
        // let mut var_map = HashMap::new();
        // for (k, v) in &self.vars {
        //     var_map.insert(k.clone(), Value::from(v.clone()));
        // }
        context.insert("var", Value::from_object(self.vars.clone()));

        // // Add environment variables under "env" key if needed
        // let mut env_map = HashMap::new();
        // for (k, v) in &self.env_vars {
        //     env_map.insert(k.clone(), Value::from(v.clone()));
        // }
        context.insert("env", Value::from_object(self.env_vars.clone()));

        // TODO: workspace map
        // other special contexts?

        // Render the template
        let rendered = env.render_str(template, context)?;
        Ok(rendered)
    }

    /// Get a variable value
    pub fn get_var(&self, name: &str) -> Option<&String> {
        self.vars.get(name)
    }

    /// Get all variables
    pub fn vars(&self) -> &HashMap<String, String> {
        &self.vars
    }

    /// Get current working directory
    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    /// Get environment variables
    pub fn env_vars(&self) -> &HashMap<String, String> {
        &self.env_vars
    }

    /// Get SSH host
    pub fn ssh_host(&self) -> Option<&String> {
        self.ssh_host.as_ref()
    }
}

impl Default for ContextResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of document context for block execution
/// This is provided to blocks when they start executing and contains
/// all the context they need without holding locks on the document
#[derive(Clone)]
pub struct DocumentExecutionView {
    pub block_id: Uuid,
    pub block: Block,
    pub runbook_id: Uuid,

    // Computed context snapshot
    pub vars: HashMap<String, String>,
    pub cwd: String,
    pub env_vars: HashMap<String, String>,
    pub ssh_host: Option<String>,

    // Handle for sending updates back to document actor
    command_tx: mpsc::UnboundedSender<DocumentCommand>,

    // Event bus for emitting Grand Central events
    pub event_bus: Arc<dyn EventBus>,
}

impl DocumentExecutionView {
    /// Update this block's context during execution
    /// This sends a command to the document actor to update the context
    pub async fn update_context<F>(&self, update_fn: F) -> Result<(), DocumentError>
    where
        F: FnOnce(&mut BlockContext) + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::UpdateContext {
                block_id: self.block_id,
                update_fn: Box::new(update_fn),
                reply: tx,
            })
            .map_err(|_| DocumentError::ActorSendError)?;
        rx.await.map_err(|_| DocumentError::ActorSendError)?
    }

    /// Complete execution and set the final context for this block
    pub async fn complete_execution(self, context: BlockContext) -> Result<(), DocumentError> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(DocumentCommand::CompleteExecution {
                block_id: self.block_id,
                context,
                reply: tx,
            })
            .map_err(|_| DocumentError::ActorSendError)?;
        rx.await.map_err(|_| DocumentError::ActorSendError)?
    }

    /// Get a variable from the execution context
    pub fn get_var(&self, name: &str) -> Option<&String> {
        self.vars.get(name)
    }

    /// Get all variables
    pub fn get_all_vars(&self) -> &HashMap<String, String> {
        &self.vars
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
