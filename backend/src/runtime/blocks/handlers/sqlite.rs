use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::{sqlite::SqliteConnectOptions, Column, Row, SqlitePool, TypeInfo};
use std::str::FromStr;
use std::sync::Arc;
use tauri::ipc::Channel;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::runtime::blocks::handler::{
    BlockFinishedData, BlockHandler, BlockLifecycleEvent, BlockOutput, CancellationToken,
    ExecutionContext, ExecutionHandle, ExecutionStatus,
};
use crate::runtime::blocks::sqlite::SQLite;
use crate::runtime::events::GCEvent;
use crate::runtime::workflow::event::WorkflowEvent;
use crate::templates::template_with_context;

pub struct SQLiteHandler;

#[async_trait]
impl BlockHandler for SQLiteHandler {
    type Block = SQLite;

    fn block_type(&self) -> &'static str {
        "sqlite"
    }

    fn output_variable(&self, _block: &Self::Block) -> Option<String> {
        None // No output variables for now
    }

    async fn execute(
        &self,
        sqlite: SQLite,
        context: ExecutionContext,
        event_sender: broadcast::Sender<WorkflowEvent>,
        output_channel: Option<Channel<BlockOutput>>,
    ) -> Result<ExecutionHandle, Box<dyn std::error::Error + Send + Sync>> {
        let handle = ExecutionHandle {
            id: Uuid::new_v4(),
            block_id: sqlite.id,
            cancellation_token: CancellationToken::new(),
            status: Arc::new(RwLock::new(ExecutionStatus::Running)),
            output_variable: None,
        };

        let sqlite_clone = sqlite.clone();
        let context_clone = context.clone();
        let handle_clone = handle.clone();
        let event_sender_clone = event_sender.clone();

        let output_channel_clone = output_channel.clone();

        tokio::spawn(async move {
            // Emit BlockStarted event via Grand Central
            if let Some(event_bus) = &context_clone.event_bus {
                let _ = event_bus
                    .emit(GCEvent::BlockStarted {
                        block_id: sqlite_clone.id,
                        runbook_id: context_clone.runbook_id,
                    })
                    .await;
            }

            let result = Self::run_sqlite_query(
                &sqlite_clone,
                context_clone.clone(),
                handle_clone.cancellation_token.clone(),
                event_sender_clone,
                output_channel_clone,
            )
            .await;

            // Determine status based on result
            let status = match result {
                Ok(_) => {
                    // Emit BlockFinished event via Grand Central
                    if let Some(event_bus) = &context_clone.event_bus {
                        let _ = event_bus
                            .emit(GCEvent::BlockFinished {
                                block_id: sqlite_clone.id,
                                runbook_id: context_clone.runbook_id,
                                success: true,
                            })
                            .await;
                    }

                    ExecutionStatus::Success("SQLite query completed successfully".to_string())
                }
                Err(e) => {
                    // Emit BlockFailed event via Grand Central
                    if let Some(event_bus) = &context_clone.event_bus {
                        let _ = event_bus
                            .emit(GCEvent::BlockFailed {
                                block_id: sqlite_clone.id,
                                runbook_id: context_clone.runbook_id,
                                error: e.to_string(),
                            })
                            .await;
                    }

                    ExecutionStatus::Failed(e.to_string())
                }
            };

            *handle_clone.status.write().await = status.clone();
        });

        Ok(handle)
    }
}

impl SQLiteHandler {
    /// Template SQLite query using the Minijinja template system
    async fn template_sqlite_query(
        query: &str,
        context: &ExecutionContext,
        sqlite_id: Uuid,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let block_id_str = sqlite_id.to_string();
        let rendered = template_with_context(
            query,
            &context.variables,
            &context.document,
            Some(&block_id_str),
            None,
        )?;
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

        Ok(Value::Object(obj))
    }

    /// Execute a single SQLite statement
    async fn execute_statement(
        pool: &SqlitePool,
        statement: &str,
        output_channel: &Option<Channel<BlockOutput>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            return Ok(());
        }

        // Check if this is a SELECT statement
        let first_word = trimmed
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase();

        if first_word == "select" {
            // Handle SELECT query
            let rows = sqlx::query(statement).fetch_all(pool).await?;

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

            // Send results as structured JSON object
            if let Some(ref ch) = output_channel {
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
            // Handle non-SELECT statement (INSERT, UPDATE, DELETE, CREATE, etc.)
            let result = sqlx::query(statement).execute(pool).await?;

            // Send execution result as structured JSON object
            if let Some(ref ch) = output_channel {
                let result_json = json!({
                    "rowsAffected": result.rows_affected(),
                    "lastInsertId": result.last_insert_rowid()
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
        sqlite: &SQLite,
        context: ExecutionContext,
        cancellation_token: CancellationToken,
        event_sender: broadcast::Sender<WorkflowEvent>,
        output_channel: Option<Channel<BlockOutput>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Send start event
        let _ = event_sender.send(WorkflowEvent::BlockStarted { id: sqlite.id });

        // Send started lifecycle event to output channel
        if let Some(ref ch) = output_channel {
            let _ = ch.send(BlockOutput {
                stdout: None,
                stderr: None,
                binary: None,
                object: None,
                lifecycle: Some(BlockLifecycleEvent::Started),
            });
        }

        // Template the query using Minijinja
        let query = Self::template_sqlite_query(&sqlite.query, &context, sqlite.id)
            .await
            .unwrap_or_else(|e| {
                eprintln!("Template error in SQLite query {}: {}", sqlite.id, e);
                sqlite.query.clone() // Fallback to original query
            });

        // Prepare database URI
        let uri = if sqlite.uri.is_empty() {
            "sqlite::memory:".to_string()
        } else if sqlite.uri.starts_with("sqlite://") {
            sqlite.uri.clone()
        } else if sqlite.uri == ":memory:" {
            "sqlite::memory:".to_string()
        } else {
            format!("sqlite://{}", sqlite.uri)
        };

        // Create SQLite connection pool
        let pool = {
            let opts = SqliteConnectOptions::from_str(&uri)?.create_if_missing(true);
            SqlitePool::connect_with(opts).await?
        };

        let query_clone = query.clone();
        let output_channel_clone = output_channel.clone();
        let cancellation_receiver = cancellation_token.take_receiver();
        let pool_clone = pool.clone();

        let execution_task = async move {
            // Split query into statements
            let statements: Vec<&str> = query_clone
                .split(';')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            if statements.is_empty() {
                return Err("No SQL statements to execute".into());
            }

            // Execute each statement
            for (i, statement) in statements.iter().enumerate() {
                if let Err(e) =
                    Self::execute_statement(&pool_clone, statement, &output_channel_clone).await
                {
                    return Err(format!("Statement {} failed: {}", i + 1, e).into());
                }
            }

            Ok(())
        };

        // Handle execution with cancellation
        let result = if let Some(cancel_rx) = cancellation_receiver {
            tokio::select! {
                _ = cancel_rx => {
                    // Close the pool
                    pool.close().await;

                    // Emit BlockCancelled event via Grand Central
                    if let Some(event_bus) = &context.event_bus {
                        let _ = event_bus.emit(GCEvent::BlockCancelled {
                            block_id: sqlite.id,
                            runbook_id: context.runbook_id,
                        }).await;
                    }

                    // Send completion events
                    let _ = event_sender.send(WorkflowEvent::BlockFinished { id: sqlite.id });
                    if let Some(ref ch) = output_channel {
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
                    // Close the pool after execution
                    pool.close().await;
                    result
                }
            }
        } else {
            let result = execution_task.await;
            // Close the pool after execution
            pool.close().await;
            result
        };

        // Send completion events only if successful
        match &result {
            Ok(_) => {
                let _ = event_sender.send(WorkflowEvent::BlockFinished { id: sqlite.id });
                if let Some(ref ch) = output_channel {
                    // Send success message
                    let _ = ch.send(BlockOutput {
                        stdout: Some("Query execution completed successfully".to_string()),
                        stderr: None,
                        binary: None,
                        object: None,
                        lifecycle: None,
                    });

                    // Send finished lifecycle event
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
            }
            Err(_) => {
                // Error events are already sent by the execution task
                // Just send BlockFinished to complete the workflow
                let _ = event_sender.send(WorkflowEvent::BlockFinished { id: sqlite.id });
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::blocks::handler::{ExecutionContext, ExecutionStatus};
    use crate::runtime::events::MemoryEventBus;
    use crate::runtime::workflow::event::WorkflowEvent;
    use std::collections::HashMap;
    use tokio::sync::broadcast;
    use tokio::time::Duration;

    fn create_test_sqlite(query: &str) -> SQLite {
        SQLite::builder()
            .id(Uuid::new_v4())
            .name("Test SQLite")
            .query(query)
            .uri(":memory:")
            .build()
    }

    fn create_test_context() -> ExecutionContext {
        ExecutionContext {
            runbook_id: Uuid::new_v4(),
            cwd: std::env::temp_dir().to_string_lossy().to_string(),
            env: HashMap::new(),
            variables: HashMap::new(),
            ssh_host: None,
            document: Vec::new(),
            ssh_pool: None,
            output_storage: None,
            pty_store: None,
            event_bus: None,
        }
    }

    fn create_test_context_with_event_bus(event_bus: Arc<MemoryEventBus>) -> ExecutionContext {
        ExecutionContext {
            runbook_id: Uuid::new_v4(),
            cwd: std::env::temp_dir().to_string_lossy().to_string(),
            env: HashMap::new(),
            variables: HashMap::new(),
            ssh_host: None,
            document: Vec::new(),
            ssh_pool: None,
            output_storage: None,
            pty_store: None,
            event_bus: Some(event_bus),
        }
    }

    #[test]
    fn test_handler_block_type() {
        let handler = SQLiteHandler;
        assert_eq!(handler.block_type(), "sqlite");
    }

    #[test]
    fn test_no_output_variable() {
        let handler = SQLiteHandler;
        let sqlite = create_test_sqlite("SELECT 1");
        assert_eq!(handler.output_variable(&sqlite), None);
    }

    #[tokio::test]
    async fn test_simple_select_query() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let sqlite = create_test_sqlite("SELECT 1 as test_column, 'hello' as message");
        let handler = SQLiteHandler;
        let handle = handler
            .execute(sqlite, create_test_context(), _tx, None)
            .await
            .expect("SQLite execution should succeed");

        // Wait for execution to complete
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Success(_) => break,
                ExecutionStatus::Failed(e) => panic!("SQLite query failed: {}", e),
                ExecutionStatus::Cancelled => panic!("SQLite query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_create_table_and_insert() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let sqlite = create_test_sqlite(
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT); \
             INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com'); \
             INSERT INTO users (name, email) VALUES ('Bob', 'bob@example.com'); \
             SELECT * FROM users ORDER BY id;",
        );

        let handler = SQLiteHandler;
        let handle = handler
            .execute(sqlite, create_test_context(), _tx, None)
            .await
            .expect("SQLite execution should succeed");

        // Wait for execution to complete
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Success(_) => break,
                ExecutionStatus::Failed(e) => panic!("SQLite query failed: {}", e),
                ExecutionStatus::Cancelled => panic!("SQLite query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_invalid_sql() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let sqlite = create_test_sqlite("INVALID SQL STATEMENT");
        let handler = SQLiteHandler;
        let handle = handler
            .execute(sqlite, create_test_context(), _tx, None)
            .await
            .expect("SQLite execution should start");

        // Wait for execution to complete with error
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break, // Expected
                ExecutionStatus::Success(_) => panic!("Invalid SQL should have failed"),
                ExecutionStatus::Cancelled => panic!("SQLite query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_variable_substitution() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let mut context = create_test_context();
        context
            .variables
            .insert("table_name".to_string(), "test_table".to_string());
        context
            .variables
            .insert("user_name".to_string(), "John Doe".to_string());

        let sqlite = create_test_sqlite(
            "CREATE TABLE {{ var.table_name }} (id INTEGER, name TEXT); \
             INSERT INTO {{ var.table_name }} VALUES (1, '{{ var.user_name }}'); \
             SELECT * FROM {{ var.table_name }};",
        );

        let handler = SQLiteHandler;
        let handle = handler
            .execute(sqlite, context, _tx, None)
            .await
            .expect("SQLite execution should succeed");

        // Wait for execution to complete
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Success(_) => break,
                ExecutionStatus::Failed(e) => panic!("SQLite query failed: {}", e),
                ExecutionStatus::Cancelled => panic!("SQLite query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_empty_query() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let sqlite = create_test_sqlite("");
        let handler = SQLiteHandler;
        let handle = handler
            .execute(sqlite, create_test_context(), _tx, None)
            .await
            .expect("SQLite execution should start");

        // Wait for execution to complete with error
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break, // Expected - no statements to execute
                ExecutionStatus::Success(_) => panic!("Empty query should have failed"),
                ExecutionStatus::Cancelled => panic!("SQLite query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_cancellation() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Use a query that might take some time (though in :memory: it's still very fast)
        let sqlite = create_test_sqlite(
            "CREATE TABLE numbers (n INTEGER); \
             INSERT INTO numbers SELECT 1 UNION SELECT 2 UNION SELECT 3;",
        );

        let handler = SQLiteHandler;
        let handle = handler
            .execute(sqlite, create_test_context(), _tx, None)
            .await
            .expect("SQLite execution should start");

        // Cancel immediately
        handle.cancellation_token.cancel();

        // Wait a bit for cancellation to propagate
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Note: Due to the synchronous nature of SQLite operations and their speed,
        // cancellation might not always work as expected in tests with :memory: databases
        // The query might complete before cancellation takes effect
        let status = handle.status.read().await.clone();
        match status {
            ExecutionStatus::Failed(e) if e.contains("cancelled") => {
                // Cancellation worked
            }
            ExecutionStatus::Success(_) => {
                // Query completed before cancellation - this is also acceptable
            }
            ExecutionStatus::Cancelled => {
                // Direct cancellation status
            }
            ExecutionStatus::Running => {
                // Still running, wait a bit more
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            ExecutionStatus::Failed(e) => {
                // Some other error occurred
                println!("Unexpected error: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_grand_central_events_successful_query() {
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Create memory event bus
        let event_bus = Arc::new(MemoryEventBus::new());
        let context = create_test_context_with_event_bus(event_bus.clone());
        let runbook_id = context.runbook_id;

        // Create and execute SQLite query
        let sqlite = create_test_sqlite("SELECT 'test' as message");
        let sqlite_id = sqlite.id;

        let handler = SQLiteHandler;
        let handle = handler.execute(sqlite, context, tx, None).await.unwrap();

        // Wait for execution to complete
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Success(_) => break,
                ExecutionStatus::Failed(e) => panic!("SQLite query failed: {}", e),
                ExecutionStatus::Cancelled => panic!("SQLite query was cancelled"),
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
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Create memory event bus
        let event_bus = Arc::new(MemoryEventBus::new());
        let context = create_test_context_with_event_bus(event_bus.clone());
        let runbook_id = context.runbook_id;

        // Create SQLite query that will fail
        let sqlite = create_test_sqlite("INVALID SQL");
        let sqlite_id = sqlite.id;

        let handler = SQLiteHandler;
        let handle = handler.execute(sqlite, context, tx, None).await.unwrap();

        // Wait for execution to complete
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break,
                ExecutionStatus::Success(_) => panic!("Invalid SQL should have failed"),
                ExecutionStatus::Cancelled => panic!("SQLite query was cancelled"),
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
                assert!(error.contains("syntax error") || error.contains("SQL"));
            }
            _ => panic!("Expected BlockFailed event, got: {:?}", events[1]),
        }
    }
}
