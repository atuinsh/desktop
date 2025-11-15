use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use minijinja::{value::Object, Environment, Value};
use serde::{ser::SerializeSeq, Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::runtime::blocks::{Block, BlockBehavior};
use crate::runtime::document::actor::LocalValueProvider;

/// A single block's context - can store multiple typed values
#[derive(Default, Debug)]
pub struct BlockContext {
    entries: HashMap<TypeId, Box<dyn BlockContextItem>>,
}

impl BlockContext {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Insert a typed value into this block's context
    pub fn insert<T: BlockContextItem + 'static>(&mut self, value: T) {
        self.entries.insert(TypeId::of::<T>(), Box::new(value));
    }

    /// Get a typed value from this block's context
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

#[async_trait::async_trait]
pub trait BlockContextStorage: Send + Sync {
    async fn save(
        &self,
        document_id: &str,
        block_id: &Uuid,
        context: &BlockContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn load(
        &self,
        document_id: &str,
        block_id: &Uuid,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete(
        &self,
        document_id: &str,
        block_id: &Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn delete_for_document(
        &self,
        runbook_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// A struct representing the resolved context of a block.
/// Since it's built from a `ContextResolver`, it's a snapshot
/// of the final context based on the blocks above it.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedContext {
    pub variables: HashMap<String, String>,
    pub cwd: String,
    pub env_vars: HashMap<String, String>,
    pub ssh_host: Option<String>,
}

impl ResolvedContext {
    pub fn from_resolver(resolver: &ContextResolver) -> Self {
        Self {
            variables: resolver.vars().clone(),
            cwd: resolver.cwd().to_string(),
            env_vars: resolver.env_vars().clone(),
            ssh_host: resolver.ssh_host().cloned(),
        }
    }

    pub async fn from_block(
        block: &(impl BlockBehavior + Clone),
        block_local_value_provider: Option<&dyn LocalValueProvider>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(context) = block
            .passive_context(&ContextResolver::new(), block_local_value_provider)
            .await?
        {
            let block_with_context =
                BlockWithContext::new(block.clone().into_block(), context, None);
            let resolver = ContextResolver::from_blocks(&[block_with_context]);
            Ok(Self::from_resolver(&resolver))
        } else {
            Ok(Self::default())
        }
    }
}

#[derive(Debug)]
pub struct BlockWithContext {
    block: Block,
    passive_context: BlockContext,
    active_context: BlockContext,
}

impl BlockWithContext {
    pub fn new(
        block: Block,
        passive_context: BlockContext,
        active_context: Option<BlockContext>,
    ) -> Self {
        Self {
            block,
            passive_context,
            active_context: active_context.unwrap_or_default(),
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

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn block_mut(&mut self) -> &mut Block {
        &mut self.block
    }

    /// Replaces the context with a new one
    pub fn update_passive_context(&mut self, context: BlockContext) {
        *self.passive_context_mut() = context;
    }

    pub fn update_active_context(&mut self, context: BlockContext) {
        *self.active_context_mut() = context;
    }
}

/// A context resolver is used to resolve templates and build a [`ResolvedContext`] from blocks.
#[derive(Clone, Debug)]
pub struct ContextResolver {
    vars: HashMap<String, String>,
    cwd: String,
    env_vars: HashMap<String, String>,
    ssh_host: Option<String>,
    extra_template_context: HashMap<String, Value>,
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
            extra_template_context: HashMap::new(),
        }
    }

    pub fn add_extra_template_context(
        &mut self,
        namespace: String,
        context: impl Object + 'static,
    ) {
        self.extra_template_context
            .insert(namespace, Value::from_object(context));
    }

    /// Build a resolver from blocks (typically all blocks above the current one)
    pub fn from_blocks(blocks: &[BlockWithContext]) -> Self {
        // Process blocks in order (earlier blocks can be overridden by later ones)
        let mut resolver = Self::new();
        for block in blocks {
            resolver.push_block(block);
        }

        resolver
    }

    /// Test-only constructor to create a resolver with specific vars
    #[cfg(test)]
    pub fn with_vars(vars: HashMap<String, String>) -> Self {
        Self {
            vars,
            cwd: std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            env_vars: HashMap::new(),
            ssh_host: None,
            extra_template_context: HashMap::new(),
        }
    }

    /// Update the resolver with the context of a block.
    /// Values are overwritten or merged as appropriate.
    pub fn push_block(&mut self, block: &BlockWithContext) {
        let passive_context = block.passive_context();
        let active_context = block.active_context();

        for ctx in [passive_context, active_context] {
            if let Some(var) = ctx.get::<DocumentVar>() {
                if let Ok(resolved_value) = self.resolve_template(&var.1) {
                    self.vars.insert(var.0.clone(), resolved_value);
                } else {
                    log::warn!("Failed to resolve template for variable {}", var.0);
                }
            }

            if let Some(dir) = ctx.get::<DocumentCwd>() {
                if let Ok(resolved_value) = self.resolve_template(&dir.0) {
                    self.cwd = resolved_value;
                } else {
                    log::warn!("Failed to resolve template for directory {}", dir.0);
                }
            }

            if let Some(env) = ctx.get::<DocumentEnvVar>() {
                if let Ok(resolved_value) = self.resolve_template(&env.1) {
                    self.env_vars.insert(env.0.clone(), resolved_value);
                } else {
                    log::warn!(
                        "Failed to resolve template for environment variable {}",
                        env.0
                    );
                }
            }

            if let Some(host) = ctx.get::<DocumentSshHost>() {
                if let Some(host) = host.0.as_ref() {
                    if let Ok(resolved_value) = self.resolve_template(host) {
                        self.ssh_host = Some(resolved_value);
                    } else {
                        log::warn!("Failed to resolve template for SSH host {}", host);
                    }
                }
            }
        }
    }

    /// Resolve a template string using minijinja
    pub fn resolve_template(&self, template: &str) -> Result<String, minijinja::Error> {
        // If the string doesn't contain template markers, return it as-is
        if !template.contains("{{") && !template.contains("{%") {
            return Ok(template.to_string());
        }

        // Create a minijinja environment
        let mut env = Environment::new();
        env.set_trim_blocks(true);

        // Build the context object for template rendering
        let mut context: HashMap<&str, Value> = HashMap::new();

        // Add any extra template context
        context.extend(
            self.extra_template_context
                .iter()
                .map(|(k, v)| (k.as_str(), v.clone())),
        );

        context.insert("var", Value::from_object(self.vars.clone()));
        context.insert("env", Value::from_object(self.env_vars.clone()));

        // Render the template
        env.render_str(template, context)
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

#[typetag::serde(tag = "type")]
pub trait BlockContextItem: Any + std::fmt::Debug + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn concrete_type_id(&self) -> TypeId;
    fn clone_box(&self) -> Box<dyn BlockContextItem>;
}

/// Variables defined by Var blocks
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentVar(pub String, pub String);

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

/// Current working directory set by Directory blocks
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

/// Environment variables set by Environment blocks
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

/// SSH connection information from SshConnect blocks
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::blocks::{directory::Directory, environment::Environment, var::Var, Block};

    #[test]
    fn test_block_context_insert_and_get() {
        let mut context = BlockContext::new();

        let var = DocumentVar("TEST_VAR".to_string(), "test_value".to_string());
        context.insert(var.clone());

        let retrieved = context.get::<DocumentVar>();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), &var);
    }

    #[test]
    fn test_block_context_multiple_types() {
        let mut context = BlockContext::new();

        let var = DocumentVar("TEST_VAR".to_string(), "test_value".to_string());
        let cwd = DocumentCwd("/tmp/test".to_string());
        let env = DocumentEnvVar("PATH".to_string(), "/usr/bin".to_string());

        context.insert(var.clone());
        context.insert(cwd.clone());
        context.insert(env.clone());

        assert_eq!(context.get::<DocumentVar>(), Some(&var));
        assert_eq!(context.get::<DocumentCwd>(), Some(&cwd));
        assert_eq!(context.get::<DocumentEnvVar>(), Some(&env));
    }

    #[test]
    fn test_block_context_get_nonexistent() {
        let context = BlockContext::new();
        assert!(context.get::<DocumentVar>().is_none());
        assert!(context.get::<DocumentCwd>().is_none());
    }

    #[test]
    fn test_block_context_overwrite_same_type() {
        let mut context = BlockContext::new();

        let var1 = DocumentVar("VAR1".to_string(), "value1".to_string());
        let var2 = DocumentVar("VAR2".to_string(), "value2".to_string());

        context.insert(var1);
        context.insert(var2.clone());

        let retrieved = context.get::<DocumentVar>();
        assert_eq!(retrieved, Some(&var2));
    }

    #[test]
    fn test_block_context_serialization_roundtrip() {
        let mut context = BlockContext::new();

        context.insert(DocumentVar(
            "TEST_VAR".to_string(),
            "test_value".to_string(),
        ));
        context.insert(DocumentCwd("/tmp/test".to_string()));
        context.insert(DocumentEnvVar("PATH".to_string(), "/usr/bin".to_string()));

        let serialized = serde_json::to_string(&context).unwrap();
        let deserialized: BlockContext = serde_json::from_str(&serialized).unwrap();

        assert_eq!(
            deserialized.get::<DocumentVar>(),
            Some(&DocumentVar(
                "TEST_VAR".to_string(),
                "test_value".to_string()
            ))
        );
        assert_eq!(
            deserialized.get::<DocumentCwd>(),
            Some(&DocumentCwd("/tmp/test".to_string()))
        );
        assert_eq!(
            deserialized.get::<DocumentEnvVar>(),
            Some(&DocumentEnvVar("PATH".to_string(), "/usr/bin".to_string()))
        );
    }

    #[test]
    fn test_context_resolver_new() {
        let resolver = ContextResolver::new();
        assert!(resolver.vars().is_empty());
        assert!(resolver.env_vars().is_empty());
        assert!(resolver.ssh_host().is_none());
    }

    #[tokio::test]
    async fn test_context_resolver_from_single_block() {
        let var_block = Var::builder()
            .id(Uuid::new_v4())
            .name("TEST_VAR")
            .value("test_value")
            .build();

        let mut context = BlockContext::new();
        context.insert(DocumentVar(
            "TEST_VAR".to_string(),
            "test_value".to_string(),
        ));

        let block_with_context = BlockWithContext::new(Block::Var(var_block), context, None);

        let resolver = ContextResolver::from_blocks(&[block_with_context]);

        assert_eq!(
            resolver.get_var("TEST_VAR"),
            Some(&"test_value".to_string())
        );
    }

    #[tokio::test]
    async fn test_context_resolver_multiple_blocks() {
        let var_block = Var::builder()
            .id(Uuid::new_v4())
            .name("VAR1")
            .value("value1")
            .build();

        let env_block = Environment::builder()
            .id(Uuid::new_v4())
            .name("PATH")
            .value("/usr/bin")
            .build();

        let dir_block = Directory::builder()
            .id(Uuid::new_v4())
            .path("/tmp/test")
            .build();

        let mut var_context = BlockContext::new();
        var_context.insert(DocumentVar("VAR1".to_string(), "value1".to_string()));

        let mut env_context = BlockContext::new();
        env_context.insert(DocumentEnvVar("PATH".to_string(), "/usr/bin".to_string()));

        let mut dir_context = BlockContext::new();
        dir_context.insert(DocumentCwd("/tmp/test".to_string()));

        let blocks = vec![
            BlockWithContext::new(Block::Var(var_block), var_context, None),
            BlockWithContext::new(Block::Environment(env_block), env_context, None),
            BlockWithContext::new(Block::Directory(dir_block), dir_context, None),
        ];

        let resolver = ContextResolver::from_blocks(&blocks);

        assert_eq!(resolver.get_var("VAR1"), Some(&"value1".to_string()));
        assert_eq!(
            resolver.env_vars().get("PATH"),
            Some(&"/usr/bin".to_string())
        );
        assert_eq!(resolver.cwd(), "/tmp/test");
    }

    #[tokio::test]
    async fn test_context_resolver_later_blocks_override() {
        let var1 = Var::builder()
            .id(Uuid::new_v4())
            .name("SHARED_VAR")
            .value("first_value")
            .build();

        let var2 = Var::builder()
            .id(Uuid::new_v4())
            .name("SHARED_VAR")
            .value("second_value")
            .build();

        let mut context1 = BlockContext::new();
        context1.insert(DocumentVar(
            "SHARED_VAR".to_string(),
            "first_value".to_string(),
        ));

        let mut context2 = BlockContext::new();
        context2.insert(DocumentVar(
            "SHARED_VAR".to_string(),
            "second_value".to_string(),
        ));

        let blocks = vec![
            BlockWithContext::new(Block::Var(var1), context1, None),
            BlockWithContext::new(Block::Var(var2), context2, None),
        ];

        let resolver = ContextResolver::from_blocks(&blocks);

        assert_eq!(
            resolver.get_var("SHARED_VAR"),
            Some(&"second_value".to_string())
        );
    }

    #[test]
    fn test_context_resolver_template_resolution_no_template() {
        let resolver = ContextResolver::new();
        let result = resolver.resolve_template("plain text").unwrap();
        assert_eq!(result, "plain text");
    }

    #[test]
    fn test_context_resolver_template_resolution_with_var() {
        let mut vars = HashMap::new();
        vars.insert("USERNAME".to_string(), "alice".to_string());

        let resolver = ContextResolver::with_vars(vars);
        let result = resolver
            .resolve_template("Hello, {{ var.USERNAME }}!")
            .unwrap();
        assert_eq!(result, "Hello, alice!");
    }

    #[test]
    fn test_context_resolver_template_resolution_with_multiple_vars() {
        let mut vars = HashMap::new();
        vars.insert("HOST".to_string(), "example.com".to_string());
        vars.insert("PORT".to_string(), "8080".to_string());

        let resolver = ContextResolver::with_vars(vars);
        let result = resolver
            .resolve_template("Connect to {{ var.HOST }}:{{ var.PORT }}")
            .unwrap();
        assert_eq!(result, "Connect to example.com:8080");
    }

    #[test]
    fn test_context_resolver_template_resolution_with_env() {
        let resolver = ContextResolver {
            vars: HashMap::new(),
            cwd: "/test".to_string(),
            env_vars: HashMap::from([("PATH".to_string(), "/usr/bin".to_string())]),
            ssh_host: None,
            extra_template_context: HashMap::new(),
        };

        let result = resolver.resolve_template("PATH is {{ env.PATH }}").unwrap();
        assert_eq!(result, "PATH is /usr/bin");
    }

    #[test]
    fn test_context_resolver_template_resolution_var_and_env() {
        let mut vars = HashMap::new();
        vars.insert("USER".to_string(), "bob".to_string());

        let mut env_vars = HashMap::new();
        env_vars.insert("HOME".to_string(), "/home/bob".to_string());

        let resolver = ContextResolver {
            vars,
            cwd: "/test".to_string(),
            env_vars,
            ssh_host: None,
            extra_template_context: HashMap::new(),
        };

        let result = resolver
            .resolve_template("User {{ var.USER }} has home {{ env.HOME }}")
            .unwrap();
        assert_eq!(result, "User bob has home /home/bob");
    }

    #[tokio::test]
    async fn test_context_resolver_push_block() {
        let mut resolver = ContextResolver::new();

        let var_block = Var::builder()
            .id(Uuid::new_v4())
            .name("NEW_VAR")
            .value("new_value")
            .build();

        let mut context = BlockContext::new();
        context.insert(DocumentVar("NEW_VAR".to_string(), "new_value".to_string()));

        let block_with_context = BlockWithContext::new(Block::Var(var_block), context, None);

        resolver.push_block(&block_with_context);

        assert_eq!(resolver.get_var("NEW_VAR"), Some(&"new_value".to_string()));
    }

    #[tokio::test]
    async fn test_context_resolver_push_block_with_active_context() {
        let mut resolver = ContextResolver::new();

        let var_block = Var::builder()
            .id(Uuid::new_v4())
            .name("PASSIVE_VAR")
            .value("passive")
            .build();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentVar(
            "PASSIVE_VAR".to_string(),
            "passive".to_string(),
        ));

        let mut block_with_context =
            BlockWithContext::new(Block::Var(var_block), passive_context, None);

        let mut active_context = BlockContext::new();
        active_context.insert(DocumentVar("ACTIVE_VAR".to_string(), "active".to_string()));
        block_with_context.update_active_context(active_context);

        resolver.push_block(&block_with_context);

        assert_eq!(
            resolver.get_var("PASSIVE_VAR"),
            Some(&"passive".to_string())
        );
        assert_eq!(resolver.get_var("ACTIVE_VAR"), Some(&"active".to_string()));
    }

    #[test]
    fn test_resolved_context_from_resolver() {
        let mut vars = HashMap::new();
        vars.insert("TEST_VAR".to_string(), "test_value".to_string());

        let mut env_vars = HashMap::new();
        env_vars.insert("PATH".to_string(), "/usr/bin".to_string());

        let resolver = ContextResolver {
            vars: vars.clone(),
            cwd: "/tmp/test".to_string(),
            env_vars: env_vars.clone(),
            ssh_host: Some("example.com".to_string()),
            extra_template_context: HashMap::new(),
        };

        let resolved = ResolvedContext::from_resolver(&resolver);

        assert_eq!(resolved.variables, vars);
        assert_eq!(resolved.cwd, "/tmp/test");
        assert_eq!(resolved.env_vars, env_vars);
        assert_eq!(resolved.ssh_host, Some("example.com".to_string()));
    }

    #[test]
    fn test_resolved_context_serialization_roundtrip() {
        let mut vars = HashMap::new();
        vars.insert("VAR1".to_string(), "value1".to_string());

        let mut env_vars = HashMap::new();
        env_vars.insert("ENV1".to_string(), "envvalue1".to_string());

        let original = ResolvedContext {
            variables: vars,
            cwd: "/test/path".to_string(),
            env_vars,
            ssh_host: Some("test.example.com".to_string()),
        };

        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: ResolvedContext = serde_json::from_str(&serialized).unwrap();

        assert_eq!(original.variables, deserialized.variables);
        assert_eq!(original.cwd, deserialized.cwd);
        assert_eq!(original.env_vars, deserialized.env_vars);
        assert_eq!(original.ssh_host, deserialized.ssh_host);
    }

    #[test]
    fn test_resolved_context_default() {
        let resolved = ResolvedContext::default();
        assert!(resolved.variables.is_empty());
        assert!(resolved.env_vars.is_empty());
        assert!(resolved.ssh_host.is_none());
    }

    #[tokio::test]
    async fn test_block_with_context_accessors() {
        let var_block = Var::builder()
            .id(Uuid::new_v4())
            .name("TEST")
            .value("value")
            .build();

        let block_id = var_block.id;

        let mut context = BlockContext::new();
        context.insert(DocumentVar("TEST".to_string(), "value".to_string()));

        let block_with_context = BlockWithContext::new(Block::Var(var_block), context, None);

        assert_eq!(block_with_context.id(), block_id);
        assert!(block_with_context
            .passive_context()
            .get::<DocumentVar>()
            .is_some());
        assert!(block_with_context
            .active_context()
            .get::<DocumentVar>()
            .is_none());
    }

    #[tokio::test]
    async fn test_block_with_context_update_contexts() {
        let var_block = Var::builder()
            .id(Uuid::new_v4())
            .name("TEST")
            .value("value")
            .build();

        let mut block_with_context =
            BlockWithContext::new(Block::Var(var_block), BlockContext::new(), None);

        let mut new_passive = BlockContext::new();
        new_passive.insert(DocumentVar("NEW_VAR".to_string(), "new_value".to_string()));
        block_with_context.update_passive_context(new_passive);

        let mut new_active = BlockContext::new();
        new_active.insert(DocumentCwd("/new/path".to_string()));
        block_with_context.update_active_context(new_active);

        assert!(block_with_context
            .passive_context()
            .get::<DocumentVar>()
            .is_some());
        assert!(block_with_context
            .active_context()
            .get::<DocumentCwd>()
            .is_some());
    }

    #[tokio::test]
    async fn test_document_context_items_equality() {
        let var1 = DocumentVar("name".to_string(), "value".to_string());
        let var2 = DocumentVar("name".to_string(), "value".to_string());
        let var3 = DocumentVar("name".to_string(), "different".to_string());

        assert_eq!(var1, var2);
        assert_ne!(var1, var3);
    }

    #[test]
    fn test_document_ssh_host_none() {
        let ssh_host = DocumentSshHost(None);
        let mut context = BlockContext::new();
        context.insert(ssh_host.clone());

        let retrieved = context.get::<DocumentSshHost>();
        assert_eq!(retrieved, Some(&DocumentSshHost(None)));
    }

    #[test]
    fn test_document_ssh_host_some() {
        let ssh_host = DocumentSshHost(Some("example.com".to_string()));
        let mut context = BlockContext::new();
        context.insert(ssh_host.clone());

        let retrieved = context.get::<DocumentSshHost>();
        assert_eq!(
            retrieved,
            Some(&DocumentSshHost(Some("example.com".to_string())))
        );
    }

    #[test]
    fn test_block_execution_output() {
        let output = BlockExecutionOutput {
            exit_code: Some(0),
            stdout: Some("output text".to_string()),
            stderr: Some("error text".to_string()),
        };

        let mut context = BlockContext::new();
        context.insert(output.clone());

        let retrieved = context.get::<BlockExecutionOutput>();
        assert_eq!(retrieved, Some(&output));
    }

    #[test]
    fn test_template_with_nested_variables() {
        let mut vars = HashMap::new();
        vars.insert("BASE_URL".to_string(), "api.example.com".to_string());
        vars.insert("VERSION".to_string(), "v1".to_string());
        vars.insert("ENDPOINT".to_string(), "users".to_string());

        let resolver = ContextResolver::with_vars(vars);
        let result = resolver
            .resolve_template("https://{{ var.BASE_URL }}/{{ var.VERSION }}/{{ var.ENDPOINT }}")
            .unwrap();
        assert_eq!(result, "https://api.example.com/v1/users");
    }

    #[test]
    fn test_template_with_extra_template_context() {
        let mut extra = HashMap::new();
        extra.insert("BASE_URL".to_string(), "api.example.com".to_string());
        extra.insert("VERSION".to_string(), "v1".to_string());
        extra.insert("ENDPOINT".to_string(), "users".to_string());

        let mut resolver = ContextResolver::new();
        resolver.add_extra_template_context("extra".to_string(), extra);
        let result = resolver
            .resolve_template(
                "https://{{ extra.BASE_URL }}/{{ extra.VERSION }}/{{ extra.ENDPOINT }}",
            )
            .unwrap();
        assert_eq!(result, "https://api.example.com/v1/users");
    }

    #[tokio::test]
    async fn test_complex_context_scenario() {
        let var1 = Var::builder()
            .id(Uuid::new_v4())
            .name("USERNAME")
            .value("alice")
            .build();

        let env1 = Environment::builder()
            .id(Uuid::new_v4())
            .name("HOME")
            .value("/home/alice")
            .build();

        let dir1 = Directory::builder()
            .id(Uuid::new_v4())
            .path("/home/alice/projects")
            .build();

        let var2 = Var::builder()
            .id(Uuid::new_v4())
            .name("PROJECT")
            .value("myapp")
            .build();

        let mut context1 = BlockContext::new();
        context1.insert(DocumentVar("USERNAME".to_string(), "alice".to_string()));

        let mut context2 = BlockContext::new();
        context2.insert(DocumentEnvVar(
            "HOME".to_string(),
            "/home/alice".to_string(),
        ));

        let mut context3 = BlockContext::new();
        context3.insert(DocumentCwd("/home/alice/projects".to_string()));

        let mut context4 = BlockContext::new();
        context4.insert(DocumentVar("PROJECT".to_string(), "myapp".to_string()));

        let blocks = vec![
            BlockWithContext::new(Block::Var(var1), context1, None),
            BlockWithContext::new(Block::Environment(env1), context2, None),
            BlockWithContext::new(Block::Directory(dir1), context3, None),
            BlockWithContext::new(Block::Var(var2), context4, None),
        ];

        let resolver = ContextResolver::from_blocks(&blocks);
        let resolved = ResolvedContext::from_resolver(&resolver);

        assert_eq!(
            resolved.variables.get("USERNAME"),
            Some(&"alice".to_string())
        );
        assert_eq!(
            resolved.variables.get("PROJECT"),
            Some(&"myapp".to_string())
        );
        assert_eq!(
            resolved.env_vars.get("HOME"),
            Some(&"/home/alice".to_string())
        );
        assert_eq!(resolved.cwd, "/home/alice/projects");
    }
}
