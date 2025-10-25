use std::{
    any::Any,
    collections::{HashMap, HashSet},
    sync::Arc,
};

use tokio::sync::mpsc;
use uuid::Uuid;

use crate::runtime::{
    blocks::{
        document::{
            block_context::{
                BlockContext, BlockWithContext, ContextResolver, DocumentCwd, DocumentEnvVar,
                DocumentSshHost, DocumentVar, ResolvedContext,
            },
            bridge::DocumentBridgeMessage,
            DocumentCommand, DocumentError,
        },
        handler::ExecutionContext,
        Block, KNOWN_UNSUPPORTED_BLOCKS,
    },
    events::{EventBus, GCEvent},
    ClientMessageChannel,
};

/// Document-level context containing all block contexts
/// This is the internal state owned by the DocumentActor
pub struct Document {
    id: String,
    blocks: Vec<BlockWithContext>,
    document_bridge: Box<dyn ClientMessageChannel<DocumentBridgeMessage>>,
    known_unsupported_blocks: HashSet<String>,
}

impl Document {
    pub fn new(
        id: String,
        document: Vec<serde_json::Value>,
        document_bridge: Box<dyn ClientMessageChannel<DocumentBridgeMessage>>,
        _command_tx: mpsc::UnboundedSender<DocumentCommand>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut doc = Self {
            id,
            blocks: vec![],
            document_bridge,
            known_unsupported_blocks: HashSet::new(),
        };
        doc.put_document(document)?;

        Ok(doc)
    }

    pub fn update_document_bridge(
        &mut self,
        document_bridge: Box<dyn ClientMessageChannel<DocumentBridgeMessage>>,
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
        self.flatten_recursive(document, &mut doc_blocks)?;
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
        &self,
        nodes: &[serde_json::Value],
        out: &mut Vec<serde_json::Value>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for node in nodes {
            out.push(node.clone());

            if let Some(children) = node.get("children").and_then(|v| v.as_array()) {
                self.flatten_recursive(children, out)?;
            }
        }

        Ok(())
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

    /// Build an execution context for a block, capturing all context from blocks above it
    pub fn build_execution_context(
        &self,
        block_id: &Uuid,
        command_tx: mpsc::UnboundedSender<super::DocumentCommand>,
        event_bus: Arc<dyn EventBus>,
    ) -> Result<ExecutionContext, DocumentError> {
        todo!()
        // let block = self
        //     .get_block(block_id)
        //     .ok_or_else(|| DocumentError::BlockNotFound(*block_id))?;

        // // Collect variables
        // let vars = self.collect_context_above::<DocumentVar, HashMap<String, String>, _>(
        //     block_id,
        //     HashMap::new(),
        //     |mut acc, var| {
        //         acc.insert(var.0.clone(), var.1.clone());
        //         acc
        //     },
        // );

        // // Get current working directory (use last one set, or default)
        // let cwd = self
        //     .get_context_above::<DocumentCwd>(block_id)
        //     .map(|cwd| cwd.0.clone())
        //     .unwrap_or_else(|| {
        //         std::env::current_dir()
        //             .unwrap_or_default()
        //             .to_string_lossy()
        //             .to_string()
        //     });

        // // Collect environment variables
        // let env_vars = self.collect_context_above::<DocumentEnvVar, HashMap<String, String>, _>(
        //     block_id,
        //     HashMap::new(),
        //     |mut acc, env| {
        //         acc.insert(env.0.clone(), env.1.clone());
        //         acc
        //     },
        // );

        // // Get SSH host (use most recent)
        // let ssh_host = self
        //     .get_context_above::<DocumentSshHost>(block_id)
        //     .and_then(|host| host.0.clone());

        // Ok(DocumentExecutionView {
        //     block_id: *block_id,
        //     block: block.block().clone(),
        //     runbook_id: Uuid::parse_str(&self.id).unwrap_or_else(|_| Uuid::new_v4()),
        //     vars,
        //     cwd,
        //     env_vars,
        //     ssh_host,
        //     command_tx,
        //     event_bus,
        // })
    }

    pub fn get_resolved_context(&self, block_id: &Uuid) -> Result<ResolvedContext, DocumentError> {
        let position = self
            .blocks
            .iter()
            .position(|b| b.id() == *block_id)
            .ok_or(DocumentError::BlockNotFound(*block_id))?;

        let resolver = ContextResolver::from_blocks(&self.blocks[..position]);
        Ok(ResolvedContext::from_resolver(&resolver))
    }

    /// Rebuild passive contexts for all blocks or blocks starting from a given index
    /// This should be called after document structure changes
    pub fn rebuild_passive_contexts(
        &mut self,
        start_index: Option<usize>,
        event_bus: Arc<dyn EventBus>,
    ) -> Result<(), Vec<DocumentError>> {
        let mut errors = Vec::new();
        let start = start_index.unwrap_or(0);

        let mut context_resolver = ContextResolver::from_blocks(&self.blocks[..start]);
        for i in start..self.blocks.len() {
            let block_id = self.blocks[i].id();

            // Build resolver from all blocks ABOVE this one
            // let resolver = ContextResolver::from_blocks(&self.blocks[..i]);

            // Evaluate passive context for this block with the resolver
            match self.blocks[i].block().passive_context(&context_resolver) {
                Ok(Some(new_context)) => {
                    self.blocks[i].update_context(new_context);
                }
                Ok(None) => {
                    // Block has no passive context, that's fine
                }
                Err(e) => {
                    let error_msg = format!("Failed to evaluate passive context: {}", e);
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
            let _ = self
                .document_bridge
                .send(DocumentBridgeMessage::BlockContextUpdate {
                    block_id,
                    context: ResolvedContext::from_resolver(&context_resolver),
                });
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}
