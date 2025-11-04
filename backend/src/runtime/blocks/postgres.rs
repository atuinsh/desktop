use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgConnectOptions, Column, PgPool, Row, TypeInfo};
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
        &self,
        pool: &PgPool,
        statement: &str,
        context: &ExecutionContext,
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
            let result_json = json!({
                "columns": column_names,
                "rows": results,
                "rowCount": results.len()
            });

            let _ = context
                .send_output(
                    BlockOutput::builder()
                        .block_id(self.id)
                        .object(result_json)
                        .build(),
                )
                .await;
        } else {
            // Handle non-SELECT statement (INSERT, UPDATE, DELETE, CREATE, etc.)
            let result = sqlx::query(statement)
                .execute(pool)
                .await
                .map_err(|e| format!("SQL execution failed: {}", e))?;

            // Send execution result as structured JSON object
            let result_json = json!({
                "rowsAffected": result.rows_affected(),
            });

            let _ = context
                .send_output(
                    BlockOutput::builder()
                        .block_id(self.id)
                        .object(result_json)
                        .build(),
                )
                .await;
        }

        Ok(())
    }

    async fn run_postgres_query(
        &self,
        context: ExecutionContext,
        cancellation_token: CancellationToken,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let block_id = self.id;

        // Send start event
        let _ = context.emit_workflow_event(WorkflowEvent::BlockStarted { id: block_id });

        // Send started lifecycle event to output channel
        let _ = context
            .send_output(
                BlockOutput::builder()
                    .block_id(self.id)
                    .lifecycle(BlockLifecycleEvent::Started)
                    .build(),
            )
            .await;

        // Template the query using context resolver
        let query = self.template_postgres_query(&context).unwrap_or_else(|e| {
            eprintln!("Template error in Postgres query {}: {}", block_id, e);
            self.query.clone() // Fallback to original query
        });

        // Validate URI format
        if let Err(e) = Self::validate_postgres_uri(&self.uri) {
            // Send error lifecycle event
            let _ = context
                .send_output(
                    BlockOutput::builder()
                        .block_id(self.id)
                        .stderr(e.clone())
                        .lifecycle(BlockLifecycleEvent::Error(BlockErrorData {
                            message: e.clone(),
                        }))
                        .build(),
                )
                .await;
            return Err(e.into());
        }

        // Send connecting status
        let _ = context
            .send_output(
                BlockOutput::builder()
                    .block_id(self.id)
                    .stdout("Connecting to database...".to_string())
                    .build(),
            )
            .await;

        // Create Postgres connection pool with reliable timeout using tokio::select!
        let pool = {
            let connection_task = async {
                let opts = PgConnectOptions::from_str(&self.uri)?;
                PgPool::connect_with(opts).await
            };

            let timeout_task = tokio::time::sleep(Duration::from_secs(10));

            tokio::select! {
                result = connection_task => {
                    match result {
                        Ok(pool) => {
                            // Send successful connection status
                                let _ = context.send_output(
                                    BlockOutput::builder()
                                        .block_id(self.id)
                                        .stdout("Connected to database successfully".to_string())
                                        .build(),
                                ).await;
                            pool
                        },
                        Err(e) => {
                            let error_msg = format!("Failed to connect to database: {}", e);
                                let _ = context.send_output(
                                    BlockOutput::builder()
                                        .block_id(self.id)
                                        .stderr(error_msg.clone())
                                        .lifecycle(BlockLifecycleEvent::Error(BlockErrorData {
                                            message: error_msg.clone(),
                                        }))
                                        .build(),
                                ).await;
                            return Err(error_msg.into());
                        }
                    }
                }
                _ = timeout_task => {
                    let error_msg = "Database connection timed out after 10 seconds. Please check your connection string and network.";
                        let _ = context.send_output(
                            BlockOutput::builder()
                                .block_id(self.id)
                                .stderr(error_msg.to_string())
                                .lifecycle(BlockLifecycleEvent::Error(BlockErrorData {
                                    message: error_msg.to_string(),
                                }))
                                .build(),
                        ).await;
                    return Err(error_msg.into());
                }
            }
        };

        let query_clone = query.clone();
        let context_clone = context.clone();
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
                let _ = context_clone
                    .send_output(
                        BlockOutput::builder()
                            .block_id(self.id)
                            .stderr(error_msg.to_string())
                            .lifecycle(BlockLifecycleEvent::Error(BlockErrorData {
                                message: error_msg.to_string(),
                            }))
                            .build(),
                    )
                    .await;
                return Err(error_msg.into());
            }

            // Send executing status
            let _ = context_clone
                .send_output(
                    BlockOutput::builder()
                        .block_id(self.id)
                        .stdout(format!(
                            "Executing {} SQL statement(s)...",
                            statements.len()
                        ))
                        .build(),
                )
                .await;

            // Execute each statement
            for (i, statement) in statements.iter().enumerate() {
                if let Err(e) = self
                    .execute_statement(&pool_clone, statement, &context_clone)
                    .await
                {
                    let error_msg = format!("Statement {} failed: {}", i + 1, e);
                    let _ = context_clone
                        .send_output(
                            BlockOutput::builder()
                                .block_id(self.id)
                                .stderr(error_msg.clone())
                                .lifecycle(BlockLifecycleEvent::Error(BlockErrorData {
                                    message: error_msg.clone(),
                                }))
                                .build(),
                        )
                        .await;
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
                    if let Some(event_bus) = &context.gc_event_bus {
                        let _ = event_bus.emit(GCEvent::BlockCancelled {
                            block_id: self.id,
                            runbook_id: context.runbook_id,
                        }).await;
                    }

                    // Send completion events
                    let _ = context.emit_workflow_event(WorkflowEvent::BlockFinished { id: block_id });
                        let _ = context.send_output(
                            BlockOutput::builder()
                                .block_id(self.id)
                                .lifecycle(BlockLifecycleEvent::Cancelled)
                                .build(),
                        ).await;
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
        let _ = context.emit_workflow_event(WorkflowEvent::BlockFinished { id: block_id });
        // Send success message
        let _ = context
            .send_output(
                BlockOutput::builder()
                    .block_id(self.id)
                    .stdout("Query execution completed successfully".to_string())
                    .build(),
            )
            .await;

        // Send finished lifecycle event
        let _ = context
            .send_output(
                BlockOutput::builder()
                    .block_id(self.id)
                    .lifecycle(BlockLifecycleEvent::Finished(BlockFinishedData {
                        exit_code: Some(0),
                        success: true,
                    }))
                    .build(),
            )
            .await;

        result
    }
}

#[async_trait::async_trait]
impl BlockBehavior for Postgres {
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

        let handle_clone = handle.clone();
        let context_clone = context.clone();
        let block_id = self.id;
        let runbook_id = context.runbook_id;

        tokio::spawn(async move {
            // Emit BlockStarted event via Grand Central
            if let Some(event_bus) = &context_clone.gc_event_bus {
                let _ = event_bus
                    .emit(GCEvent::BlockStarted {
                        block_id: self.id,
                        runbook_id,
                    })
                    .await;
            }

            let result = self
                .run_postgres_query(
                    context_clone.clone(),
                    handle_clone.cancellation_token.clone(),
                )
                .await;

            // Determine status based on result
            let status = match result {
                Ok(_) => {
                    // Emit BlockFinished event via Grand Central
                    if let Some(event_bus) = &context_clone.gc_event_bus {
                        let _ = event_bus
                            .emit(GCEvent::BlockFinished {
                                block_id: self.id,
                                runbook_id,
                                success: true,
                            })
                            .await;
                    }

                    // Store execution output in context
                    let _ = context_clone
                        .document_handle
                        .update_passive_context(block_id, move |ctx| {
                            ctx.insert(BlockExecutionOutput {
                                exit_code: Some(0),
                                stdout: Some("Query execution completed successfully".to_string()),
                                stderr: None,
                            });
                        })
                        .await;

                    ExecutionStatus::Success("Postgres query completed successfully".to_string())
                }
                Err(e) => {
                    // Emit BlockFailed event via Grand Central
                    if let Some(event_bus) = &context_clone.gc_event_bus {
                        let _ = event_bus
                            .emit(GCEvent::BlockFailed {
                                block_id: self.id,
                                runbook_id,
                                error: e.to_string(),
                            })
                            .await;
                    }

                    // Send error lifecycle event to output channel
                    let _ = context
                        .send_output(
                            BlockOutput::builder()
                                .block_id(self.id)
                                .stderr(e.to_string())
                                .lifecycle(BlockLifecycleEvent::Error(BlockErrorData {
                                    message: e.to_string(),
                                }))
                                .build(),
                        )
                        .await;

                    // Store execution output in context
                    let error_msg = e.to_string();
                    let _ = context_clone
                        .document_handle
                        .update_passive_context(block_id, move |ctx| {
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
