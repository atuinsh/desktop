use std::{any::TypeId, collections::HashMap, sync::Arc};

use downcast_rs::{impl_downcast, Downcast};
use dyn_clone::DynClone;
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::blocks::Block;

/// Trait for block-specific state that can be stored and updated during execution
///
/// Each block type can define its own state struct and implement this trait.
/// The state can be accessed via downcasting from `Box<dyn BlockState>`.
pub trait BlockState: erased_serde::Serialize + Send + Sync + std::fmt::Debug + Downcast {}
impl_downcast!(BlockState);

pub type BlockStateUpdater = Box<dyn FnOnce(&mut Box<dyn BlockState>) + Send>;

/// Trait for block-specific output that can be stored and updated during execution
///
/// Each block type can define its own output struct and implement this trait.
/// The output can be accessed via downcasting from `Box<dyn BlockOutput>`.
pub trait BlockExecutionOutput:
    erased_serde::Serialize + Send + Sync + std::fmt::Debug + Downcast + DynClone
{
    fn get_template_value(&self, _key: &str) -> Option<minijinja::Value> {
        None
    }

    fn enumerate_template_keys(&self) -> minijinja::value::Enumerator {
        minijinja::value::Enumerator::NonEnumerable
    }
}
impl_downcast!(BlockExecutionOutput);

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
            .and_then(|boxed| boxed.downcast_ref::<T>())
    }

    /// Get a mutable typed value from this block's context
    ///
    /// Returns None if no value of this type exists in the context.
    pub fn get_mut<T: BlockContextItem + 'static>(&mut self) -> Option<&mut T> {
        self.entries
            .get_mut(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast_mut::<T>())
    }
}

/// Trait for easily adding variables to a block context
///
/// This trait is implemented for `BlockContext` and can be used to add variables to a block context.
pub trait BlockVars {
    fn add_var(&mut self, var_name: String, var_value: String, var_source: String);
    fn add_env(&mut self, env_name: String, env_value: String);
    fn set_cwd(&mut self, cwd: String);
    fn set_ssh_host(&mut self, ssh_host: Option<String>);
}

impl BlockVars for BlockContext {
    fn add_var(&mut self, var_name: String, var_value: String, var_source: String) {
        if let Some(vars) = self.get_mut::<DocumentVars>() {
            vars.push(DocumentVar::new(var_name, var_value, var_source));
        } else {
            let mut vars = DocumentVars::new();
            vars.push(DocumentVar::new(var_name, var_value, var_source));
            self.insert(vars);
        }
    }

    fn add_env(&mut self, env_name: String, env_value: String) {
        if let Some(envs) = self.get_mut::<DocumentEnvVars>() {
            envs.push(env_name, env_value);
        } else {
            let mut envs = DocumentEnvVars::new();
            envs.push(env_name, env_value);
            self.insert(envs);
        }
    }

    fn set_cwd(&mut self, cwd: String) {
        self.insert(DocumentCwd(cwd));
    }

    fn set_ssh_host(&mut self, ssh_host: Option<String>) {
        self.insert(DocumentSshHost(ssh_host));
    }
}

impl Clone for BlockContext {
    fn clone(&self) -> Self {
        let mut entries = HashMap::new();
        for (type_id, item) in &self.entries {
            entries.insert(*type_id, dyn_clone::clone_box(&**item));
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
            let type_id = (&*item as &dyn std::any::Any).type_id();
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
pub struct DocumentBlock {
    block: Block,
    passive_context: BlockContext,
    active_context: BlockContext,
    state: Option<Box<dyn BlockState>>,
    execution_output: Option<Arc<dyn BlockExecutionOutput>>,
}

impl DocumentBlock {
    pub fn new(
        block: Block,
        passive_context: BlockContext,
        active_context: Option<BlockContext>,
        state: Option<Box<dyn BlockState>>,
        execution_output: Option<Arc<dyn BlockExecutionOutput>>,
    ) -> Self {
        Self {
            block,
            passive_context,
            active_context: active_context.unwrap_or_default(),
            state,
            execution_output,
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

    pub fn execution_output(&self) -> Option<Arc<dyn BlockExecutionOutput>> {
        self.execution_output.clone()
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

    pub fn replace_execution_output(&mut self, output: Box<dyn BlockExecutionOutput>) {
        self.execution_output = Some(Arc::from(output));
    }
}

/// A trait for items that can be stored in a block context. Methods in this trait are used
/// to implement serialization and deserialization for block context items, as well as handle
/// cloning and type identification.
///
/// Any type that implements this trait should use `#[typetag::serde]` to implement serialization
/// and deserialization.
#[typetag::serde(tag = "type")]
pub trait BlockContextItem: std::fmt::Debug + Send + Sync + DynClone + Downcast {}

dyn_clone::clone_trait_object!(BlockContextItem);
impl_downcast!(BlockContextItem);

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
impl BlockContextItem for DocumentVar {}

/// Container for multiple variables, used when a block produces several variables at once
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentVars {
    vars: Vec<DocumentVar>,
}

impl DocumentVars {
    pub fn new() -> Self {
        Self { vars: Vec::new() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            vars: Vec::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, var: DocumentVar) {
        self.vars.push(var);
    }

    pub fn insert(&mut self, name: String, value: String, source: String) {
        self.vars.push(DocumentVar::new(name, value, source));
    }

    pub fn iter(&self) -> impl Iterator<Item = &DocumentVar> {
        self.vars.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.vars.is_empty()
    }

    pub fn len(&self) -> usize {
        self.vars.len()
    }
}

impl FromIterator<DocumentVar> for DocumentVars {
    fn from_iter<T: IntoIterator<Item = DocumentVar>>(iter: T) -> Self {
        Self {
            vars: iter.into_iter().collect(),
        }
    }
}

impl IntoIterator for DocumentVars {
    type Item = DocumentVar;
    type IntoIter = std::vec::IntoIter<DocumentVar>;

    fn into_iter(self) -> Self::IntoIter {
        self.vars.into_iter()
    }
}

#[typetag::serde]
impl BlockContextItem for DocumentVars {}

/// Current working directory set by directory blocks
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentCwd(pub String);

#[typetag::serde]
impl BlockContextItem for DocumentCwd {}

/// Environment variables set by environment blocks
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentEnvVar(pub String, pub String);

#[typetag::serde]
impl BlockContextItem for DocumentEnvVar {}

/// Container for multiple environment variables (used when importing from sub-runbooks)
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentEnvVars {
    vars: Vec<DocumentEnvVar>,
}

impl DocumentEnvVars {
    pub fn new() -> Self {
        Self { vars: Vec::new() }
    }

    pub fn push(&mut self, name: String, value: String) {
        self.vars.push(DocumentEnvVar(name, value));
    }

    pub fn iter(&self) -> impl Iterator<Item = &DocumentEnvVar> {
        self.vars.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.vars.is_empty()
    }
}

#[typetag::serde]
impl BlockContextItem for DocumentEnvVars {}

/// SSH connection information from SSH connection and host blocks
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentSshHost(pub Option<String>);

#[typetag::serde]
impl BlockContextItem for DocumentSshHost {}

/// Identity key configuration from SSH Connect block
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum SshIdentityKeyConfig {
    /// No custom key - use SSH config/agent defaults
    None,
    /// Key content pasted directly
    Paste { content: String },
    /// Path to key file on local machine
    Path { path: String },
}

/// SSH certificate configuration from SSH Connect block
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum SshCertificateConfig {
    /// No custom certificate - auto-detect from key path or none
    None,
    /// Certificate content pasted directly
    Paste { content: String },
    /// Path to certificate file on local machine
    Path { path: String },
}

/// Rich SSH configuration from SSH Connect block
///
/// This provides detailed SSH connection settings that override ~/.ssh/config.
/// The `user_host` field is kept for backwards compatibility with the simpler
/// `DocumentSshHost` context item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSshConfig {
    /// The original user@host:port string (for display/backwards compat)
    pub user_host: String,
    /// Optional user override
    pub user: Option<String>,
    /// Optional hostname override
    pub hostname: Option<String>,
    /// Optional port override
    pub port: Option<u16>,
    /// Identity key configuration
    pub identity_key: Option<SshIdentityKeyConfig>,
    /// SSH certificate configuration
    pub certificate: Option<SshCertificateConfig>,
}

#[typetag::serde]
impl BlockContextItem for DocumentSshConfig {}
