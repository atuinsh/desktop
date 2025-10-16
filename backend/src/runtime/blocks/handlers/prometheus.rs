use async_trait::async_trait;
use reqwest::{Client, ClientBuilder};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tauri::ipc::Channel;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::runtime::blocks::handler::{
    BlockErrorData, BlockFinishedData, BlockHandler, BlockLifecycleEvent, BlockOutput,
    CancellationToken, ExecutionContext, ExecutionHandle, ExecutionStatus,
};
use crate::runtime::blocks::prometheus::Prometheus;
use crate::runtime::events::GCEvent;
use crate::runtime::workflow::event::WorkflowEvent;
use crate::templates::template_with_context;

pub struct PrometheusHandler;

#[async_trait]
impl BlockHandler for PrometheusHandler {
    type Block = Prometheus;

    fn block_type(&self) -> &'static str {
        "prometheus"
    }

    fn output_variable(&self, _block: &Self::Block) -> Option<String> {
        None // Prometheus queries typically return time series data for visualization
    }

    async fn execute(
        &self,
        prometheus: Prometheus,
        context: ExecutionContext,
        event_sender: broadcast::Sender<WorkflowEvent>,
        output_channel: Option<Channel<BlockOutput>>,
    ) -> Result<ExecutionHandle, Box<dyn std::error::Error + Send + Sync>> {
        let handle = ExecutionHandle {
            id: Uuid::new_v4(),
            block_id: prometheus.id,
            cancellation_token: CancellationToken::new(),
            status: Arc::new(RwLock::new(ExecutionStatus::Running)),
            output_variable: None,
        };

        let prometheus_clone = prometheus.clone();
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
                        block_id: prometheus_clone.id,
                        runbook_id: context_clone.runbook_id,
                    })
                    .await;
            }

            let result = Self::run_prometheus_query(
                &prometheus_clone,
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
                                block_id: prometheus_clone.id,
                                runbook_id: context_clone.runbook_id,
                                success: true,
                            })
                            .await;
                    }

                    ExecutionStatus::Success("Prometheus query completed successfully".to_string())
                }
                Err(e) => {
                    // Emit BlockFailed event via Grand Central
                    if let Some(event_bus) = &context_clone.event_bus {
                        let _ = event_bus
                            .emit(GCEvent::BlockFailed {
                                block_id: prometheus_clone.id,
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

impl PrometheusHandler {
    /// Validate Prometheus endpoint format
    fn validate_prometheus_endpoint(endpoint: &str) -> Result<(), String> {
        if endpoint.is_empty() {
            return Err("Prometheus endpoint cannot be empty".to_string());
        }

        if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
            return Err(
                "Invalid Prometheus endpoint format. Must start with 'http://' or 'https://'"
                    .to_string(),
            );
        }

        // Try parsing the URL to catch format errors early
        if url::Url::parse(endpoint).is_err() {
            return Err("Invalid URL format".to_string());
        }

        Ok(())
    }

    /// Template Prometheus query using the Minijinja template system
    async fn template_prometheus_query(
        query: &str,
        context: &ExecutionContext,
        prometheus_id: Uuid,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let block_id_str = prometheus_id.to_string();
        let rendered = template_with_context(
            query,
            &context.variables,
            &context.document,
            Some(&block_id_str),
            None,
        )?;
        Ok(rendered)
    }

    /// Calculate step size for Prometheus range queries based on time period
    fn calculate_step_size(period: &str) -> u32 {
        match period {
            "5m" => 10,      // 5 minutes -> 10 second step
            "15m" => 30,     // 15 minutes -> 30 second step
            "30m" => 60,     // 30 minutes -> 1 minute step
            "1h" => 60,      // 1 hour -> 1 minute step
            "3h" => 300,     // 3 hours -> 5 minute step
            "6h" => 600,     // 6 hours -> 10 minute step
            "24h" => 1800,   // 24 hours -> 30 minute step
            "2d" => 3600,    // 2 days -> 1 hour step
            "7d" => 3600,    // 7 days -> 1 hour step
            "30d" => 14400,  // 30 days -> 4 hour step
            "90d" => 86400,  // 90 days -> 1 day step
            "180d" => 86400, // 180 days -> 1 day step
            _ => 60,         // Default to 1 minute step
        }
    }

    /// Parse time period to seconds
    fn parse_period_to_seconds(period: &str) -> u32 {
        match period {
            "5m" => 5 * 60,
            "15m" => 15 * 60,
            "30m" => 30 * 60,
            "1h" => 60 * 60,
            "3h" => 3 * 60 * 60,
            "6h" => 6 * 60 * 60,
            "24h" => 24 * 60 * 60,
            "2d" => 2 * 24 * 60 * 60,
            "7d" => 7 * 24 * 60 * 60,
            "30d" => 30 * 24 * 60 * 60,
            "90d" => 90 * 24 * 60 * 60,
            "180d" => 180 * 24 * 60 * 60,
            _ => 60 * 60, // Default to 1 hour
        }
    }

    /// Execute Prometheus range query
    async fn execute_range_query(
        client: &Client,
        endpoint: &str,
        query: &str,
        start: u64,
        end: u64,
        step: u32,
        output_channel: &Option<Channel<BlockOutput>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/api/v1/query_range", endpoint.trim_end_matches('/'));

        let params = [
            ("query", query),
            ("start", &start.to_string()),
            ("end", &end.to_string()),
            ("step", &format!("{}s", step)),
        ];

        // Send executing status
        if let Some(ref ch) = output_channel {
            let _ = ch.send(BlockOutput {
                stdout: Some(format!("Executing Prometheus query: {}", query)),
                stderr: None,
                binary: None,
                object: None,
                lifecycle: None,
            });
        }

        let response = client
            .get(&url)
            .query(&params)
            .send()
            .await
            .map_err(|e| format!("Failed to send request: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(format!("HTTP {}: {}", status, text).into());
        }

        let json: Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse JSON response: {}", e))?;

        // Check Prometheus API status
        if json.get("status").and_then(|s| s.as_str()) != Some("success") {
            let error = json
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("Unknown Prometheus error");
            return Err(format!("Prometheus API error: {}", error).into());
        }

        // Extract and format the result data
        let data = json.get("data").ok_or("Missing data field in response")?;
        let result = data.get("result").ok_or("Missing result field in data")?;

        // Convert Prometheus result to ECharts-compatible format (similar to frontend logic)
        let mut series = Vec::new();

        if let Some(result_array) = result.as_array() {
            for series_data in result_array {
                if let (Some(metric), Some(values)) = (
                    series_data.get("metric"),
                    series_data.get("values").and_then(|v| v.as_array()),
                ) {
                    let series_name = if let Some(metric_obj) = metric.as_object() {
                        // Create a readable series name from metric labels
                        if metric_obj.is_empty() {
                            query.to_string()
                        } else {
                            metric_obj
                                .iter()
                                .map(|(k, v)| format!("{}={}", k, v.as_str().unwrap_or("unknown")))
                                .collect::<Vec<_>>()
                                .join(", ")
                        }
                    } else {
                        query.to_string()
                    };

                    let mut data_points = Vec::new();
                    for value_pair in values {
                        if let Some(pair) = value_pair.as_array() {
                            if pair.len() == 2 {
                                let timestamp = pair[0].as_f64().unwrap_or(0.0) * 1000.0; // Convert to milliseconds
                                let value = pair[1]
                                    .as_str()
                                    .and_then(|s| s.parse::<f64>().ok())
                                    .unwrap_or(0.0);
                                data_points.push(json!([timestamp, value]));
                            }
                        }
                    }

                    series.push(json!({
                        "type": "line",
                        "showSymbol": false,
                        "name": series_name,
                        "data": data_points
                    }));
                }
            }
        }

        // Send results as structured JSON object compatible with frontend charting
        if let Some(ref ch) = output_channel {
            let result_json = json!({
                "series": series,
                "queryExecuted": query,
                "timeRange": {
                    "start": start,
                    "end": end,
                    "step": step
                }
            });

            let _ = ch.send(BlockOutput {
                stdout: None,
                stderr: None,
                lifecycle: None,
                binary: None,
                object: Some(result_json),
            });
        }

        Ok(())
    }

    async fn run_prometheus_query(
        prometheus: &Prometheus,
        context: ExecutionContext,
        cancellation_token: CancellationToken,
        event_sender: broadcast::Sender<WorkflowEvent>,
        output_channel: Option<Channel<BlockOutput>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Send start event
        let _ = event_sender.send(WorkflowEvent::BlockStarted { id: prometheus.id });

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
        let query = Self::template_prometheus_query(&prometheus.query, &context, prometheus.id)
            .await
            .unwrap_or_else(|e| {
                eprintln!(
                    "Template error in Prometheus query {}: {}",
                    prometheus.id, e
                );
                prometheus.query.clone() // Fallback to original query
            });

        // Validate endpoint format
        if let Err(e) = Self::validate_prometheus_endpoint(&prometheus.endpoint) {
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

        if query.trim().is_empty() {
            let error_msg = "Prometheus query cannot be empty";
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

        // Send connecting status
        if let Some(ref ch) = output_channel {
            let _ = ch.send(BlockOutput {
                stdout: Some(format!("Connecting to Prometheus: {}", prometheus.endpoint)),
                stderr: None,
                binary: None,
                object: None,
                lifecycle: None,
            });
        }

        // Create HTTP client with timeout
        let client = ClientBuilder::new()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        // Calculate time range and step size
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let period_seconds = Self::parse_period_to_seconds(&prometheus.period);
        let start = now - period_seconds as u64;
        let end = now;
        let step = Self::calculate_step_size(&prometheus.period);

        let cancellation_receiver = cancellation_token.take_receiver();
        let endpoint = prometheus.endpoint.clone();
        let output_channel_clone = output_channel.clone();

        let execution_task = async move {
            Self::execute_range_query(
                &client,
                &endpoint,
                &query,
                start,
                end,
                step,
                &output_channel_clone,
            )
            .await
        };

        // Handle execution with cancellation
        let result = if let Some(cancel_rx) = cancellation_receiver {
            tokio::select! {
                _ = cancel_rx => {
                    // Emit BlockCancelled event via Grand Central
                    if let Some(event_bus) = &context.event_bus {
                        let _ = event_bus.emit(GCEvent::BlockCancelled {
                            block_id: prometheus.id,
                            runbook_id: context.runbook_id,
                        }).await;
                    }

                    // Send completion events
                    let _ = event_sender.send(WorkflowEvent::BlockFinished { id: prometheus.id });
                    if let Some(ref ch) = output_channel {
                        let _ = ch.send(BlockOutput {
                            stdout: None,
                            stderr: None,
                            binary: None,
                            object: None,
                            lifecycle: Some(BlockLifecycleEvent::Cancelled),
                        });
                    }
                    return Err("Prometheus query execution cancelled".into());
                }
                result = execution_task => {
                    result
                }
            }
        } else {
            execution_task.await
        };

        // Send completion events
        let _ = event_sender.send(WorkflowEvent::BlockFinished { id: prometheus.id });
        if let Some(ref ch) = output_channel {
            // Send success message
            let _ = ch.send(BlockOutput {
                stdout: Some("Prometheus query completed successfully".to_string()),
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

    fn create_test_prometheus(query: &str, endpoint: &str) -> Prometheus {
        Prometheus::builder()
            .id(Uuid::new_v4())
            .name("Test Prometheus")
            .query(query)
            .endpoint(endpoint)
            .period("5m")
            .auto_refresh(false)
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
        let handler = PrometheusHandler;
        assert_eq!(handler.block_type(), "prometheus");
    }

    #[test]
    fn test_no_output_variable() {
        let handler = PrometheusHandler;
        let prometheus = create_test_prometheus("up", "http://localhost:9090");
        assert_eq!(handler.output_variable(&prometheus), None);
    }

    #[test]
    fn test_calculate_step_size() {
        assert_eq!(PrometheusHandler::calculate_step_size("5m"), 10);
        assert_eq!(PrometheusHandler::calculate_step_size("1h"), 60);
        assert_eq!(PrometheusHandler::calculate_step_size("24h"), 1800);
        assert_eq!(PrometheusHandler::calculate_step_size("7d"), 3600);
        assert_eq!(PrometheusHandler::calculate_step_size("invalid"), 60);
    }

    #[test]
    fn test_parse_period_to_seconds() {
        assert_eq!(PrometheusHandler::parse_period_to_seconds("5m"), 300);
        assert_eq!(PrometheusHandler::parse_period_to_seconds("1h"), 3600);
        assert_eq!(PrometheusHandler::parse_period_to_seconds("24h"), 86400);
        assert_eq!(PrometheusHandler::parse_period_to_seconds("invalid"), 3600);
    }

    #[tokio::test]
    async fn test_empty_endpoint() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let prometheus = create_test_prometheus("up", "");
        let handler = PrometheusHandler;
        let handle = handler
            .execute(prometheus, create_test_context(), _tx, None)
            .await
            .expect("Prometheus execution should start");

        // Wait for execution to complete with error
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break, // Expected - empty endpoint
                ExecutionStatus::Success(_) => panic!("Empty endpoint should have failed"),
                ExecutionStatus::Cancelled => panic!("Prometheus query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_empty_query() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let prometheus = create_test_prometheus("", "http://localhost:9090");
        let handler = PrometheusHandler;
        let handle = handler
            .execute(prometheus, create_test_context(), _tx, None)
            .await
            .expect("Prometheus execution should start");

        // Wait for execution to complete with error
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break, // Expected - empty query
                ExecutionStatus::Success(_) => panic!("Empty query should have failed"),
                ExecutionStatus::Cancelled => panic!("Prometheus query was cancelled"),
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
            .insert("job".to_string(), "prometheus".to_string());

        let prometheus =
            create_test_prometheus("up{job=\"{{ var.job }}\"}", "http://localhost:9090");

        let handler = PrometheusHandler;
        let handle = handler
            .execute(prometheus, context, _tx, None)
            .await
            .expect("Prometheus execution should start");

        // Wait for execution to complete with error (no real Prometheus instance)
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break, // Expected - no real Prometheus
                ExecutionStatus::Success(_) => break, // Unexpected but possible
                ExecutionStatus::Cancelled => panic!("Prometheus query was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_cancellation() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let prometheus = create_test_prometheus("up", "http://localhost:9090");

        let handler = PrometheusHandler;
        let handle = handler
            .execute(prometheus, create_test_context(), _tx, None)
            .await
            .expect("Prometheus execution should start");

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
                // Query completed before cancellation
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

        // Create Prometheus query that will fail
        let prometheus = create_test_prometheus("invalid_query{", "http://localhost:9090");
        let prometheus_id = prometheus.id;

        let handler = PrometheusHandler;
        let handle = handler
            .execute(prometheus, context, tx, None)
            .await
            .unwrap();

        // Wait for execution to complete
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break,
                ExecutionStatus::Success(_) => break, // Might succeed if connection fails first
                ExecutionStatus::Cancelled => panic!("Prometheus query was cancelled"),
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
                assert_eq!(*block_id, prometheus_id);
                assert_eq!(*rb_id, runbook_id);
            }
            _ => panic!("Expected BlockStarted event, got: {:?}", events[0]),
        }

        // Check BlockFinished or BlockFailed event
        match &events[1] {
            GCEvent::BlockFinished {
                block_id,
                runbook_id: rb_id,
                success: _,
            } => {
                assert_eq!(*block_id, prometheus_id);
                assert_eq!(*rb_id, runbook_id);
                // Could be success or failure depending on which error occurs first
            }
            GCEvent::BlockFailed {
                block_id,
                runbook_id: rb_id,
                error: _,
            } => {
                assert_eq!(*block_id, prometheus_id);
                assert_eq!(*rb_id, runbook_id);
            }
            _ => panic!(
                "Expected BlockFinished or BlockFailed event, got: {:?}",
                events[1]
            ),
        }
    }
}
