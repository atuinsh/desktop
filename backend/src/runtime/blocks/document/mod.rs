use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use uuid::Uuid;

use crate::runtime::{
    blocks::{
        document::{
            actor::{BlockLocalValueProvider, DocumentError, DocumentHandle},
            block_context::{BlockContext, BlockWithContext, ContextResolver, ResolvedContext},
            bridge::DocumentBridgeMessage,
        },
        handler::ExecutionContext,
        Block, KNOWN_UNSUPPORTED_BLOCKS,
    },
    events::{EventBus, GCEvent},
    pty_store::PtyStoreHandle,
    ssh_pool::SshPoolHandle,
    workflow::event::WorkflowEvent,
    ClientMessageChannel,
};

pub mod actor;
pub mod block_context;
pub mod bridge;

/// Document-level context containing all block contexts
/// This is the internal state owned by the DocumentActor
pub struct Document {
    id: String,
    blocks: Vec<BlockWithContext>,
    document_bridge: Arc<dyn ClientMessageChannel<DocumentBridgeMessage>>,
    known_unsupported_blocks: HashSet<String>,
    block_local_value_provider: Option<Box<dyn BlockLocalValueProvider>>,
}

impl Document {
    pub fn new(
        id: String,
        document: Vec<serde_json::Value>,
        document_bridge: Arc<dyn ClientMessageChannel<DocumentBridgeMessage>>,
        block_local_value_provider: Option<Box<dyn BlockLocalValueProvider>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut doc = Self {
            id,
            blocks: vec![],
            document_bridge,
            known_unsupported_blocks: HashSet::new(),
            block_local_value_provider,
        };
        doc.put_document(document)?;

        Ok(doc)
    }

    pub fn reset_state(&mut self) -> Result<(), DocumentError> {
        for block in &mut self.blocks {
            block.update_passive_context(BlockContext::new());
            block.update_active_context(BlockContext::new());
        }

        Ok(())
    }

    pub fn update_document_bridge(
        &mut self,
        document_bridge: Arc<dyn ClientMessageChannel<DocumentBridgeMessage>>,
    ) {
        self.document_bridge = document_bridge;
    }

    pub fn put_document(
        &mut self,
        document: Vec<serde_json::Value>,
    ) -> Result<Option<usize>, Box<dyn std::error::Error>> {
        let new_blocks = self.flatten_document(&document)?;

        if self.blocks.is_empty() {
            self.blocks = new_blocks
                .into_iter()
                .map(|b| BlockWithContext::new(b, BlockContext::new()))
                .collect();
            return Ok(Some(0));
        }

        // Capture old state for change detection
        let old_block_ids: Vec<Uuid> = self.blocks.iter().map(|b| b.id()).collect();

        // Build a map of existing blocks by ID for quick lookup
        let mut existing_blocks_map: HashMap<Uuid, BlockWithContext> =
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
                let block_with_context =
                    BlockWithContext::new(new_block.clone(), BlockContext::new());
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

        Ok(rebuild_from_index)
    }

    /// Flatten the nested document structure into a flat list
    pub fn flatten_document(
        &mut self,
        document: &[serde_json::Value],
    ) -> Result<Vec<Block>, Box<dyn std::error::Error>> {
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
                        log::warn!(
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
    ) -> Result<(), Box<dyn std::error::Error>> {
        for node in nodes {
            out.push(node.clone());

            if let Some(children) = node.get("children").and_then(|v| v.as_array()) {
                Self::flatten_recursive(children, out)?;
            }
        }

        Ok(())
    }

    /// Get a block's index
    pub fn get_block_index(&self, block_id: &Uuid) -> Option<usize> {
        self.blocks.iter().position(|block| &block.id() == block_id)
    }

    /// Get a block's context
    pub fn get_block(&self, block_id: &Uuid) -> Option<&BlockWithContext> {
        let index = self.get_block_index(block_id)?;
        self.get_block_by_index(index)
    }

    pub fn get_block_by_index(&self, index: usize) -> Option<&BlockWithContext> {
        self.blocks.get(index)
    }

    /// Get a mutable reference to a block
    pub fn get_block_mut(&mut self, block_id: &Uuid) -> Option<&mut BlockWithContext> {
        let index = self.get_block_index(block_id)?;
        self.get_block_mut_by_index(index)
    }

    pub fn get_block_mut_by_index(&mut self, index: usize) -> Option<&mut BlockWithContext> {
        self.blocks.get_mut(index)
    }

    /// Build an execution context for a block, capturing all context from blocks above it
    #[allow(clippy::too_many_arguments)]
    pub fn build_execution_context(
        &self,
        block_id: &Uuid,
        handle: Arc<DocumentHandle>,
        event_bus: Arc<dyn EventBus>,
        event_sender: tokio::sync::broadcast::Sender<WorkflowEvent>,
        ssh_pool: Option<SshPoolHandle>,
        pty_store: Option<PtyStoreHandle>,
    ) -> Result<ExecutionContext, DocumentError> {
        // Verify block exists
        let _block = self
            .get_block(block_id)
            .ok_or(DocumentError::BlockNotFound(*block_id))?;

        // Find the block's position in the document
        let position = self
            .get_block_index(block_id)
            .ok_or(DocumentError::BlockNotFound(*block_id))?;

        // Build context resolver from all blocks above this one
        let context_resolver = ContextResolver::from_blocks(&self.blocks[..position]);

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
            .workflow_event_sender(event_sender)
            .ssh_pool_opt(ssh_pool)
            .pty_store_opt(pty_store)
            .gc_event_bus(event_bus)
            .build())
    }

    pub fn get_resolved_context(&self, block_id: &Uuid) -> Result<ResolvedContext, DocumentError> {
        let position = self
            .get_block_index(block_id)
            .ok_or(DocumentError::BlockNotFound(*block_id))?;

        let resolver = ContextResolver::from_blocks(&self.blocks[..position]);
        Ok(ResolvedContext::from_resolver(&resolver))
    }

    /// Rebuild passive contexts for all blocks or blocks starting from a given index
    /// This should be called after document structure changes or block context change
    pub async fn rebuild_contexts(
        &mut self,
        start_index: Option<usize>,
        event_bus: Arc<dyn EventBus>,
    ) -> Result<(), Vec<DocumentError>> {
        log::trace!(
            "Rebuilding passive contexts for document {} starting from index {}",
            self.id,
            start_index.unwrap_or(0)
        );

        let mut errors = Vec::new();
        let start = start_index.unwrap_or(0);

        let mut context_resolver = ContextResolver::from_blocks(&self.blocks[..start]);
        for i in start..self.blocks.len() {
            let block_id = self.blocks[i].id();

            // Build resolver from all blocks ABOVE this one
            // let resolver = ContextResolver::from_blocks(&self.blocks[..i]);

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
                    self.blocks[i].update_passive_context(new_context);
                }
                Ok(None) => {
                    self.blocks[i].update_passive_context(BlockContext::new());
                }
                Err(e) => {
                    self.blocks[i].update_passive_context(BlockContext::new());

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

            context_resolver.push_block(&self.blocks[i]);

            let document_bridge = self.document_bridge.clone();

            let _ = document_bridge
                .send(DocumentBridgeMessage::BlockContextUpdate {
                    block_id,
                    context: ResolvedContext::from_resolver(&context_resolver),
                })
                .await;
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}
