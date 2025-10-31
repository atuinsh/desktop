use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use typed_builder::TypedBuilder;
use url::Url;
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

impl Clickhouse {
    /// Validate Clickhouse URI format and connection parameters
    fn validate_clickhouse_uri(uri: &str) -> Result<(), String> {
        if uri.is_empty() {
            return Err("Clickhouse URI cannot be empty".to_string());
        }

        // For HTTP interface, we need http:// or https://
        if !uri.starts_with("http://") && !uri.starts_with("https://") {
            return Err(
                "Invalid Clickhouse URI format. Must start with 'http://' or 'https://' for HTTP interface".to_string()
            );
        }

        Ok(())
    }

    /// Template Clickhouse query using the context resolver
    fn template_clickhouse_query(
        &self,
        context: &ExecutionContext,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let rendered = context.context_resolver.resolve_template(&self.query)?;
        Ok(rendered)
    }

    /// Parse ClickHouse URI and extract HTTP endpoint and credentials
    fn parse_clickhouse_uri(
        uri: &str,
    ) -> Result<(String, String, String), Box<dyn std::error::Error + Send + Sync>> {
        let url = Url::parse(uri)?;
        let username = url.username();
        let password = url.password().unwrap_or("");

        // Build HTTP endpoint (remove any path, just use root)
        let http_endpoint = format!(
            "{}://{}:{}/",
            url.scheme(),
            url.host_str().unwrap_or("localhost"),
            url.port().unwrap_or(8123)
        );

        Ok((http_endpoint, username.to_string(), password.to_string()))
    }

    /// Execute a single Clickhouse statement via HTTP
    async fn execute_statement(
        &self,
        http_client: &reqwest::Client,
        endpoint: &str,
        username: &str,
        password: &str,
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

        let is_select = first_word == "select" || first_word == "with";

        // Add FORMAT JSONEachRow if it's a SELECT and doesn't already have a format
        let query_to_execute = if is_select && !statement.to_uppercase().contains("FORMAT") {
            format!("{} FORMAT JSONEachRow", statement)
        } else {
            statement.to_string()
        };

        // Make HTTP request to ClickHouse
        let mut request = http_client.post(endpoint).body(query_to_execute);

        // Add authentication if provided
        if !username.is_empty() {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await?;

        // Check for HTTP errors
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            return Err(format!("ClickHouse HTTP error ({}): {}", status, error_text).into());
        }

        // Parse response
        let response_text = response.text().await?;

        if is_select {
            // Parse JSONEachRow format (newline-delimited JSON objects)
            let mut results = Vec::new();
            let mut column_names = Vec::new();

            for line in response_text.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                // Parse each line as a JSON object
                match serde_json::from_str::<Value>(line) {
                    Ok(row) => {
                        // Extract column names from first row
                        if column_names.is_empty() {
                            if let Value::Object(map) = &row {
                                column_names = map.keys().cloned().collect();
                                column_names.sort(); // Ensure consistent ordering
                            }
                        }
                        results.push(row);
                    }
                    Err(e) => {
                        return Err(format!(
                            "Failed to parse JSON response: {} (line: {})",
                            e, line
                        )
                        .into());
                    }
                }
            }

            // Send results as structured JSON object
            let result_json = json!({
                "columns": column_names,
                "rows": results,
                "rowCount": results.len()
            });

            let _ = context
                .send_output(
                    BlockOutput {
                        block_id: self.id,
                        stdout: None,
                        stderr: None,
                        lifecycle: None,
                        binary: None,
                        object: Some(result_json),
                    }
                    .into(),
                )
                .await;
        } else {
            // Non-SELECT statement (INSERT, UPDATE, DELETE, CREATE, etc.)
            // ClickHouse HTTP interface returns success status for successful operations

            // Send execution result as structured JSON object
            let result_json = json!({
                "success": true,
                "message": "Statement executed successfully"
            });

            let _ = context
                .send_output(
                    BlockOutput {
                        block_id: self.id,
                        stdout: None,
                        stderr: None,
                        lifecycle: None,
                        binary: None,
                        object: Some(result_json),
                    }
                    .into(),
                )
                .await;
        }

        Ok(())
    }

    async fn run_clickhouse_query(
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
                BlockOutput {
                    block_id: self.id,
                    stdout: None,
                    stderr: None,
                    binary: None,
                    object: None,
                    lifecycle: Some(BlockLifecycleEvent::Started),
                }
                .into(),
            )
            .await;

        // Template the query using context resolver
        let query = self
            .template_clickhouse_query(&context)
            .unwrap_or_else(|e| {
                eprintln!("Template error in Clickhouse query {}: {}", block_id, e);
                self.query.clone() // Fallback to original query
            });

        // Validate URI format
        if let Err(e) = Self::validate_clickhouse_uri(&self.uri) {
            // Send error lifecycle event
            let _ = context
                .send_output(
                    BlockOutput {
                        block_id: self.id,
                        stdout: None,
                        stderr: Some(e.clone()),
                        binary: None,
                        object: None,
                        lifecycle: Some(BlockLifecycleEvent::Error(BlockErrorData {
                            message: e.clone(),
                        })),
                    }
                    .into(),
                )
                .await;
            return Err(e.into());
        }

        // Send connecting status
        let _ = context
            .send_output(
                BlockOutput {
                    block_id: self.id,
                    stdout: Some("Connecting to Clickhouse...".to_string()),
                    stderr: None,
                    binary: None,
                    object: None,
                    lifecycle: None,
                }
                .into(),
            )
            .await?;

        // Parse URI and create HTTP client
        let (endpoint, username, password) = {
            let connection_task = async {
                let (endpoint, username, password) = Self::parse_clickhouse_uri(&self.uri)?;

                // Create HTTP client with timeout
                let http_client = reqwest::Client::builder()
                    .timeout(Duration::from_secs(30))
                    .build()?;

                // Test connection with simple query
                let mut test_request = http_client
                    .post(&endpoint)
                    .body("SELECT 1 FORMAT JSONEachRow");

                if !username.is_empty() {
                    test_request = test_request.basic_auth(&username, Some(&password));
                }

                let response = test_request.send().await?;

                if !response.status().is_success() {
                    let error = response.text().await?;
                    return Err(format!("Connection test failed: {}", error).into());
                }

                Ok::<(String, String, String), Box<dyn std::error::Error + Send + Sync>>((
                    endpoint, username, password,
                ))
            };

            let timeout_task = tokio::time::sleep(Duration::from_secs(10));

            tokio::select! {
                result = connection_task => {
                    match result {
                        Ok((endpoint, username, password)) => {
                            // Send successful connection status
                            let _ = context.send_output(BlockOutput {
                                block_id: self.id,
                                stdout: Some("Connected to Clickhouse successfully".to_string()),
                                stderr: None,
                                binary: None,
                                object: None,
                                lifecycle: None,
                            }.into()).await;
                            (endpoint, username, password)
                        },
                        Err(e) => {
                            let error_msg = format!("Failed to connect to Clickhouse: {}", e);
                                let _ = context.send_output(BlockOutput {
                                        block_id: self.id,
                                    stdout: None,
                                    stderr: Some(error_msg.clone()),
                                    binary: None,
                                    object: None,
                                    lifecycle: Some(BlockLifecycleEvent::Error(BlockErrorData {
                                        message: error_msg.clone(),
                                    })),
                                }.into()).await;
                            return Err(error_msg.into());
                        }
                    }
                }
                _ = timeout_task => {
                    let error_msg = "Clickhouse connection timed out after 10 seconds. Please check your connection string and network.";
                        let _ = context.send_output(BlockOutput {
                                block_id: self.id,
                            stdout: None,
                            stderr: Some(error_msg.to_string()),
                            binary: None,
                            object: None,
                            lifecycle: Some(BlockLifecycleEvent::Error(BlockErrorData {
                                message: error_msg.to_string(),
                            })),
                        }.into()
                        ).await;
                    return Err(error_msg.into());
                }
            }
        };

        let query_clone = query.clone();
        let context_clone = context.clone();
        let cancellation_receiver = cancellation_token.take_receiver();
        let endpoint_clone = endpoint.clone();
        let username_clone = username.clone();
        let password_clone = password.clone();

        let execution_task = async move {
            // Create HTTP client for query execution
            let http_client = reqwest::Client::builder()
                .timeout(Duration::from_secs(60)) // Longer timeout for queries
                .build()?;

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
                        BlockOutput {
                            block_id: self.id,
                            stdout: None,
                            stderr: Some(error_msg.to_string()),
                            binary: None,
                            object: None,
                            lifecycle: Some(BlockLifecycleEvent::Error(BlockErrorData {
                                message: error_msg.to_string(),
                            })),
                        }
                        .into(),
                    )
                    .await;
                return Err(error_msg.into());
            }

            // Send executing status
            let _ = context_clone
                .send_output(
                    BlockOutput {
                        block_id: self.id,
                        stdout: Some(format!(
                            "Executing {} SQL statement(s)...",
                            statements.len()
                        )),
                        stderr: None,
                        binary: None,
                        object: None,
                        lifecycle: None,
                    }
                    .into(),
                )
                .await;

            // Execute each statement
            for (i, statement) in statements.iter().enumerate() {
                if let Err(e) = self
                    .execute_statement(
                        &http_client,
                        &endpoint_clone,
                        &username_clone,
                        &password_clone,
                        statement,
                        &context_clone,
                    )
                    .await
                {
                    let error_msg = format!("Statement {} failed: {}", i + 1, e);
                    let _ = context_clone
                        .send_output(
                            BlockOutput {
                                block_id: self.id,
                                stdout: None,
                                stderr: Some(error_msg.clone()),
                                binary: None,
                                object: None,
                                lifecycle: Some(BlockLifecycleEvent::Error(BlockErrorData {
                                    message: error_msg.clone(),
                                })),
                            }
                            .into(),
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
                    // Emit BlockCancelled event via Grand Central
                    if let Some(event_bus) = &context.gc_event_bus {
                        let _ = event_bus.emit(GCEvent::BlockCancelled {
                            block_id: self.id,
                            runbook_id: context.runbook_id,
                        }).await;
                    }

                    // Send completion events
                    let _ = context.emit_workflow_event(WorkflowEvent::BlockFinished { id: block_id });
                        let _ = context.send_output(BlockOutput {
                            block_id: self.id,
                                stdout: None,
                            stderr: None,
                            binary: None,
                            object: None,
                            lifecycle: Some(BlockLifecycleEvent::Cancelled),
                        }.into(),
                ).await;
                    return Err("Clickhouse query execution cancelled".into());
                }
                result = execution_task => {
                    result
                }
            }
        } else {
            execution_task.await
        };

        // Send completion events
        let _ = context.emit_workflow_event(WorkflowEvent::BlockFinished { id: block_id });
        // Send success message
        let _ = context.send_output(
            BlockOutput {
                block_id: self.id,
                stdout: Some("Query execution completed successfully".to_string()),
                stderr: None,
                binary: None,
                object: None,
                lifecycle: None,
            }
            .into(),
        );

        // Send finished lifecycle event
        let _ = context.send_output(
            BlockOutput {
                block_id: self.id,
                stdout: None,
                stderr: None,
                binary: None,
                object: None,
                lifecycle: Some(BlockLifecycleEvent::Finished(BlockFinishedData {
                    exit_code: Some(0),
                    success: true,
                })),
            }
            .into(),
        );

        result
    }
}

#[async_trait::async_trait]
impl BlockBehavior for Clickhouse {
    fn into_block(self) -> Block {
        Block::Clickhouse(self)
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
                .run_clickhouse_query(
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

                    ExecutionStatus::Success("Clickhouse query completed successfully".to_string())
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
                    let _ = context_clone
                        .send_output(
                            BlockOutput {
                                block_id: self.id,
                                stdout: None,
                                stderr: Some(e.to_string()),
                                binary: None,
                                object: None,
                                lifecycle: Some(BlockLifecycleEvent::Error(BlockErrorData {
                                    message: e.to_string(),
                                })),
                            }
                            .into(),
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
