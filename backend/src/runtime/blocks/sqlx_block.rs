use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Serialize, Serializer};
use serde_json::{json, Map, Value};
use sqlparser::{ast::Statement, dialect::Dialect};
use ts_rs::TS;
use typed_builder::TypedBuilder;

use crate::runtime::blocks::{
    handler::{
        BlockErrorData, BlockLifecycleEvent, BlockOutput, ExecutionContext, ExecutionHandle,
    },
    BlockBehavior,
};

#[derive(Debug, thiserror::Error)]
pub enum SqlxBlockError {
    #[error("Database driver error: {0}")]
    SqlxError(#[from] sqlx::Error),

    #[error("Generic error: {0}")]
    GenericError(String),

    #[error("Invalid template: {0}")]
    InvalidTemplate(String),

    #[error("Invalid SQL: {0}")]
    InvalidSql(String),

    #[error("Invalid URI: {0}")]
    InvalidUri(String),

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Cancelled")]
    Cancelled,
}

impl From<&str> for SqlxBlockError {
    fn from(value: &str) -> Self {
        SqlxBlockError::GenericError(value.to_string())
    }
}

#[derive(Debug, Clone, Serialize, TypedBuilder, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub(crate) struct SqlxQueryResult {
    columns: Vec<String>,
    rows: Vec<Map<String, Value>>,
    #[builder(default = None)]
    #[ts(type = "number | null")]
    rows_read: Option<u64>,
    #[builder(default = None)]
    #[ts(type = "number | null")]
    bytes_read: Option<u64>,
    #[serde(serialize_with = "serialize_duration")]
    #[ts(type = "number")]
    duration: Duration,
    #[builder(default = Utc::now())]
    #[ts(type = "string")]
    time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, TypedBuilder, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub(crate) struct SqlxStatementResult {
    #[ts(type = "number")]
    rows_affected: u64,
    #[builder(default = None)]
    #[ts(type = "number | null")]
    last_insert_rowid: Option<u64>,
    #[serde(serialize_with = "serialize_duration")]
    #[ts(type = "number")]
    duration: Duration,
    #[builder(default = Utc::now())]
    #[ts(type = "string")]
    time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "type", content = "data")]
#[ts(export)]
pub(crate) enum SqlxBlockExecutionResult {
    Query(SqlxQueryResult),
    Statement(SqlxStatementResult),
}

fn serialize_duration<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_f64(duration.as_secs_f64())
}

#[async_trait]
/// A trait that defines the behavior of a block that executes SQL queries and statements via sqlx
pub(crate) trait SqlxBlockBehavior: BlockBehavior + 'static {
    /// The type of the SQLx connection pool
    type Pool: Clone + Send + Sync + 'static;

    /// Execute the block. Creates an execution handle and manages all lifecycle events.
    async fn execute_query_block(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        let context_clone = context.clone();
        match self.do_execute(context_clone).await {
            Ok(_) => Ok(Some(context.handle())),
            Err(e) => match e {
                SqlxBlockError::Cancelled => {
                    let _ = context.block_cancelled().await;
                    Ok(None)
                }
                _ => {
                    let _ = context.block_failed(e.to_string()).await;
                    Err(Box::new(e))
                }
            },
        }
    }

    async fn do_execute(&self, context: ExecutionContext) -> Result<(), SqlxBlockError> {
        let cancellation_receiver = context.handle().cancellation_token.take_receiver();

        let context_clone = context.clone();
        let block_id = self.id();
        let uri = self.resolve_uri(&context)?;
        let query = self.resolve_query(&context)?;

        // Parse queries synchronously in a scope to ensure dialect is dropped, since it is not Send
        let queries: Vec<(String, bool)> = {
            let dialect = Self::dialect();
            let statements = sqlparser::parser::Parser::parse_sql(dialect.as_ref(), &query)
                .map_err(|e| SqlxBlockError::InvalidSql(e.to_string()))?;

            if statements.is_empty() {
                return Err(SqlxBlockError::InvalidSql("Query is empty".to_string()));
            }

            statements
                .iter()
                .map(|s| (s.to_string(), Self::is_query(s)))
                .collect()
        };

        let connection = Self::connect(uri.clone());
        let query_count = queries.len();

        tokio::spawn(async move {
            let _ = context_clone.block_started().await;

            let _ = context_clone
                .send_output(
                    BlockOutput::builder()
                        .block_id(block_id)
                        .object(json!({ "type": "queryCount", "count": query_count }))
                        .build(),
                )
                .await;

            let _ = context_clone
                .send_output(
                    BlockOutput::builder()
                        .block_id(block_id)
                        .stdout("Connecting to database...".to_string())
                        .build(),
                )
                .await;

            let pool = {
                let timeout = tokio::time::sleep(Duration::from_secs(10));

                tokio::select! {
                    result = connection => {
                        match result {
                            Ok(pool) => {
                                let _ = context_clone.send_output(
                                    BlockOutput::builder()
                                        .block_id(block_id)
                                        .stdout("Connected to database successfully".to_string())
                                        .build(),
                                ).await;
                                pool
                            },
                            Err(e) => {
                                return Err(e);
                            }
                        }
                    }
                    _ = timeout => {
                        let message = "Database connection timed out after 10 seconds.".to_string();
                        return Err(SqlxBlockError::ConnectionError(message))
                    }
                }
            };

            let context_clone = context_clone.clone();
            let pool_clone = pool.clone();
            let execution_task = async move {
                if query.is_empty() {
                    return Err(SqlxBlockError::InvalidSql("Query is empty".to_string()));
                }

                let _ = context_clone
                    .send_output(
                        BlockOutput::builder()
                            .block_id(block_id)
                            .stdout("Executing query...".to_string())
                            .build(),
                    )
                    .await;

                for (i, (query, is_query)) in queries.iter().enumerate() {
                    let result = if *is_query {
                        Self::execute_query(&pool_clone, &query).await
                    } else {
                        Self::execute_statement(&pool_clone, &query).await
                    };

                    if let Err(e) = result {
                        let message = format!("Statement {} failed: {}", i + 1, e);
                        let _ = context_clone
                            .send_output(
                                BlockOutput::builder()
                                    .block_id(block_id)
                                    .stderr(message.clone())
                                    .lifecycle(BlockLifecycleEvent::Error(BlockErrorData {
                                        message: message.clone(),
                                    }))
                                    .build(),
                            )
                            .await;
                        return Err(e);
                    }

                    let result = result.unwrap();
                    let _ = context_clone
                        .send_output(
                            BlockOutput::builder()
                                .block_id(block_id)
                                .object(serde_json::to_value(result).map_err(|_| {
                                    SqlxBlockError::GenericError(
                                        "Unable to serialize query result".to_string(),
                                    )
                                })?)
                                .build(),
                        )
                        .await;
                }

                Ok(())
            };

            let context_clone = context.clone();

            let result = if let Some(cancel_rx) = cancellation_receiver {
                tokio::select! {
                    _ = cancel_rx => {
                        let _ = Self::disconnect(&pool).await;
                        let _ = context_clone.block_cancelled().await;
                        return Err(SqlxBlockError::Cancelled);
                    }
                    result = execution_task => {
                        let _ = Self::disconnect(&pool).await;
                        result
                    }
                }
            } else {
                let result = execution_task.await;
                let _ = Self::disconnect(&pool).await;
                result
            };

            if let Err(e) = result {
                return Err(e);
            }

            let _ = context_clone.block_finished(None, true).await;

            result
        })
        .await
        .map_err(|e| SqlxBlockError::GenericError(e.to_string()))?
    }

    /// The dialect of the SQL database; used to parse queries
    fn dialect() -> Box<dyn Dialect>;

    /// Resolve the query from the context
    fn resolve_query(&self, context: &ExecutionContext) -> Result<String, SqlxBlockError>;

    /// Resolve the URI from the context
    fn resolve_uri(&self, context: &ExecutionContext) -> Result<String, SqlxBlockError>;

    /// Connect to the SQL database
    async fn connect(uri: String) -> Result<Self::Pool, SqlxBlockError>;

    /// Disconnect from the SQL database
    async fn disconnect(pool: &Self::Pool) -> Result<(), SqlxBlockError>;

    /// Check if the statement is a query (vs a statement)
    fn is_query(statement: &Statement) -> bool;

    /// Execute a query
    async fn execute_query(
        pool: &Self::Pool,
        query: &str,
    ) -> Result<SqlxBlockExecutionResult, SqlxBlockError>;

    /// Execute a statement
    async fn execute_statement(
        pool: &Self::Pool,
        statement: &str,
    ) -> Result<SqlxBlockExecutionResult, SqlxBlockError>;
}
