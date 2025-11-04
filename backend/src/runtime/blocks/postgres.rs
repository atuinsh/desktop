use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sqlparser::ast::Statement;
use sqlparser::dialect::{Dialect, PostgreSqlDialect};
use sqlx::{postgres::PgConnectOptions, Column, PgPool, Row, TypeInfo};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::runtime::blocks::document::block_context::BlockExecutionOutput;
use crate::runtime::blocks::handler::{
    BlockErrorData, BlockFinishedData, BlockLifecycleEvent, BlockOutput, ExecutionStatus,
};
use crate::runtime::blocks::sqlx_block::{
    SqlxBlockBehavior, SqlxBlockError, SqlxBlockExecutionResult, SqlxQueryResult,
    SqlxStatementResult,
};
use crate::runtime::blocks::{Block, BlockBehavior};
use crate::runtime::events::GCEvent;
use crate::runtime::workflow::event::WorkflowEvent;

use super::handler::{CancellationToken, ExecutionContext, ExecutionHandle};
use super::FromDocument;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Postgres {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub name: String,

    #[builder(setter(into))]
    pub query: String,

    #[builder(setter(into))]
    pub uri: String,

    #[builder(default = 0)]
    pub auto_refresh: u32,
}

impl FromDocument for Postgres {
    fn from_document(block_data: &serde_json::Value) -> Result<Self, String> {
        let block_id = block_data
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or("Block has no id")?;

        let props = block_data
            .get("props")
            .and_then(|p| p.as_object())
            .ok_or("Block has no props")?;

        let id = Uuid::parse_str(block_id).map_err(|e| e.to_string())?;

        let postgres = Postgres::builder()
            .id(id)
            .name(
                props
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Postgres Query")
                    .to_string(),
            )
            .query(
                props
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            )
            .uri(
                props
                    .get("uri")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            )
            .auto_refresh(
                props
                    .get("autoRefresh")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
            )
            .build();

        Ok(postgres)
    }
}

impl Postgres {
    /// Validate Postgres URI format and connection parameters
    fn validate_postgres_uri(uri: &str) -> Result<(), String> {
        if uri.is_empty() {
            return Err("Postgres URI cannot be empty".to_string());
        }

        if !uri.starts_with("postgres://") && !uri.starts_with("postgresql://") {
            return Err(
                "Invalid Postgres URI format. Must start with 'postgres://' or 'postgresql://'"
                    .to_string(),
            );
        }

        // Try parsing the URI to catch format errors early
        if let Err(e) = PgConnectOptions::from_str(uri) {
            return Err(format!("Invalid URI format: {}", e));
        }

        Ok(())
    }

    /// Template Postgres query using the context resolver
    fn template_postgres_query(
        &self,
        context: &ExecutionContext,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let rendered = context.context_resolver.resolve_template(&self.query)?;
        Ok(rendered)
    }

    /// Convert Postgres row to JSON value
    fn row_to_json(row: &sqlx::postgres::PgRow) -> Result<Map<String, Value>, sqlx::Error> {
        let mut obj = Map::new();

        for (i, column) in row.columns().iter().enumerate() {
            let column_name = column.name().to_string();
            let value: Value = match column.type_info().name() {
                "BOOL" => {
                    if let Ok(val) = row.try_get::<bool, _>(i) {
                        json!(val)
                    } else {
                        Value::Null
                    }
                }
                "INT2" | "SMALLINT" => {
                    if let Ok(val) = row.try_get::<i16, _>(i) {
                        json!(val)
                    } else {
                        Value::Null
                    }
                }
                "INT4" | "INTEGER" => {
                    if let Ok(val) = row.try_get::<i32, _>(i) {
                        json!(val)
                    } else {
                        Value::Null
                    }
                }
                "INT8" | "BIGINT" => {
                    if let Ok(val) = row.try_get::<i64, _>(i) {
                        json!(val)
                    } else {
                        Value::Null
                    }
                }
                "FLOAT4" | "REAL" => {
                    if let Ok(val) = row.try_get::<f32, _>(i) {
                        json!(val)
                    } else {
                        Value::Null
                    }
                }
                "FLOAT8" | "DOUBLE PRECISION" => {
                    if let Ok(val) = row.try_get::<f64, _>(i) {
                        json!(val)
                    } else {
                        Value::Null
                    }
                }
                "TEXT" | "VARCHAR" | "CHAR" | "NAME" => {
                    if let Ok(val) = row.try_get::<String, _>(i) {
                        json!(val)
                    } else {
                        Value::Null
                    }
                }
                "UUID" => {
                    if let Ok(val) = row.try_get::<Uuid, _>(i) {
                        json!(val.to_string())
                    } else {
                        Value::Null
                    }
                }
                "BYTEA" => {
                    if let Ok(val) = row.try_get::<Vec<u8>, _>(i) {
                        json!(base64::encode(val))
                    } else {
                        Value::Null
                    }
                }
                "TIMESTAMP" | "TIMESTAMPTZ" | "DATE" | "TIME" => {
                    // For date/time types, just convert to string
                    if let Ok(val) = row.try_get::<String, _>(i) {
                        json!(val)
                    } else {
                        Value::Null
                    }
                }
                "JSON" | "JSONB" => {
                    if let Ok(val) = row.try_get::<Value, _>(i) {
                        val
                    } else {
                        Value::Null
                    }
                }
                _ => {
                    // Try to get as string for unknown types
                    if let Ok(val) = row.try_get::<String, _>(i) {
                        json!(val)
                    } else {
                        Value::Null
                    }
                }
            };
            obj.insert(column_name, value);
        }

        Ok(obj)
    }
}

#[async_trait::async_trait]
impl SqlxBlockBehavior for Postgres {
    type Pool = PgPool;

    fn dialect() -> Box<dyn Dialect> {
        Box::new(PostgreSqlDialect {})
    }

    fn resolve_query(&self, context: &ExecutionContext) -> Result<String, SqlxBlockError> {
        context
            .context_resolver
            .resolve_template(&self.query)
            .map_err(|e| SqlxBlockError::InvalidTemplate(e.to_string()))
    }

    fn resolve_uri(&self, context: &ExecutionContext) -> Result<String, SqlxBlockError> {
        let uri = context
            .context_resolver
            .resolve_template(&self.uri)
            .map_err(|e| SqlxBlockError::InvalidTemplate(e.to_string()))?;

        if let Err(e) = Self::validate_postgres_uri(&uri) {
            return Err(SqlxBlockError::InvalidUri(e.to_string()));
        }

        Ok(uri)
    }

    async fn connect(uri: String) -> Result<Self::Pool, SqlxBlockError> {
        let opts = PgConnectOptions::from_str(&uri)?;
        Ok(PgPool::connect_with(opts).await?)
    }

    async fn disconnect(pool: &Self::Pool) -> Result<(), SqlxBlockError> {
        pool.close().await;
        Ok(())
    }

    fn is_query(statement: &Statement) -> bool {
        match statement {
            Statement::Query { .. } => true,
            Statement::Explain { .. } => true,
            Statement::Fetch { .. } => true,
            Statement::Pragma { .. } => true,
            Statement::ShowVariable { .. } => true,
            _ => false,
        }
    }

    async fn execute_query(
        pool: &Self::Pool,
        query: &str,
    ) -> Result<SqlxBlockExecutionResult, SqlxBlockError> {
        let start_time = Instant::now();
        let rows = sqlx::query(query).fetch_all(pool).await?;
        let duration = start_time.elapsed();
        let mut columns = Vec::new();

        if let Some(first_row) = rows.first() {
            columns = first_row
                .columns()
                .iter()
                .map(|col| col.name().to_string())
                .collect();
        }

        let results = rows
            .iter()
            .map(Self::row_to_json)
            .collect::<Result<_, _>>()?;

        Ok(SqlxBlockExecutionResult::Query(
            SqlxQueryResult::builder()
                .columns(columns)
                .rows(results)
                .duration(duration)
                .build(),
        ))
    }

    async fn execute_statement(
        pool: &Self::Pool,
        statement: &str,
    ) -> Result<SqlxBlockExecutionResult, SqlxBlockError> {
        let start_time = Instant::now();
        let result = sqlx::query(statement).execute(pool).await?;
        let duration = start_time.elapsed();

        Ok(SqlxBlockExecutionResult::Statement(
            SqlxStatementResult::builder()
                .rows_affected(result.rows_affected())
                .duration(duration)
                .build(),
        ))
    }
}

#[async_trait::async_trait]
impl BlockBehavior for Postgres {
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::Postgres(self)
    }

    async fn execute(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        let handle = ExecutionHandle {
            id: Uuid::new_v4(),
            block_id: self.id,
            cancellation_token: CancellationToken::new(),
            status: Arc::new(RwLock::new(ExecutionStatus::Running)),
            output_variable: None,
        };

        if let Err(e) = SqlxBlockBehavior::execute(&self, context, handle.clone()).await {
            *handle.status.write().await = ExecutionStatus::Failed(e.to_string());
        }

        Ok(Some(handle))
    }
}
