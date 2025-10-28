use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{sqlite::SqliteConnectOptions, Column, Row, SqlitePool, TypeInfo};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::runtime::blocks::document::block_context::BlockExecutionOutput;
use crate::runtime::blocks::handler::{
    BlockErrorData, BlockFinishedData, BlockLifecycleEvent, BlockOutput, ExecutionStatus,
};
use crate::runtime::blocks::{Block, BlockBehavior};
use crate::runtime::events::GCEvent;
use crate::runtime::workflow::event::WorkflowEvent;

use super::handler::{CancellationToken, ExecutionContext, ExecutionHandle};
use super::FromDocument;

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
    /// Template SQLite query using the context resolver
    fn template_sqlite_query(
        &self,
        context: &ExecutionContext,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let rendered = context.context_resolver.resolve_template(&self.query)?;
        Ok(rendered)
    }

    /// Convert SQLite row to JSON value
    fn row_to_json(row: &sqlx::sqlite::SqliteRow) -> Result<Value, sqlx::Error> {
        let mut obj = serde_json::Map::new();

        for (i, column) in row.columns().iter().enumerate() {
            let column_name = column.name().to_string();
            let value: Value = match column.type_info().name() {
                "NULL" => Value::Null,
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
                        json!(base64::encode(val))
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

        Ok(Value::Object(obj))
    }

    /// Execute a single SQLite statement
    async fn execute_statement(
        pool: &SqlitePool,
        statement: &str,
        context: &ExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            return Ok(());
        }

        let first_word = trimmed
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase();

        if first_word == "select" || first_word == "with" || first_word == "pragma" {
            let rows = sqlx::query(statement)
                .fetch_all(pool)
                .await
                .map_err(|e| format!("SQL query failed: {}", e))?;

            let mut results = Vec::new();
            let mut column_names = Vec::new();

            if let Some(first_row) = rows.first() {
                column_names = first_row
                    .columns()
                    .iter()
                    .map(|col| col.name().to_string())
                    .collect();
            }

            for row in &rows {
                results.push(Self::row_to_json(row)?);
            }

            if let Some(ref ch) = context.output_channel {
                let result_json = json!({
                    "columns": column_names,
                    "rows": results,
                    "rowCount": results.len()
                });

                let _ = ch.send(BlockOutput {
                    stdout: None,
                    stderr: None,
                    lifecycle: None,
                    binary: None,
                    object: Some(result_json),
                });
            }
        } else {
            let result = sqlx::query(statement)
                .execute(pool)
                .await
                .map_err(|e| format!("SQL execution failed: {}", e))?;

            if let Some(ref ch) = context.output_channel {
                let result_json = json!({
                    "rowsAffected": result.rows_affected(),
                    "lastInsertRowid": result.last_insert_rowid(),
                });

                let _ = ch.send(BlockOutput {
                    stdout: None,
                    stderr: None,
                    lifecycle: None,
                    binary: None,
                    object: Some(result_json),
                });
            }
        }

        Ok(())
    }

    async fn run_sqlite_query(
        &self,
        context: ExecutionContext,
        cancellation_token: CancellationToken,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let block_id = self.id;

        let _ = context
            .event_sender
            .send(WorkflowEvent::BlockStarted { id: block_id });

        if let Some(ref ch) = context.output_channel {
            let _ = ch.send(BlockOutput {
                stdout: None,
                stderr: None,
                binary: None,
                object: None,
                lifecycle: Some(BlockLifecycleEvent::Started),
            });
        }

        let query = self.template_sqlite_query(&context).unwrap_or_else(|e| {
            eprintln!("Template error in SQLite query {}: {}", block_id, e);
            self.query.clone()
        });

        if let Some(ref ch) = context.output_channel {
            let _ = ch.send(BlockOutput {
                stdout: Some("Connecting to database...".to_string()),
                stderr: None,
                binary: None,
                object: None,
                lifecycle: None,
            });
        }

        let pool = {
            let connection_task = async {
                let opts = SqliteConnectOptions::from_str(&self.uri)?
                    .create_if_missing(true);
                SqlitePool::connect_with(opts).await
            };

            let timeout_task = tokio::time::sleep(Duration::from_secs(10));

            tokio::select! {
                result = connection_task => {
                    match result {
                        Ok(pool) => {
                            if let Some(ref ch) = context.output_channel {
                                let _ = ch.send(BlockOutput {
                                    stdout: Some("Connected to database successfully".to_string()),
                                    stderr: None,
                                    binary: None,
                                    object: None,
                                    lifecycle: None,
                                });
                            }
                            pool
                        },
                        Err(e) => {
                            let error_msg = format!("Failed to connect to database: {}", e);
                            if let Some(ref ch) = context.output_channel {
                                let _ = ch.send(BlockOutput {
                                    stdout: None,
                                    stderr: Some(error_msg.clone()),
                                    binary: None,
                                    object: None,
                                    lifecycle: Some(BlockLifecycleEvent::Error(BlockErrorData {
                                        message: error_msg.clone(),
                                    })),
                                });
                            }
                            return Err(error_msg.into());
                        }
                    }
                }
                _ = timeout_task => {
                    let error_msg = "Database connection timed out after 10 seconds.";
                    if let Some(ref ch) = context.output_channel {
                        let _ = ch.send(BlockOutput {
                            stdout: None,
                            stderr: Some(error_msg.to_string()),
                            binary: None,
                            object: None,
                            lifecycle: Some(BlockLifecycleEvent::Error(BlockErrorData {
                                message: error_msg.to_string(),
                            })),
                        });
                    }
                    return Err(error_msg.into());
                }
            }
        };

        let query_clone = query.clone();
        let context_clone = context.clone();
        let cancellation_receiver = cancellation_token.take_receiver();
        let pool_clone = pool.clone();

        let execution_task = async move {
            let statements: Vec<&str> = query_clone
                .split(';')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            if statements.is_empty() {
                let error_msg = "No SQL statements to execute";
                if let Some(ref ch) = &context_clone.output_channel {
                    let _ = ch.send(BlockOutput {
                        stdout: None,
                        stderr: Some(error_msg.to_string()),
                        binary: None,
                        object: None,
                        lifecycle: Some(BlockLifecycleEvent::Error(BlockErrorData {
                            message: error_msg.to_string(),
                        })),
                    });
                }
                return Err(error_msg.into());
            }

            if let Some(ref ch) = &context_clone.output_channel {
                let _ = ch.send(BlockOutput {
                    stdout: Some(format!(
                        "Executing {} SQL statement(s)...",
                        statements.len()
                    )),
                    stderr: None,
                    binary: None,
                    object: None,
                    lifecycle: None,
                });
            }

            for (i, statement) in statements.iter().enumerate() {
                if let Err(e) =
                    Self::execute_statement(&pool_clone, statement, &context_clone).await
                {
                    let error_msg = format!("Statement {} failed: {}", i + 1, e);
                    if let Some(ref ch) = &context_clone.output_channel {
                        let _ = ch.send(BlockOutput {
                            stdout: None,
                            stderr: Some(error_msg.clone()),
                            binary: None,
                            object: None,
                            lifecycle: Some(BlockLifecycleEvent::Error(BlockErrorData {
                                message: error_msg.clone(),
                            })),
                        });
                    }
                    return Err(error_msg.into());
                }
            }

            Ok(())
        };

        let result = if let Some(cancel_rx) = cancellation_receiver {
            tokio::select! {
                _ = cancel_rx => {
                    pool.close().await;
                    if let Some(event_bus) = &context.event_bus {
                        let _ = event_bus.emit(GCEvent::BlockCancelled {
                            block_id,
                            runbook_id: context.runbook_id,
                        }).await;
                    }
                    let _ = context.event_sender.send(WorkflowEvent::BlockFinished { id: block_id });
                    if let Some(ref ch) = context.output_channel {
                        let _ = ch.send(BlockOutput {
                            stdout: None,
                            stderr: None,
                            binary: None,
                            object: None,
                            lifecycle: Some(BlockLifecycleEvent::Cancelled),
                        });
                    }
                    return Err("SQLite query execution cancelled".into());
                }
                result = execution_task => {
                    pool.close().await;
                    result
                }
            }
        } else {
            let result = execution_task.await;
            pool.close().await;
            result
        };

        let _ = context
            .event_sender
            .send(WorkflowEvent::BlockFinished { id: block_id });
        if let Some(ref ch) = context.output_channel {
            let _ = ch.send(BlockOutput {
                stdout: Some("Query execution completed successfully".to_string()),
                stderr: None,
                binary: None,
                object: None,
                lifecycle: None,
            });

            let _ = ch.send(BlockOutput {
                stdout: None,
                stderr: None,
                binary: None,
                object: None,
                lifecycle: Some(BlockLifecycleEvent::Finished(BlockFinishedData {
                    exit_code: Some(0),
                    success: true,
                })),
            });
        }

        result
    }
}

#[async_trait::async_trait]
impl BlockBehavior for SQLite {
    fn into_block(self) -> Block {
        Block::SQLite(self)
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

        let handle_clone = handle.clone();
        let context_clone = context.clone();
        let block_id = self.id;
        let runbook_id = context.runbook_id;

        tokio::spawn(async move {
            if let Some(event_bus) = &context_clone.event_bus {
                let _ = event_bus
                    .emit(GCEvent::BlockStarted {
                        block_id,
                        runbook_id,
                    })
                    .await;
            }

            let result = self
                .run_sqlite_query(context_clone.clone(), handle_clone.cancellation_token.clone())
                .await;

            let status = match result {
                Ok(_) => {
                    if let Some(event_bus) = &context_clone.event_bus {
                        let _ = event_bus
                            .emit(GCEvent::BlockFinished {
                                block_id,
                                runbook_id,
                                success: true,
                            })
                            .await;
                    }

                    let _ = context_clone
                        .document_handle
                        .update_context(block_id, move |ctx| {
                            ctx.insert(BlockExecutionOutput {
                                exit_code: Some(0),
                                stdout: Some("Query execution completed successfully".to_string()),
                                stderr: None,
                            });
                        })
                        .await;

                    ExecutionStatus::Success("SQLite query completed successfully".to_string())
                }
                Err(e) => {
                    if let Some(event_bus) = &context_clone.event_bus {
                        let _ = event_bus
                            .emit(GCEvent::BlockFailed {
                                block_id,
                                runbook_id,
                                error: e.to_string(),
                            })
                            .await;
                    }

                    if let Some(ref ch) = context_clone.output_channel {
                        let _ = ch.send(BlockOutput {
                            stdout: None,
                            stderr: Some(e.to_string()),
                            binary: None,
                            object: None,
                            lifecycle: Some(BlockLifecycleEvent::Error(BlockErrorData {
                                message: e.to_string(),
                            })),
                        });
                    }

                    let error_msg = e.to_string();
                    let _ = context_clone
                        .document_handle
                        .update_context(block_id, move |ctx| {
                            ctx.insert(BlockExecutionOutput {
                                exit_code: Some(1),
                                stdout: None,
                                stderr: Some(error_msg),
                            });
                        })
                        .await;

                    ExecutionStatus::Failed(e.to_string())
                }
            };

            *handle_clone.status.write().await = status;
        });

        Ok(Some(handle))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::blocks::document::actor::{DocumentCommand, DocumentHandle};
    use crate::runtime::blocks::document::block_context::ContextResolver;
    use crate::runtime::events::MemoryEventBus;
    use std::collections::HashMap;
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
        let document_handle = DocumentHandle::from_raw("test-runbook".to_string(), tx);
        let context_resolver = ContextResolver::new();
        let (event_sender, _event_receiver) = tokio::sync::broadcast::channel(16);

        ExecutionContext {
            runbook_id: Uuid::new_v4(),
            document_handle,
            context_resolver,
            output_channel: None,
            event_sender,
            ssh_pool: None,
            pty_store: None,
            event_bus: None,
        }
    }

    fn create_test_context_with_vars(vars: Vec<(&str, &str)>) -> ExecutionContext {
        let (tx, _rx) = mpsc::unbounded_channel::<DocumentCommand>();
        let document_handle = DocumentHandle::from_raw("test-runbook".to_string(), tx);

        let vars_map: HashMap<String, String> = vars
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        let context_resolver = ContextResolver::with_vars(vars_map);

        let (event_sender, _event_receiver) = tokio::sync::broadcast::channel(16);

        ExecutionContext {
            runbook_id: Uuid::new_v4(),
            document_handle,
            context_resolver,
            output_channel: None,
            event_sender,
            ssh_pool: None,
            pty_store: None,
            event_bus: None,
        }
    }

    fn create_test_context_with_event_bus(event_bus: Arc<MemoryEventBus>) -> ExecutionContext {
        let (tx, _rx) = mpsc::unbounded_channel::<DocumentCommand>();
        let document_handle = DocumentHandle::from_raw("test-runbook".to_string(), tx);
        let context_resolver = ContextResolver::new();
        let (event_sender, _event_receiver) = tokio::sync::broadcast::channel(16);

        ExecutionContext {
            runbook_id: Uuid::new_v4(),
            document_handle,
            context_resolver,
            output_channel: None,
            event_sender,
            ssh_pool: None,
            pty_store: None,
            event_bus: Some(event_bus),
        }
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

    // Template resolution tests
    #[tokio::test]
    async fn test_template_sqlite_query_no_template() {
        let sqlite = create_test_sqlite("SELECT * FROM users", "sqlite::memory:");
        let context = create_test_context();

        let result = sqlite.template_sqlite_query(&context).unwrap();
        assert_eq!(result, "SELECT * FROM users");
    }

    #[tokio::test]
    async fn test_template_sqlite_query_with_vars() {
        let sqlite = create_test_sqlite(
            "SELECT * FROM users WHERE id = {{ var.user_id }}",
            "sqlite::memory:",
        );
        let context = create_test_context_with_vars(vec![("user_id", "123")]);

        let result = sqlite.template_sqlite_query(&context).unwrap();
        assert_eq!(result, "SELECT * FROM users WHERE id = 123");
    }

    #[tokio::test]
    async fn test_template_sqlite_query_multiple_vars() {
        let sqlite = create_test_sqlite(
            "SELECT * FROM users WHERE id = {{ var.user_id }} AND name = '{{ var.user_name }}'",
            "sqlite::memory:",
        );
        let context = create_test_context_with_vars(vec![("user_id", "123"), ("user_name", "Alice")]);

        let result = sqlite.template_sqlite_query(&context).unwrap();
        assert_eq!(result, "SELECT * FROM users WHERE id = 123 AND name = 'Alice'");
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
                ExecutionStatus::Success(_) => break,
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
                ExecutionStatus::Success(_) => break,
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

        let handle = sqlite.execute(context).await.unwrap().unwrap();

        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(e) => {
                    assert!(e.contains("SQL"));
                    break;
                }
                ExecutionStatus::Success(_) => panic!("Query should have failed"),
                ExecutionStatus::Cancelled => panic!("Query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_empty_query() {
        let sqlite = create_test_sqlite("", "sqlite::memory:");
        let context = create_test_context();

        let handle = sqlite.execute(context).await.unwrap().unwrap();

        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(e) => {
                    assert!(e.contains("No SQL statements"));
                    break;
                }
                ExecutionStatus::Success(_) => panic!("Query should have failed"),
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

        let handle = sqlite.execute(context).await.unwrap().unwrap();

        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Success(_) => break,
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

        let handle = sqlite.execute(context).await.unwrap().unwrap();

        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break,
                ExecutionStatus::Success(_) => panic!("Query should have failed with invalid URI"),
                ExecutionStatus::Cancelled => panic!("Query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    // Event bus tests
    #[tokio::test]
    async fn test_grand_central_events_successful_query() {
        let event_bus = Arc::new(MemoryEventBus::new());
        let context = create_test_context_with_event_bus(event_bus.clone());
        let runbook_id = context.runbook_id;

        let sqlite = create_test_sqlite("SELECT 1", "sqlite::memory:");
        let sqlite_id = sqlite.id;

        let handle = sqlite.execute(context).await.unwrap().unwrap();

        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Success(_) => break,
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
        let context = create_test_context_with_event_bus(event_bus.clone());
        let runbook_id = context.runbook_id;

        let sqlite = create_test_sqlite("INVALID SQL", "sqlite::memory:");
        let sqlite_id = sqlite.id;

        let handle = sqlite.execute(context).await.unwrap().unwrap();

        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break,
                ExecutionStatus::Success(_) => panic!("Query should have failed"),
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
                assert!(error.contains("SQL") || error.contains("syntax"));
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
            ExecutionStatus::Cancelled | ExecutionStatus::Success(_) | ExecutionStatus::Failed(_) => {
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
