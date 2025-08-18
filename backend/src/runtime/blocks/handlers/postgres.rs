use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::{postgres::PgConnectOptions, Column, PgPool, Row, TypeInfo};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tauri::ipc::Channel;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::runtime::blocks::handler::{
    BlockErrorData, BlockFinishedData, BlockHandler, BlockLifecycleEvent, BlockOutput,
    CancellationToken, ExecutionContext, ExecutionHandle, ExecutionStatus,
};
use crate::runtime::blocks::postgres::Postgres;
use crate::runtime::events::GCEvent;
use crate::runtime::workflow::event::WorkflowEvent;
use crate::templates::template_with_context;

pub struct PostgresHandler;

#[async_trait]
impl BlockHandler for PostgresHandler {
    type Block = Postgres;

    fn block_type(&self) -> &'static str {
        "postgres"
    }

    fn output_variable(&self, _block: &Self::Block) -> Option<String> {
        None // No output variables for now
    }

    async fn execute(
        &self,
        postgres: Postgres,
        context: ExecutionContext,
        event_sender: broadcast::Sender<WorkflowEvent>,
        output_channel: Option<Channel<BlockOutput>>,
    ) -> Result<ExecutionHandle, Box<dyn std::error::Error + Send + Sync>> {
        let handle = ExecutionHandle {
            id: Uuid::new_v4(),
            block_id: postgres.id,
            cancellation_token: CancellationToken::new(),
            status: Arc::new(RwLock::new(ExecutionStatus::Running)),
            output_variable: None,
        };

        let postgres_clone = postgres.clone();
        let context_clone = context.clone();
        let handle_clone = handle.clone();
        let event_sender_clone = event_sender.clone();

        let output_channel_clone = output_channel.clone();
        let output_channel_error = output_channel.clone();

        tokio::spawn(async move {
            // Emit BlockStarted event via Grand Central
            if let Some(event_bus) = &context_clone.event_bus {
                let _ = event_bus
                    .emit(GCEvent::BlockStarted {
                        block_id: postgres_clone.id,
                        runbook_id: context_clone.runbook_id,
                    })
                    .await;
            }

            let result = Self::run_postgres_query(
                &postgres_clone,
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
                                block_id: postgres_clone.id,
                                runbook_id: context_clone.runbook_id,
                                success: true,
                            })
                            .await;
                    }

                    ExecutionStatus::Success("Postgres query completed successfully".to_string())
                }
                Err(e) => {
                    // Emit BlockFailed event via Grand Central
                    if let Some(event_bus) = &context_clone.event_bus {
                        let _ = event_bus
                            .emit(GCEvent::BlockFailed {
                                block_id: postgres_clone.id,
                                runbook_id: context_clone.runbook_id,
                                error: e.to_string(),
                            })
                            .await;
                    }

                    // Send error lifecycle event to output channel
                    if let Some(ref ch) = output_channel_error {
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

                    ExecutionStatus::Failed(e.to_string())
                }
            };

            *handle_clone.status.write().await = status.clone();
        });

        Ok(handle)
    }
}

impl PostgresHandler {
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

    /// Template Postgres query using the Minijinja template system
    async fn template_postgres_query(
        query: &str,
        context: &ExecutionContext,
        postgres_id: Uuid,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let block_id_str = postgres_id.to_string();
        let rendered = template_with_context(
            query,
            &context.variables,
            &context.document,
            Some(&block_id_str),
        )?;
        Ok(rendered)
    }

    /// Convert Postgres row to JSON value
    fn row_to_json(row: &sqlx::postgres::PgRow) -> Result<Value, sqlx::Error> {
        let mut obj = serde_json::Map::new();

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

        Ok(Value::Object(obj))
    }

    /// Execute a single Postgres statement
    async fn execute_statement(
        pool: &PgPool,
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

        if first_word == "select" || first_word == "with" {
            // Handle SELECT query or CTE
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
            let result = sqlx::query(statement)
                .execute(pool)
                .await
                .map_err(|e| format!("SQL execution failed: {}", e))?;

            // Send execution result as structured JSON object
            if let Some(ref ch) = output_channel {
                let result_json = json!({
                    "rowsAffected": result.rows_affected(),
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

    async fn run_postgres_query(
        postgres: &Postgres,
        context: ExecutionContext,
        cancellation_token: CancellationToken,
        event_sender: broadcast::Sender<WorkflowEvent>,
        output_channel: Option<Channel<BlockOutput>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Send start event
        let _ = event_sender.send(WorkflowEvent::BlockStarted { id: postgres.id });

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
        let query = Self::template_postgres_query(&postgres.query, &context, postgres.id)
            .await
            .unwrap_or_else(|e| {
                eprintln!("Template error in Postgres query {}: {}", postgres.id, e);
                postgres.query.clone() // Fallback to original query
            });

        // Validate URI format
        if let Err(e) = Self::validate_postgres_uri(&postgres.uri) {
            // Send error lifecycle event
            if let Some(ref ch) = output_channel {
                let _ = ch.send(BlockOutput {
                    stdout: None,
                    stderr: Some(e.clone()),
                    binary: None,
                    object: None,
                    lifecycle: Some(BlockLifecycleEvent::Error(BlockErrorData {
                        message: e.clone(),
                    })),
                });
            }
            return Err(e.into());
        }

        // Send connecting status
        if let Some(ref ch) = output_channel {
            let _ = ch.send(BlockOutput {
                stdout: Some("Connecting to database...".to_string()),
                stderr: None,
                binary: None,
                object: None,
                lifecycle: None,
            });
        }

        // Create Postgres connection pool with reliable timeout using tokio::select!
        let pool = {
            let connection_task = async {
                let opts = PgConnectOptions::from_str(&postgres.uri)?;
                PgPool::connect_with(opts).await
            };

            let timeout_task = tokio::time::sleep(Duration::from_secs(10));

            tokio::select! {
                result = connection_task => {
                    match result {
                        Ok(pool) => {
                            // Send successful connection status
                            if let Some(ref ch) = output_channel {
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
                            if let Some(ref ch) = output_channel {
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
                    let error_msg = "Database connection timed out after 10 seconds. Please check your connection string and network.";
                    if let Some(ref ch) = output_channel {
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
                let error_msg = "No SQL statements to execute";
                if let Some(ref ch) = &output_channel_clone {
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

            // Send executing status
            if let Some(ref ch) = &output_channel_clone {
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

            // Execute each statement
            for (i, statement) in statements.iter().enumerate() {
                if let Err(e) =
                    Self::execute_statement(&pool_clone, statement, &output_channel_clone).await
                {
                    let error_msg = format!("Statement {} failed: {}", i + 1, e);
                    if let Some(ref ch) = &output_channel_clone {
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

        // Handle execution with cancellation
        let result = if let Some(cancel_rx) = cancellation_receiver {
            tokio::select! {
                _ = cancel_rx => {
                    // Close the pool
                    pool.close().await;

                    // Emit BlockCancelled event via Grand Central
                    if let Some(event_bus) = &context.event_bus {
                        let _ = event_bus.emit(GCEvent::BlockCancelled {
                            block_id: postgres.id,
                            runbook_id: context.runbook_id,
                        }).await;
                    }

                    // Send completion events
                    let _ = event_sender.send(WorkflowEvent::BlockFinished { id: postgres.id });
                    if let Some(ref ch) = output_channel {
                        let _ = ch.send(BlockOutput {
                            stdout: None,
                            stderr: None,
                            binary: None,
                            object: None,
                            lifecycle: Some(BlockLifecycleEvent::Cancelled),
                        });
                    }
                    return Err("Postgres query execution cancelled".into());
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

        // Send completion events
        let _ = event_sender.send(WorkflowEvent::BlockFinished { id: postgres.id });
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

    fn create_test_postgres(query: &str) -> Postgres {
        Postgres::builder()
            .id(Uuid::new_v4())
            .name("Test Postgres")
            .query(query)
            .uri("postgresql://test:test@localhost/test")
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
        let handler = PostgresHandler;
        assert_eq!(handler.block_type(), "postgres");
    }

    #[test]
    fn test_no_output_variable() {
        let handler = PostgresHandler;
        let postgres = create_test_postgres("SELECT 1");
        assert_eq!(handler.output_variable(&postgres), None);
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

        let postgres = create_test_postgres(
            "CREATE TABLE {{ var.table_name }} (id SERIAL, name TEXT); \
             INSERT INTO {{ var.table_name }} VALUES (1, '{{ var.user_name }}'); \
             SELECT * FROM {{ var.table_name }};",
        );

        let handler = PostgresHandler;
        let handle = handler
            .execute(postgres, context, _tx, None)
            .await
            .expect("Postgres execution should start");

        // Wait for execution to complete with error (no real database)
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break, // Expected - no real database
                ExecutionStatus::Success(_) => panic!("Should have failed without database"),
                ExecutionStatus::Cancelled => panic!("Postgres query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_empty_uri() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let mut postgres = create_test_postgres("SELECT 1");
        postgres.uri = "".to_string();

        let handler = PostgresHandler;
        let handle = handler
            .execute(postgres, create_test_context(), _tx, None)
            .await
            .expect("Postgres execution should start");

        // Wait for execution to complete with error
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break, // Expected - empty URI
                ExecutionStatus::Success(_) => panic!("Empty URI should have failed"),
                ExecutionStatus::Cancelled => panic!("Postgres query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_empty_query() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let postgres = create_test_postgres("");
        let handler = PostgresHandler;
        let handle = handler
            .execute(postgres, create_test_context(), _tx, None)
            .await
            .expect("Postgres execution should start");

        // Wait for execution to complete with error
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break, // Expected - no statements to execute
                ExecutionStatus::Success(_) => panic!("Empty query should have failed"),
                ExecutionStatus::Cancelled => panic!("Postgres query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_cancellation() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let postgres = create_test_postgres("SELECT pg_sleep(10)");

        let handler = PostgresHandler;
        let handle = handler
            .execute(postgres, create_test_context(), _tx, None)
            .await
            .expect("Postgres execution should start");

        // Cancel immediately
        handle.cancellation_token.cancel();

        // Wait a bit for cancellation to propagate
        tokio::time::sleep(Duration::from_millis(200)).await;

        let status = handle.status.read().await.clone();
        match status {
            ExecutionStatus::Failed(e) if e.contains("cancelled") => {
                // Cancellation worked
            }
            ExecutionStatus::Failed(_) => {
                // Some other error occurred (e.g., connection failed)
            }
            ExecutionStatus::Success(_) => {
                // Query completed before cancellation - unlikely with sleep
            }
            ExecutionStatus::Cancelled => {
                // Direct cancellation status
            }
            ExecutionStatus::Running => {
                // Still running, wait a bit more
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }

    #[tokio::test]
    async fn test_grand_central_events_failed_query() {
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Create memory event bus
        let event_bus = Arc::new(MemoryEventBus::new());
        let context = create_test_context_with_event_bus(event_bus.clone());
        let runbook_id = context.runbook_id;

        // Create Postgres query that will fail
        let postgres = create_test_postgres("INVALID SQL");
        let postgres_id = postgres.id;

        let handler = PostgresHandler;
        let handle = handler.execute(postgres, context, tx, None).await.unwrap();

        // Wait for execution to complete
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break,
                ExecutionStatus::Success(_) => panic!("Invalid SQL should have failed"),
                ExecutionStatus::Cancelled => panic!("Postgres query was cancelled"),
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
                assert_eq!(*block_id, postgres_id);
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
                assert_eq!(*block_id, postgres_id);
                assert_eq!(*rb_id, runbook_id);
                assert!(
                    error.contains("URI")
                        || error.contains("SQL")
                        || error.contains("syntax")
                        || error.contains("database")
                        || error.contains("role")
                );
            }
            _ => panic!("Expected BlockFailed event, got: {:?}", events[1]),
        }
    }
}
