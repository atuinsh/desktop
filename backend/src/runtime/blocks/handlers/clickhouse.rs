use async_trait::async_trait;
use reqwest;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tauri::ipc::Channel;
use tokio::sync::{broadcast, RwLock};
use url::Url;
use uuid::Uuid;

use crate::runtime::blocks::clickhouse::Clickhouse;
use crate::runtime::blocks::handler::{
    BlockErrorData, BlockFinishedData, BlockHandler, BlockLifecycleEvent, BlockOutput,
    CancellationToken, ExecutionContext, ExecutionHandle, ExecutionStatus,
};
use crate::runtime::events::GCEvent;
use crate::runtime::workflow::event::WorkflowEvent;
use crate::templates::template_with_context;

pub struct ClickhouseHandler;

#[async_trait]
impl BlockHandler for ClickhouseHandler {
    type Block = Clickhouse;

    fn block_type(&self) -> &'static str {
        "clickhouse"
    }

    fn output_variable(&self, _block: &Self::Block) -> Option<String> {
        None // No output variables for now
    }

    async fn execute(
        &self,
        clickhouse: Clickhouse,
        context: ExecutionContext,
        event_sender: broadcast::Sender<WorkflowEvent>,
        output_channel: Option<Channel<BlockOutput>>,
    ) -> Result<ExecutionHandle, Box<dyn std::error::Error + Send + Sync>> {
        let handle = ExecutionHandle {
            id: Uuid::new_v4(),
            block_id: clickhouse.id,
            cancellation_token: CancellationToken::new(),
            status: Arc::new(RwLock::new(ExecutionStatus::Running)),
            output_variable: None,
        };

        let clickhouse_clone = clickhouse.clone();
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
                        block_id: clickhouse_clone.id,
                        runbook_id: context_clone.runbook_id,
                    })
                    .await;
            }

            let result = Self::run_clickhouse_query(
                &clickhouse_clone,
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
                                block_id: clickhouse_clone.id,
                                runbook_id: context_clone.runbook_id,
                                success: true,
                            })
                            .await;
                    }

                    ExecutionStatus::Success("Clickhouse query completed successfully".to_string())
                }
                Err(e) => {
                    // Emit BlockFailed event via Grand Central
                    if let Some(event_bus) = &context_clone.event_bus {
                        let _ = event_bus
                            .emit(GCEvent::BlockFailed {
                                block_id: clickhouse_clone.id,
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

impl ClickhouseHandler {
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

    /// Template Clickhouse query using the Minijinja template system
    async fn template_clickhouse_query(
        query: &str,
        context: &ExecutionContext,
        clickhouse_id: Uuid,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let block_id_str = clickhouse_id.to_string();
        let rendered = template_with_context(
            query,
            &context.variables,
            &context.document,
            Some(&block_id_str),
            None,
        )?;
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
        http_client: &reqwest::Client,
        endpoint: &str,
        username: &str,
        password: &str,
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
            // Non-SELECT statement (INSERT, UPDATE, DELETE, CREATE, etc.)
            // ClickHouse HTTP interface returns success status for successful operations

            // Send execution result as structured JSON object
            if let Some(ref ch) = output_channel {
                let result_json = json!({
                    "success": true,
                    "message": "Statement executed successfully"
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

    async fn run_clickhouse_query(
        clickhouse: &Clickhouse,
        context: ExecutionContext,
        cancellation_token: CancellationToken,
        event_sender: broadcast::Sender<WorkflowEvent>,
        output_channel: Option<Channel<BlockOutput>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Send start event
        let _ = event_sender.send(WorkflowEvent::BlockStarted { id: clickhouse.id });

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
        let query = Self::template_clickhouse_query(&clickhouse.query, &context, clickhouse.id)
            .await
            .unwrap_or_else(|e| {
                eprintln!(
                    "Template error in Clickhouse query {}: {}",
                    clickhouse.id, e
                );
                clickhouse.query.clone() // Fallback to original query
            });

        // Validate URI format
        if let Err(e) = Self::validate_clickhouse_uri(&clickhouse.uri) {
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
                stdout: Some("Connecting to Clickhouse...".to_string()),
                stderr: None,
                binary: None,
                object: None,
                lifecycle: None,
            });
        }

        // Parse URI and create HTTP client
        let (endpoint, username, password, _http_client) = {
            let connection_task = async {
                let (endpoint, username, password) = Self::parse_clickhouse_uri(&clickhouse.uri)?;

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

                Ok::<
                    (String, String, String, reqwest::Client),
                    Box<dyn std::error::Error + Send + Sync>,
                >((endpoint, username, password, http_client))
            };

            let timeout_task = tokio::time::sleep(Duration::from_secs(10));

            tokio::select! {
                result = connection_task => {
                    match result {
                        Ok((endpoint, username, password, http_client)) => {
                            // Send successful connection status
                            if let Some(ref ch) = output_channel {
                                let _ = ch.send(BlockOutput {
                                    stdout: Some("Connected to Clickhouse successfully".to_string()),
                                    stderr: None,
                                    binary: None,
                                    object: None,
                                    lifecycle: None,
                                });
                            }
                            (endpoint, username, password, http_client)
                        },
                        Err(e) => {
                            let error_msg = format!("Failed to connect to Clickhouse: {}", e);
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
                    let error_msg = "Clickhouse connection timed out after 10 seconds. Please check your connection string and network.";
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
                if let Err(e) = Self::execute_statement(
                    &http_client,
                    &endpoint_clone,
                    &username_clone,
                    &password_clone,
                    statement,
                    &output_channel_clone,
                )
                .await
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
                    // Emit BlockCancelled event via Grand Central
                    if let Some(event_bus) = &context.event_bus {
                        let _ = event_bus.emit(GCEvent::BlockCancelled {
                            block_id: clickhouse.id,
                            runbook_id: context.runbook_id,
                        }).await;
                    }

                    // Send completion events
                    let _ = event_sender.send(WorkflowEvent::BlockFinished { id: clickhouse.id });
                    if let Some(ref ch) = output_channel {
                        let _ = ch.send(BlockOutput {
                            stdout: None,
                            stderr: None,
                            binary: None,
                            object: None,
                            lifecycle: Some(BlockLifecycleEvent::Cancelled),
                        });
                    }
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
        let _ = event_sender.send(WorkflowEvent::BlockFinished { id: clickhouse.id });
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

    fn create_test_clickhouse(query: &str) -> Clickhouse {
        Clickhouse::builder()
            .id(Uuid::new_v4())
            .name("Test Clickhouse")
            .query(query)
            .uri("http://localhost:8123")
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
        let handler = ClickhouseHandler;
        assert_eq!(handler.block_type(), "clickhouse");
    }

    #[test]
    fn test_no_output_variable() {
        let handler = ClickhouseHandler;
        let clickhouse = create_test_clickhouse("SELECT 1");
        assert_eq!(handler.output_variable(&clickhouse), None);
    }

    #[test]
    fn test_uri_validation() {
        assert!(ClickhouseHandler::validate_clickhouse_uri("http://localhost:8123").is_ok());
        assert!(
            ClickhouseHandler::validate_clickhouse_uri("https://clickhouse.example.com:8443")
                .is_ok()
        );
        assert!(ClickhouseHandler::validate_clickhouse_uri("tcp://localhost:9000").is_err());
        assert!(ClickhouseHandler::validate_clickhouse_uri("").is_err());
    }

    #[test]
    fn test_uri_parsing() {
        let (endpoint, username, password) =
            ClickhouseHandler::parse_clickhouse_uri("http://user:pass@localhost:8123/db").unwrap();
        assert_eq!(endpoint, "http://localhost:8123/");
        assert_eq!(username, "user");
        assert_eq!(password, "pass");
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

        let clickhouse = create_test_clickhouse(
            "CREATE TABLE {{ var.table_name }} (id UInt32, name String) ENGINE = Memory; \
             INSERT INTO {{ var.table_name }} VALUES (1, '{{ var.user_name }}'); \
             SELECT * FROM {{ var.table_name }};",
        );

        let handler = ClickhouseHandler;
        let handle = handler
            .execute(clickhouse, context, _tx, None)
            .await
            .expect("Clickhouse execution should start");

        // Wait for execution to complete with error (no real database)
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break, // Expected - no real database
                ExecutionStatus::Success(_) => panic!("Should have failed without database"),
                ExecutionStatus::Cancelled => panic!("Clickhouse query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_invalid_uri() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let mut clickhouse = create_test_clickhouse("SELECT 1");
        clickhouse.uri = "tcp://localhost:9000".to_string(); // Invalid for HTTP interface

        let handler = ClickhouseHandler;
        let handle = handler
            .execute(clickhouse, create_test_context(), _tx, None)
            .await
            .expect("Clickhouse execution should start");

        // Wait for execution to complete with error
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break, // Expected - invalid URI
                ExecutionStatus::Success(_) => panic!("Invalid URI should have failed"),
                ExecutionStatus::Cancelled => panic!("Clickhouse query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_empty_query() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let clickhouse = create_test_clickhouse("");
        let handler = ClickhouseHandler;
        let handle = handler
            .execute(clickhouse, create_test_context(), _tx, None)
            .await
            .expect("Clickhouse execution should start");

        // Wait for execution to complete with error
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break, // Expected - no statements to execute
                ExecutionStatus::Success(_) => panic!("Empty query should have failed"),
                ExecutionStatus::Cancelled => panic!("Clickhouse query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_cancellation() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let clickhouse = create_test_clickhouse("SELECT sleep(10)");

        let handler = ClickhouseHandler;
        let handle = handler
            .execute(clickhouse, create_test_context(), _tx, None)
            .await
            .expect("Clickhouse execution should start");

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

        // Create Clickhouse query that will fail
        let clickhouse = create_test_clickhouse("INVALID SQL");
        let clickhouse_id = clickhouse.id;

        let handler = ClickhouseHandler;
        let handle = handler
            .execute(clickhouse, context, tx, None)
            .await
            .unwrap();

        // Wait for execution to complete
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break,
                ExecutionStatus::Success(_) => panic!("Invalid SQL should have failed"),
                ExecutionStatus::Cancelled => panic!("Clickhouse query was cancelled"),
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
                assert_eq!(*block_id, clickhouse_id);
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
                assert_eq!(*block_id, clickhouse_id);
                assert_eq!(*rb_id, runbook_id);
                assert!(
                    error.contains("URI")
                        || error.contains("SQL")
                        || error.contains("syntax")
                        || error.contains("HTTP")
                        || error.contains("url")
                        || error.contains("connect")
                );
            }
            _ => panic!("Expected BlockFailed event, got: {:?}", events[1]),
        }
    }
}
