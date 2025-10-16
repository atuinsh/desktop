use minijinja::{Environment, Value};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::runtime::blocks::document::block_context::{
    BlockContext, BlockWithContext, DocumentCwd, DocumentEnvVar, DocumentSshHost, DocumentVar,
};
use crate::runtime::blocks::document::document::Document;
use crate::runtime::blocks::document::{DocumentCommand, DocumentError};
use crate::runtime::blocks::Block;
use crate::runtime::events::EventBus;

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
    pub command_tx: mpsc::UnboundedSender<DocumentCommand>,

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
