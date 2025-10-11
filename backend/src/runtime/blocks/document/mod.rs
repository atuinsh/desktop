use serde::{Deserialize, Serialize};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use uuid::Uuid;

use crate::runtime::blocks::Block;

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlockChangeSource {
    Local,
    Paste,
    Drop,
    Undo,
    Redo,
    UndoRedo,
    YjsRemote,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockInsert {
    block: serde_json::Value,
    source: BlockChangeSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockDelete {
    block: serde_json::Value,
    source: BlockChangeSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockUpdate {
    block: serde_json::Value,
    source: BlockChangeSource,
    prev_block: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockMove {
    block: serde_json::Value,
    source: BlockChangeSource,
    prev_block: serde_json::Value,
    prevParent: Option<serde_json::Value>,
    currentParent: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockNoteChange {
    Insert(BlockInsert),
    Delete(BlockDelete),
    Update(BlockUpdate),
    Move(BlockMove),
}

/// Document-level context containing all block contexts
pub struct Document {
    id: String,
    blocks: Vec<BlockWithContext>,
}

impl Document {
    pub fn new(
        id: String,
        document: Vec<serde_json::Value>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let blocks = Self::flatten_document(&document)?;

        Ok(Self { id, blocks })
    }

    pub fn apply_change(&mut self, change: BlockNoteChange) {
        log::debug!(
            "Applying document change to document {id}: {change:?}",
            id = self.id
        );

        match change {
            BlockNoteChange::Insert(insert) => {
                //
            }
            BlockNoteChange::Delete(delete) => {
                let block_id = delete
                    .block
                    .get("id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok());

                if let Some(block_id) = block_id {
                    let block_index = self.blocks.iter().position(|block| block.id() == block_id);
                }
            }
            BlockNoteChange::Update(update) => {
                //
            }
            BlockNoteChange::Move(move_) => {
                //
            }
        }
    }

    /// Flatten the nested document structure into a flat list
    fn flatten_document(
        document: &[serde_json::Value],
    ) -> Result<Vec<BlockWithContext>, Box<dyn std::error::Error>> {
        let mut doc_blocks = Vec::new();
        Self::flatten_recursive(document, &mut doc_blocks)?;
        let blocks = doc_blocks
            .iter()
            .filter_map(|value| Block::from_document(value).ok())
            .map(|block| BlockWithContext::new(block, BlockContext::new()))
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
