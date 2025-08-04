pub(crate) mod clickhouse;
pub(crate) mod context_blocks;
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

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub(crate) enum Block {
    Terminal(terminal::Terminal),
    Script(script::Script),
    Postgres(postgres::Postgres),
    Http(http::Http),
    Prometheus(prometheus::Prometheus),
    Clickhouse(clickhouse::Clickhouse),
    Mysql(mysql::Mysql),

    #[serde(rename = "sqlite")]
    SQLite(sqlite::SQLite),
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
        }
    }
}
