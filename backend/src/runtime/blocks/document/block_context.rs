use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

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

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn block_mut(&mut self) -> &mut Block {
        &mut self.block
    }

    pub fn replace_context(&mut self, context: BlockContext) {
        self.context = context;
    }
}

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
