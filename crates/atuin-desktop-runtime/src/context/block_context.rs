use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::blocks::Block;

/// Trait for block-specific state that can be stored and updated during execution
///
/// Each block type can define its own state struct and implement this trait.
/// The state can be accessed via downcasting from `Box<dyn BlockState>`.
pub trait BlockState: erased_serde::Serialize + Send + Sync + std::fmt::Debug + Any {
    /// Returns a reference to the state as `Any` for downcasting
    fn as_any(&self) -> &dyn Any;

    /// Returns a mutable reference to the state as `Any` for downcasting
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Extension trait for easier downcasting of `Box<dyn BlockState>`
pub trait BlockStateExt {
    /// Attempt to downcast to a concrete state type
    fn downcast_ref<T: BlockState>(&self) -> Option<&T>;

    /// Attempt to mutably downcast to a concrete state type
    fn downcast_mut<T: BlockState>(&mut self) -> Option<&mut T>;
}

impl BlockStateExt for Box<dyn BlockState> {
    fn downcast_ref<T: BlockState>(&self) -> Option<&T> {
        self.as_any().downcast_ref::<T>()
    }

    fn downcast_mut<T: BlockState>(&mut self) -> Option<&mut T> {
        self.as_any_mut().downcast_mut::<T>()
    }
}

pub type BlockStateUpdater = Box<dyn FnOnce(&mut Box<dyn BlockState>) + Send>;

/// Container for block context items
///
/// Block context items implement the [`BlockContextItem`] trait and are stored
/// in a type-safe map. Only one item of each type can be stored, as items are
/// keyed by their concrete type ID.
///
/// Context can include variables, environment settings, execution outputs, etc.
#[derive(Default, Debug)]
pub struct BlockContext {
    entries: HashMap<TypeId, Box<dyn BlockContextItem>>,
}

impl BlockContext {
    /// Create a new empty block context
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Insert a typed value into this block's context
    ///
    /// If a value of this type already exists, it will be replaced.
    pub fn insert<T: BlockContextItem + 'static>(&mut self, value: T) {
        self.entries.insert(TypeId::of::<T>(), Box::new(value));
    }

    /// Get a typed value from this block's context
    ///
    /// Returns None if no value of this type exists in the context.
    pub fn get<T: BlockContextItem + 'static>(&self) -> Option<&T> {
        self.entries
            .get(&TypeId::of::<T>())
            .and_then(|boxed| boxed.as_any().downcast_ref::<T>())
    }
}

impl Clone for BlockContext {
    fn clone(&self) -> Self {
        let mut entries = HashMap::new();
        for (type_id, item) in &self.entries {
            entries.insert(*type_id, item.clone_box());
        }
        Self { entries }
    }
}

impl Serialize for BlockContext {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.entries.len()))?;
        for value in self.entries.values() {
            seq.serialize_element(value.as_ref())?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for BlockContext {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let items: Vec<Box<dyn BlockContextItem>> = Vec::deserialize(deserializer)?;
        let mut context = Self::new();
        for item in items {
            let type_id = item.concrete_type_id();
            context.entries.insert(type_id, item);
        }
        Ok(context)
    }
}

/// A block paired with its passive and active context
///
/// - **Passive context**: Values available before execution (e.g., variable definitions)
/// - **Active context**: Values produced during execution (e.g., command output)
#[derive(Debug)]
pub struct BlockWithContext {
    block: Block,
    passive_context: BlockContext,
    active_context: BlockContext,
    state: Option<Box<dyn BlockState>>,
}

impl BlockWithContext {
    pub fn new(
        block: Block,
        passive_context: BlockContext,
        active_context: Option<BlockContext>,
        state: Option<Box<dyn BlockState>>,
    ) -> Self {
        Self {
            block,
            passive_context,
            active_context: active_context.unwrap_or_default(),
            state,
        }
    }

    pub fn id(&self) -> Uuid {
        self.block.id()
    }

    pub fn passive_context(&self) -> &BlockContext {
        &self.passive_context
    }

    pub fn passive_context_mut(&mut self) -> &mut BlockContext {
        &mut self.passive_context
    }

    pub fn active_context(&self) -> &BlockContext {
        &self.active_context
    }

    pub fn active_context_mut(&mut self) -> &mut BlockContext {
        &mut self.active_context
    }

    pub fn state(&self) -> Option<&dyn BlockState> {
        self.state.as_ref().map(|s| s.as_ref())
    }

    pub fn state_mut(&mut self) -> Option<&mut Box<dyn BlockState>> {
        self.state.as_mut()
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn block_mut(&mut self) -> &mut Block {
        &mut self.block
    }

    /// Replaces the context with a new one
    pub fn replace_passive_context(&mut self, context: BlockContext) {
        *self.passive_context_mut() = context;
    }

    pub fn replace_active_context(&mut self, context: BlockContext) {
        *self.active_context_mut() = context;
    }
}

/// A trait for items that can be stored in a block context. Methods in this trait are used
/// to implement serialization and deserialization for block context items, as well as handle
/// cloning and type identification.
///
/// Any type that implements this trait should use `#[typetag::serde]` to implement serialization
/// and deserialization.
#[typetag::serde(tag = "type")]
pub trait BlockContextItem: Any + std::fmt::Debug + Send + Sync {
    /// Returns a reference to the item as a dynamic [`Any`] type.
    /// You can implement this method by returning `self` from the method.
    fn as_any(&self) -> &dyn Any;

    /// Returns the concrete type ID of the item. This is used to identify the item when it is
    /// stored in a hash map.
    /// You can implement this method by returning `TypeId::of::<Self>()`.
    fn concrete_type_id(&self) -> TypeId;

    /// Returns a boxed clone of the item. This is used to clone the item when it is stored in a
    /// hash map.
    /// You can implement this method by returning `Box::new(self.clone())`.
    fn clone_box(&self) -> Box<dyn BlockContextItem>;
}

/// Variables defined by template variable blocks
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentVar {
    pub name: String,
    pub value: String,
    pub source: String,
}

impl DocumentVar {
    pub fn new(name: String, value: String, source: String) -> Self {
        Self {
            name,
            value,
            source,
        }
    }
}

#[typetag::serde]
impl BlockContextItem for DocumentVar {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn concrete_type_id(&self) -> TypeId {
        TypeId::of::<Self>()
    }

    fn clone_box(&self) -> Box<dyn BlockContextItem> {
        Box::new(self.clone())
    }
}

/// Current working directory set by directory blocks
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentCwd(pub String);

#[typetag::serde]
impl BlockContextItem for DocumentCwd {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn concrete_type_id(&self) -> TypeId {
        TypeId::of::<Self>()
    }

    fn clone_box(&self) -> Box<dyn BlockContextItem> {
        Box::new(self.clone())
    }
}

/// Environment variables set by environment blocks
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentEnvVar(pub String, pub String);

#[typetag::serde]
impl BlockContextItem for DocumentEnvVar {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn concrete_type_id(&self) -> TypeId {
        TypeId::of::<Self>()
    }

    fn clone_box(&self) -> Box<dyn BlockContextItem> {
        Box::new(self.clone())
    }
}

/// SSH connection information from SSH connection and host blocks
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentSshHost(pub Option<String>);

#[typetag::serde]
impl BlockContextItem for DocumentSshHost {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn concrete_type_id(&self) -> TypeId {
        TypeId::of::<Self>()
    }

    fn clone_box(&self) -> Box<dyn BlockContextItem> {
        Box::new(self.clone())
    }
}

/// Execution output from blocks that produce results
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockExecutionOutput {
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    // Future: dataframes, complex data structures, etc.
}

#[typetag::serde]
impl BlockContextItem for BlockExecutionOutput {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn concrete_type_id(&self) -> TypeId {
        TypeId::of::<Self>()
    }

    fn clone_box(&self) -> Box<dyn BlockContextItem> {
        Box::new(self.clone())
    }
}
