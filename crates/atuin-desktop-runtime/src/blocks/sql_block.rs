use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Serialize, Serializer};
use serde_json::{json, Map, Value};
use sqlparser::{ast::Statement, dialect::Dialect};
use ts_rs::TS;
use typed_builder::TypedBuilder;

use crate::{
    blocks::{BlockBehavior, BlockExecutionError, QueryBlockBehavior, QueryBlockError},
    context::BlockExecutionOutput,
    execution::{ExecutionContext, StreamingBlockOutput},
};

#[derive(Debug, thiserror::Error)]
pub enum SqlBlockError {
    #[error("Database driver error: {0}")]
    SqlxError(#[from] sqlx::Error),

    #[error("Operation timed out")]
    Timeout,

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Query block behavior error: {0}")]
    QueryBlockBehaviorError(#[from] QueryBlockError),

    #[error("Query error: {0}")]
    QueryError(String),

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

impl From<&str> for SqlBlockError {
    fn from(value: &str) -> Self {
        SqlBlockError::GenericError(value.to_string())
    }
}

impl From<String> for SqlBlockError {
    fn from(value: String) -> Self {
        SqlBlockError::GenericError(value)
    }
}

impl BlockExecutionError for SqlBlockError {
    fn cancelled() -> Self {
        SqlBlockError::Cancelled
    }

    fn timeout(_message: String) -> Self {
        SqlBlockError::Timeout
    }

    fn serialization_error(message: String) -> Self {
        SqlBlockError::SerializationError(message)
    }

    fn is_cancelled(&self) -> bool {
        matches!(self, SqlBlockError::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize, TypedBuilder, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SqlQueryResult {
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
pub struct SqlStatementResult {
    #[ts(type = "number | null")]
    #[builder(default = None, setter(strip_option))]
    rows_affected: Option<u64>,
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
pub enum SqlBlockExecutionResult {
    Query(SqlQueryResult),
    Statement(SqlStatementResult),
}

/// Output structure for SQL blocks that implements BlockExecutionOutput
/// for template access to query results.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SqlBlockOutput {
    /// All results from the SQL execution (supports multiple statements)
    pub results: Vec<SqlBlockExecutionResult>,
    /// Total execution duration in seconds
    #[serde(serialize_with = "serialize_duration")]
    #[ts(type = "number")]
    pub total_duration: Duration,
}

impl SqlBlockOutput {
    /// Create a new SqlBlockOutput from a list of results
    pub fn new(results: Vec<SqlBlockExecutionResult>) -> Self {
        let total_duration = results
            .iter()
            .map(|r| match r {
                SqlBlockExecutionResult::Query(q) => q.duration,
                SqlBlockExecutionResult::Statement(s) => s.duration,
            })
            .sum();

        Self {
            results,
            total_duration,
        }
    }

    /// Get the total number of rows returned across all query results
    pub fn total_rows(&self) -> usize {
        self.results
            .iter()
            .filter_map(|r| match r {
                SqlBlockExecutionResult::Query(q) => Some(q.rows.len()),
                SqlBlockExecutionResult::Statement(_) => None,
            })
            .sum()
    }

    /// Get the total number of rows affected across all statement results
    pub fn total_rows_affected(&self) -> u64 {
        self.results
            .iter()
            .filter_map(|r| match r {
                SqlBlockExecutionResult::Statement(s) => s.rows_affected,
                SqlBlockExecutionResult::Query(_) => None,
            })
            .sum()
    }

    /// Get the first result (convenience for single-statement queries)
    pub fn first_result(&self) -> Option<&SqlBlockExecutionResult> {
        self.results.first()
    }

    /// Get the first query result (convenience for single SELECT queries)
    pub fn first_query(&self) -> Option<&SqlQueryResult> {
        self.results.iter().find_map(|r| match r {
            SqlBlockExecutionResult::Query(q) => Some(q),
            _ => None,
        })
    }

    /// Get the first statement result
    pub fn first_statement(&self) -> Option<&SqlStatementResult> {
        self.results.iter().find_map(|r| match r {
            SqlBlockExecutionResult::Statement(s) => Some(s),
            _ => None,
        })
    }

    /// Get all rows from all query results as a flat list
    pub fn all_rows(&self) -> Vec<&Map<String, Value>> {
        self.results
            .iter()
            .filter_map(|r| match r {
                SqlBlockExecutionResult::Query(q) => Some(q.rows.iter()),
                _ => None,
            })
            .flatten()
            .collect()
    }
}

impl BlockExecutionOutput for SqlBlockOutput {
    fn get_template_value(&self, key: &str) -> Option<minijinja::Value> {
        match key {
            // Primary access methods
            "results" => Some(minijinja::Value::from_serialize(&self.results)),
            "first" => self.first_result().map(minijinja::Value::from_serialize),

            // Aggregated data
            "total_rows" => Some(minijinja::Value::from(self.total_rows())),
            "total_rows_affected" => Some(minijinja::Value::from(self.total_rows_affected())),
            "total_duration" => Some(minijinja::Value::from(self.total_duration.as_secs_f64())),
            "result_count" => Some(minijinja::Value::from(self.results.len())),

            // Convenience accessors for the first query result
            "rows" => self
                .first_query()
                .map(|q| minijinja::Value::from_serialize(&q.rows)),
            "columns" => self
                .first_query()
                .map(|q| minijinja::Value::from_serialize(&q.columns)),

            // Convenience accessor for statement results
            "rows_affected" => self
                .first_statement()
                .and_then(|s| s.rows_affected)
                .map(minijinja::Value::from),

            _ => None,
        }
    }

    fn enumerate_template_keys(&self) -> minijinja::value::Enumerator {
        minijinja::value::Enumerator::Str(&[
            "results",
            "first",
            "total_rows",
            "total_rows_affected",
            "total_duration",
            "result_count",
            "rows",
            "columns",
            "rows_affected",
        ])
    }
}

fn serialize_duration<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_f64(duration.as_secs_f64())
}

#[async_trait]
/// A trait that defines the behavior of a block that executes SQL queries and statements via sqlx.
/// Provides SQL-specific functionality like parsing, query vs statement distinction, and handling
/// multiple semicolon-separated queries. Automatically implements QueryBlockBehavior.
pub trait SqlBlockBehavior: BlockBehavior + 'static {
    /// The type of the SQLx connection pool
    type Pool: Clone + Send + Sync + 'static;

    /// The dialect of the SQL database; used to parse queries
    fn dialect() -> Box<dyn Dialect>;

    /// Resolve the query from the context (required by QueryBlockBehavior)
    fn resolve_query(&self, context: &ExecutionContext) -> Result<String, SqlBlockError>;

    /// Resolve the URI from the context
    fn resolve_uri(&self, context: &ExecutionContext) -> Result<String, SqlBlockError>;

    /// Connect to the SQL database (static method for actual connection logic)
    async fn create_pool(&self, uri: String) -> Result<Self::Pool, SqlBlockError>;

    /// Close the SQL database connection (static method for actual disconnection logic)
    async fn close_pool(&self, pool: &Self::Pool) -> Result<(), SqlBlockError>;

    /// Check if the statement is a query (vs a statement)
    fn is_query(statement: &Statement) -> bool;

    /// Execute a SQL query (SELECT, etc.)
    async fn execute_sql_query(
        &self,
        pool: &Self::Pool,
        query: &str,
    ) -> Result<SqlBlockExecutionResult, SqlBlockError>;

    /// Execute a SQL statement (INSERT, UPDATE, DELETE, etc.)
    async fn execute_sql_statement(
        &self,
        pool: &Self::Pool,
        statement: &str,
    ) -> Result<SqlBlockExecutionResult, SqlBlockError>;
}

// Provide blanket implementations for QueryBlockBehavior methods
#[async_trait]
impl<T> QueryBlockBehavior for T
where
    T: SqlBlockBehavior,
{
    type Connection = T::Pool;
    type QueryResult = SqlBlockExecutionResult;
    type Error = SqlBlockError;

    fn resolve_query(&self, context: &ExecutionContext) -> Result<String, Self::Error> {
        // Delegate to the SQL-specific resolve_query implementation
        <Self as SqlBlockBehavior>::resolve_query(self, context)
    }

    fn resolve_connection_string(
        &self,
        context: &ExecutionContext,
    ) -> Result<String, SqlBlockError> {
        <Self as SqlBlockBehavior>::resolve_uri(self, context)
    }

    async fn connect(&self, uri: String) -> Result<Self::Connection, SqlBlockError> {
        <Self as SqlBlockBehavior>::create_pool(self, uri).await
    }

    async fn disconnect(&self, connection: &Self::Connection) -> Result<(), SqlBlockError> {
        <Self as SqlBlockBehavior>::close_pool(self, connection).await
    }

    async fn execute_query(
        &self,
        connection: &Self::Connection,
        query: &str,
        context: &ExecutionContext,
    ) -> Result<Vec<SqlBlockExecutionResult>, SqlBlockError> {
        let block_id = context.handle().block_id;

        // Parse queries synchronously in a scope to ensure dialect is dropped, since it is not Send
        let queries: Vec<(String, bool)> = {
            let dialect = <Self as SqlBlockBehavior>::dialect();
            let statements =
                sqlparser::parser::Parser::parse_sql_with_offsets(dialect.as_ref(), query)
                    .map_err(|e| SqlBlockError::InvalidSql(e.to_string()))?;

            if statements.is_empty() {
                return Err(SqlBlockError::InvalidSql("Query is empty".to_string()));
            }

            for statement in statements.iter() {
                tracing::info!("Statement: {:?}", statement);
            }

            statements
                .iter()
                .map(|(s, offset)| {
                    (
                        query[offset.start()..offset.end()].to_string(),
                        <Self as SqlBlockBehavior>::is_query(s),
                    )
                })
                .collect()
        };

        let query_count = queries.len();

        // Send query count metadata
        let _ = context
            .send_output(
                StreamingBlockOutput::builder()
                    .block_id(block_id)
                    .object(json!({ "type": "queryCount", "count": query_count }))
                    .build(),
            )
            .await;

        // Execute each query/statement and collect results
        let mut results = Vec::new();
        for (sql_text, is_query) in queries.iter() {
            let result = if *is_query {
                <Self as SqlBlockBehavior>::execute_sql_query(self, connection, sql_text).await?
            } else {
                <Self as SqlBlockBehavior>::execute_sql_statement(self, connection, sql_text)
                    .await?
            };
            results.push(result);
        }

        Ok(results)
    }

    fn create_output(
        &self,
        results: &[Self::QueryResult],
    ) -> Option<Box<dyn BlockExecutionOutput>> {
        Some(Box::new(SqlBlockOutput::new(results.to_vec())))
    }
}
