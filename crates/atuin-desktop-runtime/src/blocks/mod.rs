//! Block types and execution behaviors
//!
//! This module defines all block types that can be executed in a runbook document.
//! Blocks represent individual operations like running scripts, querying databases,
//! making HTTP requests, or setting variables.
//!
//! Each block type implements the [`BlockBehavior`] trait which defines how blocks
//! provide context and execute their operations.

pub(crate) mod clickhouse;
pub(crate) mod directory;
pub(crate) mod dropdown;
pub(crate) mod editor;
pub(crate) mod environment;
pub(crate) mod host;
pub(crate) mod http;
pub(crate) mod kubernetes;
pub(crate) mod local_directory;
pub(crate) mod local_var;
pub(crate) mod markdown_render;
pub(crate) mod mysql;
pub(crate) mod pause;
pub(crate) mod postgres;
pub(crate) mod prometheus;
pub(crate) mod query_block;
pub(crate) mod script;
pub(crate) mod sql_block;
pub(crate) mod sqlite;
pub(crate) mod ssh_connect;
pub(crate) mod terminal;
pub(crate) mod var;
pub(crate) mod var_display;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use query_block::{BlockExecutionError, QueryBlockBehavior, QueryBlockError};
pub use script::ScriptOutput;
pub use sql_block::{
    SqlBlockBehavior, SqlBlockError, SqlBlockExecutionResult, SqlQueryResult, SqlStatementResult,
};

use crate::{
    client::LocalValueProvider,
    context::{BlockContext, BlockState, ContextResolver},
    execution::{ExecutionContext, ExecutionHandle},
};

/// Block types that are known to exist but are not supported for execution
///
/// These blocks are typically display-only blocks like paragraphs, images, etc.
/// that don't have executable behavior.
pub const KNOWN_UNSUPPORTED_BLOCKS: &[&str] = &[
    "audio",
    "bulletedListItem",
    "checkListItem",
    "codeBlock",
    "file",
    "heading",
    "horizontal_rule",
    "image",
    "numberedListItem",
    "paragraph",
    "quote",
    "table",
    "video",
];

/// Trait for parsing block data from a document JSON representation
pub trait FromDocument: Sized {
    /// Parse block data from a JSON value
    ///
    /// # Errors
    /// Returns an error string if the JSON data cannot be parsed into this block type
    fn from_document(block_data: &serde_json::Value) -> Result<Self, String>;
}

/// Core trait defining block execution behavior
///
/// All block types must implement this trait to provide context and execute operations.
/// Blocks can provide passive context (values available before execution) and
/// active context (values produced during execution).
#[async_trait]
pub trait BlockBehavior: Sized + Send + Sync {
    /// Convert this block into the generic Block enum
    fn into_block(self) -> Block;

    /// Get the unique identifier for this block
    fn id(&self) -> Uuid;

    /// Create the initial state for the block
    fn create_state(&self) -> Option<Box<dyn BlockState>> {
        None
    }

    /// Provide passive context before execution
    ///
    /// Passive context is evaluated based on the block's configuration and
    /// doesn't require execution. For example, a variable block provides its
    /// value as passive context.
    ///
    /// # Arguments
    /// * `resolver` - Context resolver for template interpolation
    /// * `block_local_value_provider` - Optional provider for local values
    async fn passive_context(
        &self,
        _resolver: &ContextResolver,
        _block_local_value_provider: Option<&dyn LocalValueProvider>,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    /// Execute this block
    ///
    /// Executes the block's operation and returns an execution handle for tracking
    /// the block's lifecycle and cancellation.
    ///
    /// # Arguments
    /// * `context` - Execution context providing access to document state and resources
    async fn execute(
        self,
        _context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
}

/// Enum representing all supported block types
///
/// Each variant corresponds to a specific block implementation with its own
/// behavior and configuration.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum Block {
    Terminal(terminal::Terminal),
    Script(script::Script),
    Postgres(postgres::Postgres),
    Http(http::Http),
    Prometheus(prometheus::Prometheus),
    Clickhouse(clickhouse::Clickhouse),
    Mysql(mysql::Mysql),
    Kubernetes(kubernetes::Kubernetes),

    #[serde(rename = "sqlite")]
    SQLite(sqlite::SQLite),

    LocalVar(local_var::LocalVar),
    Var(var::Var),
    Environment(environment::Environment),
    Directory(directory::Directory),
    LocalDirectory(local_directory::LocalDirectory),
    SshConnect(ssh_connect::SshConnect),
    Host(host::Host),
    VarDisplay(var_display::VarDisplay),
    MarkdownRender(markdown_render::MarkdownRender),
    Editor(editor::Editor),
    Dropdown(dropdown::Dropdown),
    Pause(pause::Pause),
}

impl Block {
    /// Get the unique identifier of this block
    pub fn id(&self) -> Uuid {
        match self {
            Block::Terminal(terminal) => terminal.id,
            Block::Script(script) => script.id,
            Block::SQLite(sqlite) => sqlite.id,
            Block::Postgres(postgres) => postgres.id,
            Block::Http(http) => http.id,
            Block::Prometheus(prometheus) => prometheus.id,
            Block::Clickhouse(clickhouse) => clickhouse.id,
            Block::Mysql(mysql) => mysql.id,
            Block::Kubernetes(kubernetes) => kubernetes.id,

            Block::LocalVar(local_var) => local_var.id,
            Block::Var(var) => var.id,
            Block::Environment(environment) => environment.id,
            Block::Directory(directory) => directory.id,
            Block::LocalDirectory(local_directory) => local_directory.id,
            Block::SshConnect(ssh_connect) => ssh_connect.id,
            Block::Host(host) => host.id,
            Block::VarDisplay(var_display) => var_display.id,
            Block::MarkdownRender(markdown_render) => markdown_render.id,
            Block::Editor(editor) => editor.id,
            Block::Dropdown(dropdown) => dropdown.id,
            Block::Pause(pause) => pause.id,
        }
    }

    /// Get the display name of this block
    #[allow(dead_code)]
    pub fn name(&self) -> String {
        match self {
            Block::Terminal(terminal) => terminal.name.clone(),
            Block::Script(script) => script.name.clone(),
            Block::SQLite(sqlite) => sqlite.name.clone(),
            Block::Postgres(postgres) => postgres.name.clone(),
            Block::Http(http) => http.name.clone(),
            Block::Prometheus(prometheus) => prometheus.name.clone(),
            Block::Clickhouse(clickhouse) => clickhouse.name.clone(),
            Block::Mysql(mysql) => mysql.name.clone(),
            Block::Kubernetes(kubernetes) => kubernetes.name.clone(),
            Block::Dropdown(dropdown) => dropdown.name.clone(),

            Block::Editor(_) => "".to_string(),
            Block::LocalVar(_) => "".to_string(),
            Block::Var(_) => "".to_string(),
            Block::Environment(_) => "".to_string(),
            Block::Directory(_) => "".to_string(),
            Block::LocalDirectory(_) => "".to_string(),
            Block::SshConnect(_) => "".to_string(),
            Block::Host(_) => "".to_string(),
            Block::VarDisplay(_) => "".to_string(),
            Block::MarkdownRender(_) => "".to_string(),
            Block::Pause(_) => "".to_string(),
        }
    }

    /// Parse a block from document JSON data
    ///
    /// # Arguments
    /// * `block_data` - JSON representation of the block
    ///
    /// # Errors
    /// Returns an error if the block type is unknown or parsing fails
    pub fn from_document(block_data: &serde_json::Value) -> Result<Self, String> {
        let block_type = block_data
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or("Block has no type")?;

        match block_type {
            "script" => Ok(Block::Script(script::Script::from_document(block_data)?)),
            "terminal" | "run" => Ok(Block::Terminal(terminal::Terminal::from_document(
                block_data,
            )?)),
            "postgres" => Ok(Block::Postgres(postgres::Postgres::from_document(
                block_data,
            )?)),
            "http" => Ok(Block::Http(http::Http::from_document(block_data)?)),
            "prometheus" => Ok(Block::Prometheus(prometheus::Prometheus::from_document(
                block_data,
            )?)),
            "clickhouse" => Ok(Block::Clickhouse(clickhouse::Clickhouse::from_document(
                block_data,
            )?)),
            "mysql" => Ok(Block::Mysql(mysql::Mysql::from_document(block_data)?)),
            "kubernetes-get" => Ok(Block::Kubernetes(kubernetes::Kubernetes::from_document(
                block_data,
            )?)),
            "sqlite" => Ok(Block::SQLite(sqlite::SQLite::from_document(block_data)?)),
            "local-var" => Ok(Block::LocalVar(local_var::LocalVar::from_document(
                block_data,
            )?)),
            "var" => Ok(Block::Var(var::Var::from_document(block_data)?)),
            "env" => Ok(Block::Environment(environment::Environment::from_document(
                block_data,
            )?)),
            "directory" => Ok(Block::Directory(directory::Directory::from_document(
                block_data,
            )?)),
            "local-directory" => Ok(Block::LocalDirectory(
                local_directory::LocalDirectory::from_document(block_data)?,
            )),
            "ssh-connect" => Ok(Block::SshConnect(ssh_connect::SshConnect::from_document(
                block_data,
            )?)),
            "host-select" => Ok(Block::Host(host::Host::from_document(block_data)?)),
            "var_display" => Ok(Block::VarDisplay(var_display::VarDisplay::from_document(
                block_data,
            )?)),
            "markdown_render" => Ok(Block::MarkdownRender(
                markdown_render::MarkdownRender::from_document(block_data)?,
            )),
            "editor" => Ok(Block::Editor(editor::Editor::from_document(block_data)?)),
            "dropdown" => Ok(Block::Dropdown(dropdown::Dropdown::from_document(
                block_data,
            )?)),
            "pause" => Ok(Block::Pause(pause::Pause::from_document(block_data)?)),
            _ => Err(format!("Unknown block type: {}", block_type)),
        }
    }

    /// Get the passive context for this block
    ///
    /// Passive context includes values that are available before execution,
    /// such as variable definitions or environment settings.
    ///
    /// # Arguments
    /// * `resolver` - Context resolver for template interpolation
    /// * `block_local_value_provider` - Optional provider for local values
    pub async fn passive_context(
        &self,
        resolver: &ContextResolver,
        block_local_value_provider: Option<&dyn LocalValueProvider>,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Block::LocalVar(local_var) => {
                local_var
                    .passive_context(resolver, block_local_value_provider)
                    .await
            }
            Block::Var(var) => {
                var.passive_context(resolver, block_local_value_provider)
                    .await
            }
            Block::Environment(environment) => {
                environment
                    .passive_context(resolver, block_local_value_provider)
                    .await
            }
            Block::Directory(directory) => {
                directory
                    .passive_context(resolver, block_local_value_provider)
                    .await
            }
            Block::LocalDirectory(local_directory) => {
                local_directory
                    .passive_context(resolver, block_local_value_provider)
                    .await
            }
            Block::SshConnect(ssh_connect) => {
                ssh_connect
                    .passive_context(resolver, block_local_value_provider)
                    .await
            }
            Block::Host(host) => {
                host.passive_context(resolver, block_local_value_provider)
                    .await
            }
            Block::VarDisplay(var_display) => {
                var_display
                    .passive_context(resolver, block_local_value_provider)
                    .await
            }
            Block::MarkdownRender(markdown_render) => {
                markdown_render
                    .passive_context(resolver, block_local_value_provider)
                    .await
            }
            Block::Editor(editor) => {
                editor
                    .passive_context(resolver, block_local_value_provider)
                    .await
            }
            Block::Dropdown(dropdown) => {
                dropdown
                    .passive_context(resolver, block_local_value_provider)
                    .await
            }
            Block::Terminal(terminal) => {
                terminal
                    .passive_context(resolver, block_local_value_provider)
                    .await
            }
            Block::Script(script) => {
                script
                    .passive_context(resolver, block_local_value_provider)
                    .await
            }
            Block::SQLite(sqlite) => {
                sqlite
                    .passive_context(resolver, block_local_value_provider)
                    .await
            }
            Block::Postgres(postgres) => {
                postgres
                    .passive_context(resolver, block_local_value_provider)
                    .await
            }
            Block::Http(http) => {
                http.passive_context(resolver, block_local_value_provider)
                    .await
            }
            Block::Prometheus(prometheus) => {
                prometheus
                    .passive_context(resolver, block_local_value_provider)
                    .await
            }
            Block::Clickhouse(clickhouse) => {
                clickhouse
                    .passive_context(resolver, block_local_value_provider)
                    .await
            }
            Block::Mysql(clickhouse) => {
                clickhouse
                    .passive_context(resolver, block_local_value_provider)
                    .await
            }
            Block::Kubernetes(kubernetes) => {
                kubernetes
                    .passive_context(resolver, block_local_value_provider)
                    .await
            }
            Block::Pause(pause) => {
                pause
                    .passive_context(resolver, block_local_value_provider)
                    .await
            }
        }
    }

    /// Create the initial state for the block
    pub fn create_state(&self) -> Option<Box<dyn BlockState>> {
        match self {
            Block::Terminal(terminal) => terminal.create_state(),
            Block::Script(script) => script.create_state(),
            Block::SQLite(sqlite) => sqlite.create_state(),
            Block::Postgres(postgres) => postgres.create_state(),
            Block::Http(http) => http.create_state(),
            Block::Prometheus(prometheus) => prometheus.create_state(),
            Block::Clickhouse(clickhouse) => clickhouse.create_state(),
            Block::Mysql(mysql) => mysql.create_state(),
            Block::Kubernetes(kubernetes) => kubernetes.create_state(),
            Block::LocalVar(local_var) => local_var.create_state(),
            Block::Var(var) => var.create_state(),
            Block::Environment(environment) => environment.create_state(),
            Block::Directory(directory) => directory.create_state(),
            Block::LocalDirectory(local_directory) => local_directory.create_state(),
            Block::SshConnect(ssh_connect) => ssh_connect.create_state(),
            Block::Host(host) => host.create_state(),
            Block::VarDisplay(var_display) => var_display.create_state(),
            Block::MarkdownRender(markdown_render) => markdown_render.create_state(),
            Block::Editor(editor) => editor.create_state(),
            Block::Dropdown(dropdown) => dropdown.create_state(),
            Block::Pause(pause) => pause.create_state(),
        }
    }

    /// Execute this block
    ///
    /// Performs the block's operation and returns an execution handle for
    /// tracking lifecycle and cancellation.
    ///
    /// # Arguments
    /// * `context` - Execution context with document state and resources
    pub async fn execute(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Block::Terminal(terminal) => terminal.execute(context).await,
            Block::Script(script) => script.execute(context).await,
            Block::Postgres(postgres) => postgres.execute(context).await,
            Block::Http(http) => http.execute(context).await,
            Block::Prometheus(prometheus) => prometheus.execute(context).await,
            Block::Clickhouse(clickhouse) => clickhouse.execute(context).await,
            Block::Mysql(mysql) => mysql.execute(context).await,
            Block::Kubernetes(kubernetes) => kubernetes.execute(context).await,
            Block::SQLite(sqlite) => sqlite.execute(context).await,
            Block::LocalVar(local_var) => local_var.execute(context).await,
            Block::Var(var) => var.execute(context).await,
            Block::Environment(environment) => environment.execute(context).await,
            Block::Directory(directory) => directory.execute(context).await,
            Block::LocalDirectory(local_directory) => local_directory.execute(context).await,
            Block::SshConnect(ssh_connect) => ssh_connect.execute(context).await,
            Block::Host(host) => host.execute(context).await,
            Block::VarDisplay(var_display) => var_display.execute(context).await,
            Block::MarkdownRender(markdown_render) => markdown_render.execute(context).await,
            Block::Editor(editor) => editor.execute(context).await,
            Block::Dropdown(dropdown) => dropdown.execute(context).await,
            Block::Pause(pause) => pause.execute(context).await,
        }
    }
}

impl TryInto<Block> for &serde_json::Value {
    type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

    fn try_into(self) -> Result<Block, Self::Error> {
        Block::from_document(self).map_err(|e| e.into())
    }
}
