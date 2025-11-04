pub(crate) mod decode;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlparser::ast::Statement;
use sqlparser::dialect::{Dialect, MySqlDialect};
use sqlx::{mysql::MySqlConnectOptions, Column, MySqlPool, Row};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::runtime::blocks::handler::ExecutionStatus;
use crate::runtime::blocks::sqlx_block::{
    SqlxBlockBehavior, SqlxBlockError, SqlxBlockExecutionResult, SqlxQueryResult,
    SqlxStatementResult,
};
use crate::runtime::blocks::{Block, BlockBehavior};

use super::handler::{CancellationToken, ExecutionContext, ExecutionHandle};
use super::FromDocument;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Mysql {
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

impl FromDocument for Mysql {
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

        let mysql = Mysql::builder()
            .id(id)
            .name(
                props
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("MySQL Query")
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

        Ok(mysql)
    }
}

impl Mysql {
    /// Validate MySQL URI format and connection parameters
    fn validate_mysql_uri(uri: &str) -> Result<(), String> {
        if uri.is_empty() {
            return Err("MySQL URI cannot be empty".to_string());
        }

        if !uri.starts_with("mysql://") && !uri.starts_with("mariadb://") {
            return Err(
                "Invalid MySQL URI format. Must start with 'mysql://' or 'mariadb://'".to_string(),
            );
        }

        // Try parsing the URI to catch format errors early
        if let Err(e) = MySqlConnectOptions::from_str(uri) {
            return Err(format!("Invalid URI format: {}", e));
        }

        Ok(())
    }

    /// Convert MySQL row to JSON value using existing decode module
    fn row_to_json(row: &sqlx::mysql::MySqlRow) -> Result<Map<String, Value>, sqlx::Error> {
        let mut obj = Map::new();

        for (i, column) in row.columns().iter().enumerate() {
            let column_name = column.name().to_string();
            let raw_value = row.try_get_raw(i)?;

            // Use existing MySQL decode function
            let value = decode::to_json(raw_value).unwrap_or(Value::Null);

            obj.insert(column_name, value);
        }

        Ok(obj)
    }
}

#[async_trait::async_trait]
impl BlockBehavior for Mysql {
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::Mysql(self)
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

#[async_trait::async_trait]
impl SqlxBlockBehavior for Mysql {
    type Pool = MySqlPool;

    fn dialect() -> Box<dyn Dialect> {
        Box::new(MySqlDialect {})
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

        if let Err(e) = Self::validate_mysql_uri(&uri) {
            return Err(SqlxBlockError::InvalidUri(e.to_string()));
        }

        Ok(uri)
    }

    async fn connect(uri: String) -> Result<Self::Pool, SqlxBlockError> {
        let opts = MySqlConnectOptions::from_str(&uri)?;
        Ok(MySqlPool::connect_with(opts).await?)
    }

    async fn disconnect(pool: &Self::Pool) -> Result<(), SqlxBlockError> {
        pool.close().await;
        Ok(())
    }

    fn is_query(statement: &Statement) -> bool {
        match statement {
            Statement::Query { .. } => true,
            Statement::Explain { .. } => true,
            Statement::ExplainTable { .. } => true,
            Statement::Fetch { .. } => true,
            Statement::Pragma { .. } => true,
            Statement::ShowVariables { .. } => true,
            Statement::ShowCreate { .. } => true,
            Statement::ShowColumns { .. } => true,
            Statement::ShowTables { .. } => true,
            Statement::ShowCollation { .. } => true,
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
