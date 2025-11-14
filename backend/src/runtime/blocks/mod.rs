pub(crate) mod clickhouse;
pub(crate) mod directory;
pub(crate) mod document;
pub(crate) mod editor;
pub(crate) mod environment;
pub(crate) mod handler;
pub(crate) mod host;
pub(crate) mod http;
pub(crate) mod local_directory;
pub(crate) mod local_var;
pub(crate) mod mysql;
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

use crate::runtime::blocks::{
    document::{
        actor::LocalValueProvider,
        block_context::{BlockContext, ContextResolver},
    },
    handler::{ExecutionContext, ExecutionHandle},
};

pub trait FromDocument: Sized {
    fn from_document(block_data: &serde_json::Value) -> Result<Self, String>;
}

#[async_trait]
pub trait BlockBehavior: Sized + Send + Sync {
    fn into_block(self) -> Block;

    fn id(&self) -> Uuid;

    async fn passive_context(
        &self,
        _resolver: &ContextResolver,
        _block_local_value_provider: Option<&dyn LocalValueProvider>,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    async fn execute(
        self,
        _context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
}

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
    Editor(editor::Editor),
}

impl Block {
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

            Block::LocalVar(local_var) => local_var.id,
            Block::Var(var) => var.id,
            Block::Environment(environment) => environment.id,
            Block::Directory(directory) => directory.id,
            Block::LocalDirectory(local_directory) => local_directory.id,
            Block::SshConnect(ssh_connect) => ssh_connect.id,
            Block::Host(host) => host.id,
            Block::VarDisplay(var_display) => var_display.id,
            Block::Editor(editor) => editor.id,
        }
    }

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
            Block::Editor(_) => "".to_string(),

            Block::LocalVar(_) => "".to_string(),
            Block::Var(_) => "".to_string(),
            Block::Environment(_) => "".to_string(),
            Block::Directory(_) => "".to_string(),
            Block::LocalDirectory(_) => "".to_string(),
            Block::SshConnect(_) => "".to_string(),
            Block::Host(_) => "".to_string(),
            Block::VarDisplay(_) => "".to_string(),
        }
    }

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
            "editor" => Ok(Block::Editor(editor::Editor::from_document(block_data)?)),
            _ => Err(format!("Unknown block type: {}", block_type)),
        }
    }

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
            Block::Editor(editor) => {
                editor
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
        }
    }

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
            Block::SQLite(sqlite) => sqlite.execute(context).await,
            Block::LocalVar(local_var) => local_var.execute(context).await,
            Block::Var(var) => var.execute(context).await,
            Block::Environment(environment) => environment.execute(context).await,
            Block::Directory(directory) => directory.execute(context).await,
            Block::LocalDirectory(local_directory) => local_directory.execute(context).await,
            Block::SshConnect(ssh_connect) => ssh_connect.execute(context).await,
            Block::Host(host) => host.execute(context).await,
            Block::VarDisplay(var_display) => var_display.execute(context).await,
            Block::Editor(editor) => editor.execute(context).await,
        }
    }
}

impl TryInto<Block> for &serde_json::Value {
    type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

    fn try_into(self) -> Result<Block, Self::Error> {
        Block::from_document(self).map_err(|e| e.into())
    }
}
