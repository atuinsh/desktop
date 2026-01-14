//! Document management and lifecycle
//!
//! This module provides the core document abstraction for runbook execution.
//! Documents contain a collection of blocks and manage their execution state,
//! context propagation, and lifecycle events.
//!
//! The primary interface is the [`DocumentHandle`], which provides async methods
//! for interacting with a running document.

pub(crate) mod actor;

pub use actor::{DocumentError, DocumentHandle};
use serde_json::Value;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use uuid::Uuid;

use crate::templates::DocumentTemplateState;

use crate::{
    blocks::{Block, KNOWN_UNSUPPORTED_BLOCKS},
    client::{DocumentBridgeMessage, LocalValueProvider, MessageChannel, RunbookContentLoader},
    context::{
        BlockContext, BlockContextStorage, BlockState, ContextResolver, DocumentBlock,
        ResolvedContext,
    },
    events::{EventBus, GCEvent},
    execution::{ExecutionContext, ExecutionHandle},
    pty::PtyStoreHandle,
    ssh::SshPoolHandle,
};

/// Document-level context containing all block contexts
/// This is the internal state owned by the DocumentActor
pub(crate) struct Document {
    pub(crate) id: String,
    pub(crate) raw: Vec<serde_json::Value>,
    pub(crate) blocks: Vec<DocumentBlock>,
    pub(crate) document_bridge: Arc<dyn MessageChannel<DocumentBridgeMessage>>,
    pub(crate) known_unsupported_blocks: HashSet<String>,
    pub(crate) block_local_value_provider: Option<Arc<dyn LocalValueProvider>>,
    pub(crate) context_storage: Option<Box<dyn BlockContextStorage>>,
    /// Loader for sub-runbook content (optional - sub-runbooks won't work without this)
    pub(crate) runbook_loader: Option<Arc<dyn RunbookContentLoader>>,
    /// Parent context resolver for sub-runbooks. When set, this document inherits
    /// vars, env_vars, cwd, and ssh_host from the parent.
    pub(crate) parent_context: Option<Arc<ContextResolver>>,
    /// The workspace root path, if this document belongs to an offline workspace.
    /// Used for template resolution (e.g., `{{ workspace.root }}`).
    pub(crate) workspace_root: Option<String>,
    /// Tracks the last ResolvedContext sent to the frontend for each block.
    /// Used to avoid sending redundant BlockContextUpdate messages when the
    /// resolved context hasn't actually changed.
    last_sent_contexts: HashMap<Uuid, ResolvedContext>,
}

impl Document {
    pub async fn new(
        id: String,
        document: Vec<serde_json::Value>,
        document_bridge: Arc<dyn MessageChannel<DocumentBridgeMessage>>,
        block_local_value_provider: Option<Arc<dyn LocalValueProvider>>,
        context_storage: Option<Box<dyn BlockContextStorage>>,
        runbook_loader: Option<Arc<dyn RunbookContentLoader>>,
        workspace_root: Option<String>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut doc = Self {
            id,
            blocks: vec![],
            raw: vec![],
            document_bridge,
            known_unsupported_blocks: HashSet::new(),
            block_local_value_provider,
            context_storage,
            runbook_loader,
            parent_context: None,
            workspace_root,
            last_sent_contexts: HashMap::new(),
        };
        doc.put_document(document).await?;

        Ok(doc)
    }

    pub async fn reset_state(&mut self) -> Result<(), DocumentError> {
        // Clear last sent contexts so rebuild_contexts will send fresh updates
        self.last_sent_contexts.clear();

        for block in &mut self.blocks {
            block.replace_passive_context(BlockContext::new());
            block.replace_active_context(BlockContext::new());
            if let Some(storage) = self.context_storage.as_ref() {
                let result = storage
                    .delete(self.id.as_str(), &block.id())
                    .await
                    .map_err(|e| DocumentError::StoreActiveContextError(e.to_string()));

                if let Err(e) = result {
                    tracing::warn!(
                        "Failed to delete stored active context for block {block_id} in document {document_id}: {e}",
                        block_id = block.id(),
                        document_id = self.id
                    );
                }
            }
        }

        Ok(())
    }

    pub fn update_document_bridge(
        &mut self,
        document_bridge: Arc<dyn MessageChannel<DocumentBridgeMessage>>,
    ) {
        self.document_bridge = document_bridge;
    }

    pub async fn put_document(
        &mut self,
        document: Vec<serde_json::Value>,
    ) -> Result<Option<usize>, Box<dyn std::error::Error + Send + Sync>> {
        let new_blocks = self.flatten_document(&document)?;
        self.raw = document;

        if self.blocks.is_empty() {
            self.blocks = Vec::with_capacity(new_blocks.len());
            for block in new_blocks {
                let context = if let Some(storage) = self.context_storage.as_ref() {
                    storage
                        .load(self.id.as_str(), &block.id())
                        .await
                        .unwrap_or(None)
                } else {
                    None
                };
                let block_state = block.create_state();
                self.blocks.push(DocumentBlock::new(
                    block,
                    BlockContext::new(),
                    context,
                    block_state,
                    None,
                ));
            }
            return Ok(Some(0));
        }

        // Capture old state for change detection
        let old_block_ids: Vec<Uuid> = self.blocks.iter().map(|b| b.id()).collect();

        // Build a map of existing blocks by ID for quick lookup
        let mut existing_blocks_map: HashMap<Uuid, DocumentBlock> =
            self.blocks.drain(..).map(|b| (b.id(), b)).collect();

        // Track which blocks need context rebuild
        let mut rebuild_from_index: Option<usize> = None;

        // Single pass: Build the final block list in the correct order
        let mut updated_blocks = Vec::with_capacity(new_blocks.len());

        for (new_index, new_block) in new_blocks.into_iter().enumerate() {
            if let Some(mut existing) = existing_blocks_map.remove(&new_block.id()) {
                // Block exists - check if content changed or position moved
                let content_changed = existing.block() != &new_block;
                let old_index = old_block_ids.iter().position(|id| id == &new_block.id());
                let position_changed = old_index != Some(new_index);

                if content_changed {
                    let block = existing.block_mut();
                    *block = new_block;
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
                let document_block =
                    DocumentBlock::new(new_block.clone(), BlockContext::new(), None, None, None);
                updated_blocks.push(document_block);

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
                // Clean up last sent context for deleted block
                self.last_sent_contexts.remove(deleted_id);

                if let Some(storage) = self.context_storage.as_ref() {
                    let result = storage
                        .delete(self.id.as_str(), deleted_id)
                        .await
                        .map_err(|e| DocumentError::StoreActiveContextError(e.to_string()));

                    if let Err(e) = result {
                        tracing::warn!(
                            "Failed to delete stored active context for block {block_id} in document {document_id}: {e}",
                            block_id = deleted_id,
                            document_id = self.id
                        );
                    }
                }

                if let Some(old_index) = old_block_ids.iter().position(|id| id == deleted_id) {
                    rebuild_from_index = Some(match rebuild_from_index {
                        Some(existing) => std::cmp::min(existing, old_index),
                        None => old_index,
                    });
                }
            }
        }

        self.blocks = updated_blocks;

        Ok(rebuild_from_index)
    }

    /// Flatten the nested document structure into a flat list
    pub fn flatten_document(
        &mut self,
        document: &[serde_json::Value],
    ) -> Result<Vec<Block>, Box<dyn std::error::Error + Send + Sync>> {
        let mut doc_blocks = Vec::with_capacity(document.len());
        Self::flatten_recursive(document, &mut doc_blocks)?;
        let blocks = doc_blocks
            .iter()
            .filter_map(|value| match value.try_into() {
                Ok(block) => Some(block),
                Err(e) => {
                    let block_type: String = value.get("type").and_then(|v| v.as_str()).unwrap_or("<unknown>").to_string();
                    let block_id: String = value.get("id").and_then(|v| v.as_str()).unwrap_or("<unknown id>").to_string();

                    let inserted = self.known_unsupported_blocks.insert(
                        block_id,
                    );

                    if !KNOWN_UNSUPPORTED_BLOCKS.contains(&block_type.as_str()) && inserted
                    {
                        tracing::warn!(
                            "Failed to parse Value with ID {:?} of type {:?} into Block: {:?}. Will not warn about this block again.",
                            value
                                .get("id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("<unknown>"),
                            value
                                .get("type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("<unknown>"),
                            e
                        );
                    }
                    None
                }
            })
            .collect();

        Ok(blocks)
    }

    fn flatten_recursive(
        nodes: &[serde_json::Value],
        out: &mut Vec<serde_json::Value>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for node in nodes {
            out.push(node.clone());

            if let Some(children) = node.get("children").and_then(|v| v.as_array()) {
                Self::flatten_recursive(children, out)?;
            }
        }

        Ok(())
    }

    /// Get all blocks
    pub fn blocks(&self) -> &[DocumentBlock] {
        &self.blocks
    }

    /// Get a block's index
    pub fn get_block_index(&self, block_id: &Uuid) -> Option<usize> {
        self.blocks.iter().position(|block| &block.id() == block_id)
    }

    /// Get a block's context
    pub fn get_block(&self, block_id: &Uuid) -> Option<&DocumentBlock> {
        let index = self.get_block_index(block_id)?;
        self.get_block_by_index(index)
    }

    pub fn get_block_by_index(&self, index: usize) -> Option<&DocumentBlock> {
        self.blocks.get(index)
    }

    /// Get a mutable reference to a block
    pub fn get_block_mut(&mut self, block_id: &Uuid) -> Option<&mut DocumentBlock> {
        let index = self.get_block_index(block_id)?;
        self.get_block_mut_by_index(index)
    }

    pub fn get_block_mut_by_index(&mut self, index: usize) -> Option<&mut DocumentBlock> {
        self.blocks.get_mut(index)
    }

    /// Set the parent context for this document (used for sub-runbooks)
    pub fn set_parent_context(&mut self, parent: Arc<ContextResolver>) {
        self.parent_context = Some(parent);
    }

    /// Get the current context resolver (includes all blocks and parent context)
    pub fn get_context_resolver(&self) -> ContextResolver {
        let mut resolver = match &self.parent_context {
            Some(parent) => ContextResolver::from_parent(parent),
            None => ContextResolver::new(),
        };

        if let Some(ref workspace_root) = self.workspace_root {
            let mut workspace_context = HashMap::new();
            workspace_context.insert("root".to_string(), workspace_root.clone());
            resolver.add_extra_template_context("workspace".to_string(), workspace_context);
        }

        resolver.push_blocks(&self.blocks);
        resolver
    }

    /// Build an execution context for a block, capturing all context from blocks above it
    #[allow(clippy::too_many_arguments)]
    pub fn build_execution_context(
        &self,
        block_id: &Uuid,
        handle: Arc<DocumentHandle>,
        event_bus: Arc<dyn EventBus>,
        ssh_pool: Option<SshPoolHandle>,
        pty_store: Option<PtyStoreHandle>,
        extra_template_context: Option<HashMap<String, HashMap<String, String>>>,
    ) -> Result<ExecutionContext, DocumentError> {
        // Verify block exists
        let _block = self
            .get_block(block_id)
            .ok_or(DocumentError::BlockNotFound(*block_id))?;

        // Find the block's position in the document
        let position = self
            .get_block_index(block_id)
            .ok_or(DocumentError::BlockNotFound(*block_id))?;

        // Build context resolver - add extra context BEFORE processing blocks
        // so that templates like {{ workspace.root }} can resolve during block processing
        let mut context_resolver = match &self.parent_context {
            Some(parent) => ContextResolver::from_parent(parent),
            None => ContextResolver::new(),
        };

        // Add workspace template context first (before blocks are processed)
        if let Some(ref workspace_root) = self.workspace_root {
            let mut workspace_context = HashMap::new();
            workspace_context.insert("root".to_string(), workspace_root.clone());
            context_resolver.add_extra_template_context("workspace".to_string(), workspace_context);
        }

        // Add any extra template context passed by caller
        if let Some(extra_template_context) = extra_template_context {
            for (namespace, context) in extra_template_context {
                context_resolver.add_extra_template_context(namespace.clone(), context.clone());
            }
        }

        let mut runbook_template_context = HashMap::new();
        runbook_template_context.insert("id".to_string(), self.id.clone());
        context_resolver
            .add_extra_template_context("runbook".to_string(), runbook_template_context);

        // Now process blocks - templates will resolve against the context we just set up
        context_resolver.push_blocks(&self.blocks[..position]);

        let block_outputs = self
            .blocks
            .iter()
            .map(|block| (block.id().to_string(), block.execution_output().clone()))
            .collect::<HashMap<_, _>>();

        let document_template_context = DocumentTemplateState::new(
            flatten_document(&self.raw).as_slice(),
            Some(&block_id.to_string()),
            block_outputs,
        );

        if let Some(document_template_context) = document_template_context {
            context_resolver
                .add_extra_template_context("doc".to_string(), document_template_context);
        }

        // Create DocumentHandle for the block to use for context updates
        let document_handle = handle.clone();

        // Parse runbook ID
        let runbook_id = Uuid::parse_str(&self.id)
            .map_err(|_| DocumentError::InvalidRunbookId(self.id.clone()))?;

        let output_channel = self.document_bridge.clone();

        Ok(ExecutionContext::builder()
            .block_id(*block_id)
            .runbook_id(runbook_id)
            .document_handle(document_handle)
            .context_resolver(Arc::new(context_resolver))
            .output_channel(output_channel)
            .ssh_pool_opt(ssh_pool)
            .pty_store_opt(pty_store)
            .gc_event_bus(event_bus)
            .handle(ExecutionHandle::new(*block_id))
            .runbook_loader_opt(self.runbook_loader.clone())
            .build())
    }

    pub fn get_resolved_context(&self, block_id: &Uuid) -> Result<ResolvedContext, DocumentError> {
        let position = self
            .get_block_index(block_id)
            .ok_or(DocumentError::BlockNotFound(*block_id))?;

        let mut resolver = match &self.parent_context {
            Some(parent) => ContextResolver::from_parent(parent),
            None => ContextResolver::new(),
        };

        if let Some(ref workspace_root) = self.workspace_root {
            let mut workspace_context = HashMap::new();
            workspace_context.insert("root".to_string(), workspace_root.clone());
            resolver.add_extra_template_context("workspace".to_string(), workspace_context);
        }

        resolver.push_blocks(&self.blocks[..position]);
        Ok(ResolvedContext::from_resolver(&resolver))
    }

    pub fn get_last_block_resolved_context(&self) -> Result<ResolvedContext, DocumentError> {
        let resolver = ContextResolver::from_blocks(&self.blocks);
        Ok(ResolvedContext::from_resolver(&resolver))
    }

    pub fn get_block_state(&self, block_id: &Uuid) -> Result<Value, DocumentError> {
        let position = self
            .get_block_index(block_id)
            .ok_or(DocumentError::BlockNotFound(*block_id))?;

        if let Some(block) = self.blocks.get(position) {
            if let Some(state) = block.state() {
                return self.serialize_block_state(state);
            }
        }

        Err(DocumentError::BlockNotFound(*block_id))
    }

    /// Rebuild passive contexts for all blocks or blocks starting from a given index
    /// This should be called after document structure changes or block context change
    pub async fn rebuild_contexts(
        &mut self,
        start_index: Option<usize>,
        event_bus: Arc<dyn EventBus>,
    ) -> Result<(), Vec<DocumentError>> {
        tracing::trace!(
            "Rebuilding passive contexts for document {} starting from index {}",
            self.id,
            start_index.unwrap_or(0)
        );

        let mut errors = Vec::new();
        let start = start_index.unwrap_or(0);

        // Build context resolver - add extra context BEFORE processing blocks
        // so that templates like {{ workspace.root }} can resolve during block processing
        let mut context_resolver = match &self.parent_context {
            Some(parent) => ContextResolver::from_parent(parent),
            None => ContextResolver::new(),
        };

        // Add workspace template context first (before blocks are processed)
        if let Some(ref workspace_root) = self.workspace_root {
            let mut workspace_context = HashMap::new();
            workspace_context.insert("root".to_string(), workspace_root.clone());
            context_resolver.add_extra_template_context("workspace".to_string(), workspace_context);
        }

        // Now process blocks[..start] with workspace context available
        context_resolver.push_blocks(&self.blocks[..start]);

        for i in start..self.blocks.len() {
            let block_id = self.blocks[i].id();

            // Build DocumentTemplateState so blocks can access doc.named[name].output etc.
            let block_outputs = self
                .blocks
                .iter()
                .map(|block| (block.id().to_string(), block.execution_output()))
                .collect::<HashMap<_, _>>();

            let document_template_context = DocumentTemplateState::new(
                flatten_document(&self.raw).as_slice(),
                Some(&block_id.to_string()),
                block_outputs,
            );

            if let Some(document_template_context) = document_template_context {
                context_resolver
                    .add_extra_template_context("doc".to_string(), document_template_context);
            }

            // Evaluate passive context for this block with the resolver
            match self.blocks[i]
                .block()
                .passive_context(
                    &context_resolver,
                    self.block_local_value_provider.as_deref(),
                )
                .await
            {
                Ok(Some(new_context)) => {
                    self.blocks[i].replace_passive_context(new_context);
                }
                Ok(None) => {
                    self.blocks[i].replace_passive_context(BlockContext::new());
                }
                Err(e) => {
                    self.blocks[i].replace_passive_context(BlockContext::new());

                    let error_msg = format!(
                        "Failed to evaluate passive context for block {block_id}: {}",
                        e
                    );
                    errors.push(DocumentError::PassiveContextError(error_msg.clone()));

                    // Emit Grand Central event for the error asynchronously
                    let event_bus = event_bus.clone();
                    let runbook_id = Uuid::parse_str(&self.id).unwrap_or_else(|_| Uuid::new_v4());
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

            // Only send BlockContextUpdate if the resolved context actually changed
            let new_resolved_context = ResolvedContext::from_resolver(&context_resolver);
            let context_changed = self
                .last_sent_contexts
                .get(&block_id)
                .map(|last| last != &new_resolved_context)
                .unwrap_or(true);

            if context_changed {
                let document_bridge = self.document_bridge.clone();
                let _ = document_bridge
                    .send(DocumentBridgeMessage::BlockContextUpdate {
                        block_id,
                        context: new_resolved_context.clone(),
                    })
                    .await;
                self.last_sent_contexts
                    .insert(block_id, new_resolved_context);
            }

            // Update the context resolver for the next block
            context_resolver.push_block(&self.blocks[i]);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub(crate) async fn emit_state_changed(
        &self,
        block_id: Uuid,
        state: &dyn BlockState,
    ) -> Result<(), DocumentError> {
        let state_value = self.serialize_block_state(state)?;

        let _ = self
            .document_bridge
            .send(DocumentBridgeMessage::BlockStateChanged {
                block_id,
                state: state_value,
            })
            .await;
        Ok(())
    }

    fn serialize_block_state(&self, state: &dyn BlockState) -> Result<Value, DocumentError> {
        let mut buf = Vec::new();
        let mut serializer = serde_json::Serializer::new(&mut buf);
        let mut erased = <dyn erased_serde::Serializer>::erase(&mut serializer);
        state
            .erased_serialize(&mut erased)
            .map_err(|e| DocumentError::StateSerializationError(e.to_string()))?;
        serde_json::from_slice(&buf)
            .map_err(|e| DocumentError::StateSerializationError(e.to_string()))
    }

    pub async fn emit_block_execution_output_changed(
        &self,
        block_id: Uuid,
    ) -> Result<(), DocumentError> {
        let _ = self
            .document_bridge
            .send(DocumentBridgeMessage::BlockExecutionOutputChanged { block_id })
            .await;
        Ok(())
    }

    pub(crate) async fn store_active_context(&self, block_id: Uuid) -> Result<(), DocumentError> {
        let block = self
            .get_block(&block_id)
            .ok_or(DocumentError::BlockNotFound(block_id))?;
        if let Some(storage) = self.context_storage.as_ref() {
            let result = storage
                .save(self.id.as_str(), &block_id, block.active_context())
                .await
                .map_err(|e| DocumentError::InvalidStructure(e.to_string()));

            if let Err(e) = result {
                tracing::warn!(
                    "Failed to store active context for block {block_id} in document {document_id}: {e}",
                    block_id = block.id(),
                    document_id = self.id
                );
            }
        }
        Ok(())
    }
}

/// Flatten a document to include nested blocks (like those in ToggleHeading children)
/// This creates a linear execution order regardless of UI nesting structure
pub fn flatten_document(doc: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut flattened = Vec::with_capacity(doc.len());
    for block in doc {
        flattened.push(block.clone());
        if let Some(children) = block.get("children").and_then(|c| c.as_array()) {
            if !children.is_empty() {
                flattened.extend(flatten_document(children));
            }
        }
    }
    flattened
}
