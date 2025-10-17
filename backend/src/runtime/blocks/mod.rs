pub(crate) mod clickhouse;
pub(crate) mod context;
pub(crate) mod document;
pub(crate) mod editor;
pub(crate) mod handler;
pub(crate) mod handlers;
pub(crate) mod http;
pub(crate) mod mysql;
pub(crate) mod postgres;
pub(crate) mod prometheus;
pub(crate) mod registry;
pub(crate) mod script;
pub(crate) mod sqlite;
pub(crate) mod terminal;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::runtime::blocks::{
    document::{
        block_context::BlockContext,
        document_context::{ContextResolver, DocumentExecutionView},
    },
    handler::ExecutionHandle,
};

pub trait FromDocument: Sized {
    fn from_document(block_data: &serde_json::Value) -> Result<Self, String>;
}

#[async_trait]
pub trait BlockBehavior: Send + Sync {
    fn passive_context(
        &self,
        _resolver: &ContextResolver,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn execute(
        &self,
        _execution_context: DocumentExecutionView,
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

    LocalVar(context::local_var::LocalVar),
    Var(context::var::Var),
    Environment(context::environment::Environment),
    Directory(context::directory::Directory),
    SshConnect(context::ssh_connect::SshConnect),
    Host(context::host::Host),
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
            Block::SshConnect(ssh_connect) => ssh_connect.id,
            Block::Host(host) => host.id,
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

            Block::LocalVar(_) => "".to_string(),
            Block::Var(_) => "".to_string(),
            Block::Environment(_) => "".to_string(),
            Block::Directory(_) => "".to_string(),
            Block::SshConnect(_) => "".to_string(),
            Block::Host(_) => "".to_string(),
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
            "local-var" => Ok(Block::LocalVar(
                context::local_var::LocalVar::from_document(block_data)?,
            )),
            "var" => Ok(Block::Var(context::var::Var::from_document(block_data)?)),
            "env" => Ok(Block::Environment(
                context::environment::Environment::from_document(block_data)?,
            )),
            "directory" => Ok(Block::Directory(
                context::directory::Directory::from_document(block_data)?,
            )),
            "ssh-connect" => Ok(Block::SshConnect(
                context::ssh_connect::SshConnect::from_document(block_data)?,
            )),
            "host" => Ok(Block::Host(context::host::Host::from_document(block_data)?)),
            _ => Err(format!("Unsupported block type: {}", block_type)),
        }
    }

    pub fn passive_context(
        &self,
        resolver: &ContextResolver,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Block::LocalVar(local_var) => local_var.passive_context(resolver),
            Block::Var(var) => var.passive_context(resolver),
            Block::Environment(environment) => environment.passive_context(resolver),
            Block::Directory(directory) => directory.passive_context(resolver),
            Block::SshConnect(ssh_connect) => ssh_connect.passive_context(resolver),
            Block::Host(host) => host.passive_context(resolver),

            // Explicitly listing for exhaustiveness
            Block::Terminal(_) => Ok(None),
            Block::Script(_) => Ok(None),
            Block::SQLite(_) => Ok(None),
            Block::Postgres(_) => Ok(None),
            Block::Http(_) => Ok(None),
            Block::Prometheus(_) => Ok(None),
            Block::Clickhouse(_) => Ok(None),
            Block::Mysql(_) => Ok(None),
        }
    }
}
