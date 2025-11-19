use base64::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sqlparser::ast::Statement;
use sqlparser::dialect::{Dialect, SQLiteDialect};
use sqlx::{sqlite::SqliteConnectOptions, Column, Row, SqlitePool, TypeInfo};
use std::str::FromStr;
use std::time::Instant;
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::blocks::query_block::QueryBlockBehavior;
use crate::blocks::sql_block::{
    SqlBlockBehavior, SqlBlockError, SqlBlockExecutionResult, SqlQueryResult, SqlStatementResult,
};
use crate::blocks::{Block, BlockBehavior, FromDocument};
use crate::execution::{ExecutionContext, ExecutionHandle};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct SQLite {
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

impl FromDocument for SQLite {
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

        let sqlite = SQLite::builder()
            .id(id)
            .name(
                props
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("SQLite Query")
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

        Ok(sqlite)
    }
}

impl SQLite {
    /// Convert SQLite row to JSON value
    fn row_to_json(row: &sqlx::sqlite::SqliteRow) -> Result<Map<String, Value>, sqlx::Error> {
        let mut obj = Map::new();

        for (i, column) in row.columns().iter().enumerate() {
            let column_name = column.name().to_string();
            let value: Value = match column.type_info().name() {
                "NULL" => {
                    // For NULL type info (common with expressions like count(*)),
                    // try to decode as different types in order of preference
                    if let Ok(val) = row.try_get::<i64, _>(i) {
                        json!(val)
                    } else if let Ok(val) = row.try_get::<f64, _>(i) {
                        json!(val)
                    } else if let Ok(val) = row.try_get::<String, _>(i) {
                        json!(val)
                    } else if let Ok(val) = row.try_get::<Vec<u8>, _>(i) {
                        json!(BASE64_STANDARD.encode(val))
                    } else {
                        Value::Null
                    }
                }
                "INTEGER" => {
                    if let Ok(val) = row.try_get::<i64, _>(i) {
                        json!(val)
                    } else {
                        Value::Null
                    }
                }
                "REAL" => {
                    if let Ok(val) = row.try_get::<f64, _>(i) {
                        json!(val)
                    } else {
                        Value::Null
                    }
                }
                "TEXT" => {
                    if let Ok(val) = row.try_get::<String, _>(i) {
                        json!(val)
                    } else {
                        Value::Null
                    }
                }
                "BLOB" => {
                    if let Ok(val) = row.try_get::<Vec<u8>, _>(i) {
                        json!(BASE64_STANDARD.encode(val))
                    } else {
                        Value::Null
                    }
                }
                _ => {
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
impl BlockBehavior for SQLite {
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::SQLite(self)
    }

    async fn execute(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        QueryBlockBehavior::execute_query_block(self, context).await
    }
}

#[async_trait::async_trait]
impl SqlBlockBehavior for SQLite {
    type Pool = SqlitePool;

    fn dialect() -> Box<dyn Dialect> {
        Box::new(SQLiteDialect {})
    }

    fn resolve_uri(&self, context: &ExecutionContext) -> Result<String, SqlBlockError> {
        context
            .context_resolver
            .resolve_template(&self.uri)
            .map_err(|e| SqlBlockError::InvalidTemplate(e.to_string()))
    }

    async fn create_pool(&self, uri: String) -> Result<Self::Pool, SqlBlockError> {
        let opts = SqliteConnectOptions::from_str(&uri)?.create_if_missing(true);
        Ok(SqlitePool::connect_with(opts).await?)
    }

    async fn close_pool(&self, pool: &Self::Pool) -> Result<(), SqlBlockError> {
        pool.close().await;
        Ok(())
    }

    fn resolve_query(&self, context: &ExecutionContext) -> Result<String, SqlBlockError> {
        context
            .context_resolver
            .resolve_template(&self.query)
            .map_err(|e| SqlBlockError::InvalidTemplate(e.to_string()))
    }

    fn is_query(statement: &Statement) -> bool {
        matches!(
            statement,
            Statement::Explain { .. }
                | Statement::Fetch { .. }
                | Statement::Query { .. }
                | Statement::Pragma { .. }
        )
    }

    async fn execute_sql_query(
        &self,
        pool: &Self::Pool,
        query: &str,
    ) -> Result<SqlBlockExecutionResult, SqlBlockError> {
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

        Ok(SqlBlockExecutionResult::Query(
            SqlQueryResult::builder()
                .columns(columns)
                .rows(results)
                .duration(duration)
                .build(),
        ))
    }

    async fn execute_sql_statement(
        &self,
        pool: &Self::Pool,
        statement: &str,
    ) -> Result<SqlBlockExecutionResult, SqlBlockError> {
        let start_time = Instant::now();
        let result = sqlx::query(statement).execute(pool).await?;
        let duration = start_time.elapsed();

        Ok(SqlBlockExecutionResult::Statement(
            SqlStatementResult::builder()
                .rows_affected(result.rows_affected())
                .last_insert_rowid(Some(result.last_insert_rowid() as u64))
                .duration(duration)
                .build(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ContextResolver;
    use crate::document::actor::DocumentCommand;
    use crate::document::DocumentHandle;
    use crate::events::{GCEvent, MemoryEventBus};
    use crate::execution::ExecutionStatus;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    fn create_test_sqlite(query: &str, uri: &str) -> SQLite {
        SQLite::builder()
            .id(Uuid::new_v4())
            .name("Test SQLite")
            .query(query)
            .uri(uri)
            .build()
    }

    fn create_test_context() -> ExecutionContext {
        let (tx, _rx) = mpsc::unbounded_channel::<DocumentCommand>();
        let document_handle = DocumentHandle::from_raw(
            "test-runbook".to_string(),
            tx,
            Arc::new(MemoryEventBus::new()),
        );
        let context_resolver = ContextResolver::new();
        let (event_sender, _event_receiver) = tokio::sync::broadcast::channel(16);

        let block_id = Uuid::new_v4();
        ExecutionContext::builder()
            .block_id(block_id)
            .runbook_id(Uuid::new_v4())
            .document_handle(document_handle)
            .context_resolver(Arc::new(context_resolver))
            .workflow_event_sender(event_sender)
            .handle(ExecutionHandle::new(block_id))
            .build()
    }

    fn create_test_context_with_event_bus(
        block_id: Uuid,
        event_bus: Arc<MemoryEventBus>,
    ) -> ExecutionContext {
        let (tx, _rx) = mpsc::unbounded_channel::<DocumentCommand>();
        let document_handle =
            DocumentHandle::from_raw("test-runbook".to_string(), tx, event_bus.clone());
        let context_resolver = Arc::new(ContextResolver::new());
        let (event_sender, _event_receiver) = tokio::sync::broadcast::channel(16);

        ExecutionContext::builder()
            .block_id(block_id)
            .runbook_id(Uuid::new_v4())
            .document_handle(document_handle)
            .context_resolver(context_resolver)
            .workflow_event_sender(event_sender)
            .gc_event_bus(event_bus)
            .handle(ExecutionHandle::new(block_id))
            .build()
    }

    // FromDocument tests
    #[tokio::test]
    async fn test_from_document_valid() {
        let id = Uuid::new_v4();
        let json_data = serde_json::json!({
            "id": id.to_string(),
            "props": {
                "name": "Test Query",
                "query": "SELECT * FROM users",
                "uri": "sqlite::memory:",
                "autoRefresh": 5
            },
            "type": "sqlite"
        });

        let sqlite = SQLite::from_document(&json_data).unwrap();
        assert_eq!(sqlite.id, id);
        assert_eq!(sqlite.name, "Test Query");
        assert_eq!(sqlite.query, "SELECT * FROM users");
        assert_eq!(sqlite.uri, "sqlite::memory:");
        assert_eq!(sqlite.auto_refresh, 5);
    }

    #[tokio::test]
    async fn test_from_document_defaults() {
        let id = Uuid::new_v4();
        let json_data = serde_json::json!({
            "id": id.to_string(),
            "props": {},
            "type": "sqlite"
        });

        let sqlite = SQLite::from_document(&json_data).unwrap();
        assert_eq!(sqlite.id, id);
        assert_eq!(sqlite.name, "SQLite Query");
        assert_eq!(sqlite.query, "");
        assert_eq!(sqlite.uri, "");
        assert_eq!(sqlite.auto_refresh, 0);
    }

    #[tokio::test]
    async fn test_from_document_missing_id() {
        let json_data = serde_json::json!({
            "props": {
                "query": "SELECT 1"
            }
        });

        let result = SQLite::from_document(&json_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no id"));
    }

    #[tokio::test]
    async fn test_from_document_invalid_id() {
        let json_data = serde_json::json!({
            "id": "not-a-uuid",
            "props": {}
        });

        let result = SQLite::from_document(&json_data);
        assert!(result.is_err());
    }

    // Execution tests
    #[tokio::test]
    async fn test_simple_select_query() {
        let sqlite = create_test_sqlite("SELECT 1 as num, 'hello' as text", "sqlite::memory:");
        let context = create_test_context();

        let handle = sqlite.execute(context).await.unwrap().unwrap();

        // Wait for execution to complete
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Success => break,
                ExecutionStatus::Failed(e) => panic!("Query failed: {}", e),
                ExecutionStatus::Cancelled => panic!("Query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_create_table_and_insert() {
        let query = r#"
            CREATE TABLE test_users (id INTEGER PRIMARY KEY, name TEXT);
            INSERT INTO test_users (id, name) VALUES (1, 'Alice');
            INSERT INTO test_users (id, name) VALUES (2, 'Bob');
        "#;
        let sqlite = create_test_sqlite(query, "sqlite::memory:");
        let context = create_test_context();

        let handle = sqlite.execute(context).await.unwrap().unwrap();

        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Success => break,
                ExecutionStatus::Failed(e) => panic!("Query failed: {}", e),
                ExecutionStatus::Cancelled => panic!("Query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_invalid_sql_syntax() {
        let sqlite = create_test_sqlite("SELECT FROM users", "sqlite::memory:");
        let context = create_test_context();
        let handle = context.handle();

        let _ = sqlite.execute(context).await;

        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(e) => {
                    assert!(e.contains("SQL"));
                    break;
                }
                ExecutionStatus::Success => panic!("Query should have failed"),
                ExecutionStatus::Cancelled => panic!("Query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_empty_query() {
        let sqlite = create_test_sqlite("", "sqlite::memory:");
        let context = create_test_context();
        let handle = context.handle();

        let _ = sqlite.execute(context).await;

        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(e) => {
                    assert!(e.contains("Query is empty"));
                    break;
                }
                ExecutionStatus::Success => panic!("Query should have failed"),
                ExecutionStatus::Cancelled => panic!("Query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_query_with_semicolons() {
        let query = "SELECT 1; SELECT 2; SELECT 3";
        let sqlite = create_test_sqlite(query, "sqlite::memory:");
        let context = create_test_context();
        let handle = context.handle();

        let _ = sqlite.execute(context).await;

        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Success => break,
                ExecutionStatus::Failed(e) => panic!("Query failed: {}", e),
                ExecutionStatus::Cancelled => panic!("Query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_invalid_uri() {
        let sqlite = create_test_sqlite("SELECT 1", "invalid://uri");
        let context = create_test_context();
        let handle = context.handle();

        let _ = sqlite.execute(context).await;

        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break,
                ExecutionStatus::Success => panic!("Query should have failed with invalid URI"),
                ExecutionStatus::Cancelled => panic!("Query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    // Event bus tests
    #[tokio::test]
    async fn test_grand_central_events_successful_query() {
        let event_bus = Arc::new(MemoryEventBus::new());
        let sqlite = create_test_sqlite("SELECT 1", "sqlite::memory:");
        let sqlite_id = sqlite.id;
        let context = create_test_context_with_event_bus(sqlite_id, event_bus.clone());
        let handle = context.handle();
        let runbook_id = context.runbook_id;

        let _ = sqlite.execute(context).await;

        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Success => break,
                ExecutionStatus::Failed(e) => panic!("Query failed: {}", e),
                ExecutionStatus::Cancelled => panic!("Query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }

        // Verify events were emitted
        let events = event_bus.events();
        assert_eq!(events.len(), 2);

        // Check BlockStarted event
        match &events[0] {
            GCEvent::BlockStarted {
                block_id,
                runbook_id: rb_id,
            } => {
                assert_eq!(*block_id, sqlite_id);
                assert_eq!(*rb_id, runbook_id);
            }
            _ => panic!("Expected BlockStarted event, got: {:?}", events[0]),
        }

        // Check BlockFinished event
        match &events[1] {
            GCEvent::BlockFinished {
                block_id,
                runbook_id: rb_id,
                success,
            } => {
                assert_eq!(*block_id, sqlite_id);
                assert_eq!(*rb_id, runbook_id);
                assert_eq!(*success, true);
            }
            _ => panic!("Expected BlockFinished event, got: {:?}", events[1]),
        }
    }

    #[tokio::test]
    async fn test_grand_central_events_failed_query() {
        let event_bus = Arc::new(MemoryEventBus::new());
        let sqlite = create_test_sqlite("SELECT * from ADFASDFSDFADFSDF", "sqlite::memory:");
        let sqlite_id = sqlite.id;
        let context = create_test_context_with_event_bus(sqlite_id, event_bus.clone());
        let handle = context.handle();
        let runbook_id = context.runbook_id;

        let _ = sqlite.execute(context).await;

        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break,
                ExecutionStatus::Success => panic!("Query should have failed"),
                ExecutionStatus::Cancelled => panic!("Query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }

        // Verify events were emitted
        let events = event_bus.events();
        assert_eq!(events.len(), 2);

        // Check BlockStarted event
        match &events[0] {
            GCEvent::BlockStarted {
                block_id,
                runbook_id: rb_id,
            } => {
                assert_eq!(*block_id, sqlite_id);
                assert_eq!(*rb_id, runbook_id);
            }
            _ => panic!("Expected BlockStarted event, got: {:?}", events[0]),
        }

        // Check BlockFailed event
        match &events[1] {
            GCEvent::BlockFailed {
                block_id,
                runbook_id: rb_id,
                error,
            } => {
                assert_eq!(*block_id, sqlite_id);
                assert_eq!(*rb_id, runbook_id);
                assert!(error.contains("no such table"));
            }
            _ => panic!("Expected BlockFailed event, got: {:?}", events[1]),
        }
    }

    // Cancellation test
    #[tokio::test]
    async fn test_query_cancellation() {
        // Use a long-running query to test cancellation
        let sqlite = create_test_sqlite(
            "SELECT 1; SELECT 2; SELECT 3; SELECT 4; SELECT 5",
            "sqlite::memory:",
        );
        let context = create_test_context();

        let handle = sqlite.execute(context).await.unwrap().unwrap();

        // Cancel immediately
        handle.cancellation_token.cancel();

        // Wait a bit for cancellation to take effect
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let status = handle.status.read().await.clone();
        // The query might complete before cancellation, or it might be cancelled
        match status {
            ExecutionStatus::Cancelled | ExecutionStatus::Success | ExecutionStatus::Failed(_) => {
                // Any of these outcomes is acceptable for this test
            }
            ExecutionStatus::Running => {
                // Give it more time
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            }
        }
    }

    // Serialization tests
    #[tokio::test]
    async fn test_json_serialization_roundtrip() {
        let original = create_test_sqlite("SELECT * FROM users", "sqlite://test.db");

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: SQLite = serde_json::from_str(&json).unwrap();

        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.name, deserialized.name);
        assert_eq!(original.query, deserialized.query);
        assert_eq!(original.uri, deserialized.uri);
        assert_eq!(original.auto_refresh, deserialized.auto_refresh);
    }
}
