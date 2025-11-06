use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlparser::ast::Statement;
use sqlparser::dialect::{ClickHouseDialect, Dialect};
use std::time::{Duration, Instant};
use typed_builder::TypedBuilder;
use url::Url;
use uuid::Uuid;

use crate::runtime::blocks::document::block_context::BlockExecutionOutput;
use crate::runtime::blocks::handler::{BlockOutput, ExecutionStatus};
use crate::runtime::blocks::sqlx_block::{
    SqlxBlockBehavior, SqlxBlockError, SqlxBlockExecutionResult, SqlxQueryResult,
    SqlxStatementResult,
};
use crate::runtime::blocks::{Block, BlockBehavior};
use crate::runtime::events::GCEvent;

use super::handler::{CancellationToken, ExecutionContext, ExecutionHandle};
use super::FromDocument;

type ClientWithUri = (reqwest::Client, String);

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Clickhouse {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub name: String,

    #[builder(setter(into))]
    pub query: String,

    #[builder(setter(into))]
    pub uri: String,

    #[builder(default = 0)]
    pub auto_refresh: i32,
}

impl FromDocument for Clickhouse {
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

        let clickhouse = Clickhouse::builder()
            .id(id)
            .name(
                props
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("ClickHouse Query")
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
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as i32,
            )
            .build();

        Ok(clickhouse)
    }
}

impl Clickhouse {}

#[async_trait::async_trait]
impl BlockBehavior for Clickhouse {
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::Clickhouse(self)
    }

    async fn execute(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        SqlxBlockBehavior::execute_query_block(self, context).await
    }
}

#[async_trait::async_trait]
impl SqlxBlockBehavior for Clickhouse {
    type Pool = ClientWithUri;

    fn dialect() -> Box<dyn Dialect> {
        Box::new(ClickHouseDialect {})
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

        Ok(uri)
    }

    async fn connect(uri: String) -> Result<Self::Pool, SqlxBlockError> {
        if uri.is_empty() {
            return Err(SqlxBlockError::InvalidUri("URI is empty".to_string()));
        }

        if !uri.starts_with("http://") && !uri.starts_with("https://") {
            return Err(SqlxBlockError::InvalidUri(
                "URI must start with 'http://' or 'https://'".to_string(),
            ));
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| SqlxBlockError::ConnectionError(e.to_string()))?;

        // Test connection with simple query
        let test_request = client.post(&uri).body("SELECT 1 FORMAT JSONEachRow");
        let response = test_request
            .send()
            .await
            .map_err(|e| SqlxBlockError::ConnectionError(e.to_string()))?;

        if !response.status().is_success() {
            let error = response
                .text()
                .await
                .map_err(|e| SqlxBlockError::ConnectionError(e.to_string()))?;
            return Err(SqlxBlockError::ConnectionError(format!(
                "Connection test failed: {}",
                error
            )));
        }

        Ok((client, uri))
    }

    async fn disconnect(pool: &Self::Pool) -> Result<(), SqlxBlockError> {
        Ok(())
    }

    fn is_query(statement: &Statement) -> bool {
        match statement {
            Statement::Query { .. } => true,
            _ => false,
        }
    }

    async fn execute_query(
        pool: &Self::Pool,
        query: &str,
    ) -> Result<SqlxBlockExecutionResult, SqlxBlockError> {
        let (client, uri) = pool;

        let query_to_execute = if !query.to_uppercase().contains("FORMAT") {
            format!("{} FORMAT JSONEachRow", query)
        } else {
            query.to_string()
        };

        log::info!("Executing query: {}", query_to_execute);
        let request = client.post(uri).body(query_to_execute);

        let response = request
            .send()
            .await
            .map_err(|e| SqlxBlockError::ConnectionError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .map_err(|e| SqlxBlockError::GenericError(e.to_string()))?;
            return Err(SqlxBlockError::QueryError(error_text));
        }

        let start_time = Instant::now();
        let response_text = response
            .text()
            .await
            .map_err(|e| SqlxBlockError::GenericError(e.to_string()))?;
        let duration = start_time.elapsed();

        let lines = response_text.lines();
        let mut results = Vec::with_capacity(lines.size_hint().0);
        let mut column_names = Vec::new();

        for line in response_text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Parse each line as a JSON object
            match serde_json::from_str::<Value>(line) {
                Ok(row) => {
                    log::info!("Row: {:?}", row);
                    // Extract column names from first row
                    if let Value::Object(map) = row {
                        if column_names.is_empty() {
                            column_names = map.keys().cloned().collect();
                            column_names.sort(); // Ensure consistent ordering
                        }

                        results.push(map);
                    }
                }
                Err(e) => {
                    return Err(SqlxBlockError::GenericError(format!(
                        "Failed to parse JSON response: {} (line: {})",
                        e, line
                    )));
                }
            }
        }

        Ok(SqlxBlockExecutionResult::Query(
            SqlxQueryResult::builder()
                .columns(column_names)
                .rows(results)
                .duration(duration)
                .build(),
        ))
    }

    async fn execute_statement(
        pool: &Self::Pool,
        statement: &str,
    ) -> Result<SqlxBlockExecutionResult, SqlxBlockError> {
        let (client, uri) = pool;

        log::info!("Executing statement: {}", statement);
        let request = client.post(uri).body(statement.to_string());

        let start_time = Instant::now();
        let response = request
            .send()
            .await
            .map_err(|e| SqlxBlockError::ConnectionError(e.to_string()))?;
        let duration = start_time.elapsed();
        log::info!("Duration: {:?}", duration);

        // Non-SELECT statement (INSERT, UPDATE, DELETE, CREATE, etc.)
        // ClickHouse HTTP interface returns success status for successful operations
        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .map_err(|e| SqlxBlockError::GenericError(e.to_string()))?;
            return Err(SqlxBlockError::QueryError(error_text));
        }

        log::info!("Response: {:?}", &response.text().await.unwrap());

        Ok(SqlxBlockExecutionResult::Statement(
            SqlxStatementResult::builder().duration(duration).build(),
        ))
    }
}
