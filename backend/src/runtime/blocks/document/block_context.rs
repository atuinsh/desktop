use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use minijinja::{Environment, Value};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::runtime::blocks::{Block, BlockBehavior};

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

    pub fn from_block(
        block: &(impl BlockBehavior + Clone),
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(context) = block.passive_context(&ContextResolver::new())? {
            let block_with_context = BlockWithContext::new(block.clone().into_block(), context);
            let resolver = ContextResolver::from_blocks(&[block_with_context]);
            Ok(Self::from_resolver(&resolver))
        } else {
            Ok(Self::default())
        }
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

    /// Replaces the context with a new one
    pub fn update_context(&mut self, context: BlockContext) {
        *self.context_mut() = context;
    }
}

/// A context resolver is used to resolve templates and build a [`ResolvedContext`] from blocks.
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
        }
    }

    /// Update the resolver with the context of a block.
    /// Values are overwritten or merged as appropriate.
    pub fn push_block(&mut self, block: &BlockWithContext) {
        if let Some(var) = block.context().get::<DocumentVar>() {
            if let Ok(resolved_value) = self.resolve_template(&var.1) {
                self.vars.insert(var.0.clone(), resolved_value);
            } else {
                log::warn!("Failed to resolve template for variable {}", var.0);
            }
        }

        if let Some(dir) = block.context().get::<DocumentCwd>() {
            if let Ok(resolved_value) = self.resolve_template(&dir.0) {
                self.cwd = resolved_value;
            } else {
                log::warn!("Failed to resolve template for directory {}", dir.0);
            }
        }

        if let Some(env) = block.context().get::<DocumentEnvVar>() {
            if let Ok(resolved_value) = self.resolve_template(&env.1) {
                self.env_vars.insert(env.0.clone(), resolved_value);
            } else {
                log::warn!(
                    "Failed to resolve template for environment variable {}",
                    env.0
                );
            }
        }

        if let Some(host) = block.context().get::<DocumentSshHost>() {
            if let Some(host) = host.0.as_ref() {
                if let Ok(resolved_value) = self.resolve_template(host) {
                    self.ssh_host = Some(resolved_value);
                } else {
                    log::warn!("Failed to resolve template for SSH host {}", host);
                }
            }
        }
    }

    /// Resolve a template string using minijinja
    /// TODO[mkt]: Support workspace data
    pub fn resolve_template(
        &self,
        template: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
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

/// Variables defined by Var blocks
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentVar(pub String, pub String);

/// Current working directory set by Directory blocks
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentCwd(pub String);

/// Environment variables set by Environment blocks
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentEnvVar(pub String, pub String);

/// SSH connection information from SshConnect blocks
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentSshHost(pub Option<String>);

/// Execution output from blocks that produce results
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockExecutionOutput {
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    // Future: dataframes, complex data structures, etc.
}
