use reqwest::{Client, ClientBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use ts_rs::TS;
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::runtime::blocks::document::block_context::BlockExecutionOutput;
use crate::runtime::blocks::handler::{
    BlockErrorData, BlockFinishedData, BlockLifecycleEvent, BlockOutput, ExecutionStatus,
};
use crate::runtime::blocks::query_block::QueryBlockBehavior;
use crate::runtime::blocks::{Block, BlockBehavior};
use crate::runtime::events::GCEvent;
use crate::runtime::workflow::event::WorkflowEvent;

use super::handler::{CancellationToken, ExecutionContext, ExecutionHandle};
use super::FromDocument;

// Prometheus-specific types for QueryBlockBehavior
type PrometheusConnection = (Client, String, PrometheusTimeRange);

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub(crate) struct PrometheusQueryResult {
    series: Vec<PrometheusSeries>,
    query_executed: String,
    time_range: PrometheusTimeRange,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub(crate) struct PrometheusSeries {
    #[serde(rename = "type")]
    series_type: String,
    show_symbol: bool,
    name: String,
    data: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub(crate) struct PrometheusTimeRange {
    start: u64,
    end: u64,
    step: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum PrometheusBlockError {
    #[error("Query error: {0}")]
    QueryError(String),

    #[error("Invalid template: {0}")]
    InvalidTemplate(String),

    #[error("Invalid endpoint: {0}")]
    InvalidEndpoint(String),

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Cancelled")]
    Cancelled,

    #[error("Generic error: {0}")]
    GenericError(String),
}

impl From<&str> for PrometheusBlockError {
    fn from(value: &str) -> Self {
        PrometheusBlockError::GenericError(value.to_string())
    }
}

impl From<String> for PrometheusBlockError {
    fn from(value: String) -> Self {
        PrometheusBlockError::GenericError(value)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Prometheus {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub name: String,

    #[builder(setter(into))]
    pub query: String,

    #[builder(setter(into))]
    pub endpoint: String,

    #[builder(setter(into))]
    pub period: String,

    #[builder(default = false)]
    pub auto_refresh: bool,
}

impl FromDocument for Prometheus {
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

        let prometheus = Prometheus::builder()
            .id(id)
            .name(
                props
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Prometheus Query")
                    .to_string(),
            )
            .query(
                props
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            )
            .endpoint(
                props
                    .get("endpoint")
                    .and_then(|v| v.as_str())
                    .unwrap_or("http://localhost:9090")
                    .to_string(),
            )
            .period(
                props
                    .get("period")
                    .and_then(|v| v.as_str())
                    .unwrap_or("5m")
                    .to_string(),
            )
            .auto_refresh(
                props
                    .get("autoRefresh")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            )
            .build();

        Ok(prometheus)
    }
}

impl Prometheus {
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

        if url::Url::parse(endpoint).is_err() {
            return Err("Invalid URL format".to_string());
        }

        Ok(())
    }

    /// Template Prometheus query using the context resolver
    fn template_prometheus_query(
        &self,
        context: &ExecutionContext,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let rendered = context.context_resolver.resolve_template(&self.query)?;
        Ok(rendered)
    }

    /// Calculate step size for Prometheus range queries based on time period
    fn calculate_step_size(period: &str) -> u32 {
        match period {
            "5m" => 10,
            "15m" => 30,
            "30m" => 60,
            "1h" => 60,
            "3h" => 300,
            "6h" => 600,
            "24h" => 1800,
            "2d" => 3600,
            "7d" => 3600,
            "30d" => 14400,
            "90d" => 86400,
            "180d" => 86400,
            _ => 60,
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
            _ => 60 * 60,
        }
    }

    /// Execute Prometheus range query
    #[allow(clippy::too_many_arguments)]
    async fn execute_range_query(
        &self,
        client: &Client,
        endpoint: &str,
        query: &str,
        start: u64,
        end: u64,
        step: u32,
        context: &ExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/api/v1/query_range", endpoint.trim_end_matches('/'));

        let params = [
            ("query", query),
            ("start", &start.to_string()),
            ("end", &end.to_string()),
            ("step", &format!("{}s", step)),
        ];

        let _ = context
            .send_output(
                BlockOutput::builder()
                    .block_id(self.id)
                    .stdout(format!("Executing Prometheus query: {}", query))
                    .build(),
            )
            .await;

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

        if json.get("status").and_then(|s| s.as_str()) != Some("success") {
            let error = json
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("Unknown Prometheus error");
            return Err(format!("Prometheus API error: {}", error).into());
        }

        let data = json.get("data").ok_or("Missing data field in response")?;
        let result = data.get("result").ok_or("Missing result field in data")?;

        let mut series = Vec::new();

        if let Some(result_array) = result.as_array() {
            for series_data in result_array {
                if let (Some(metric), Some(values)) = (
                    series_data.get("metric"),
                    series_data.get("values").and_then(|v| v.as_array()),
                ) {
                    let series_name = if let Some(metric_obj) = metric.as_object() {
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
                                let timestamp = pair[0].as_f64().unwrap_or(0.0) * 1000.0;
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

        let result_json = json!({
            "series": series,
            "queryExecuted": query,
            "timeRange": {
                "start": start,
                "end": end,
                "step": step
            }
        });

        let _ = context
            .send_output(
                BlockOutput::builder()
                    .block_id(self.id)
                    .object(result_json)
                    .build(),
            )
            .await;

        Ok(())
    }

    async fn run_prometheus_query(
        &self,
        context: ExecutionContext,
        cancellation_token: CancellationToken,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let _ = context.emit_workflow_event(WorkflowEvent::BlockStarted { id: self.id });

        let _ = context
            .send_output(
                BlockOutput::builder()
                    .block_id(self.id)
                    .lifecycle(BlockLifecycleEvent::Started)
                    .build(),
            )
            .await;

        let query = self
            .template_prometheus_query(&context)
            .unwrap_or_else(|e| {
                eprintln!("Template error in Prometheus query {}: {}", self.id, e);
                self.query.clone()
            });

        if let Err(e) = Self::validate_prometheus_endpoint(&self.endpoint) {
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

        if query.trim().is_empty() {
            let error_msg = "Prometheus query cannot be empty";
            let _ = context
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

        let _ = context
            .send_output(
                BlockOutput::builder()
                    .block_id(self.id)
                    .stdout(format!("Connecting to Prometheus: {}", self.endpoint))
                    .build(),
            )
            .await;

        let client = ClientBuilder::new()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let period_seconds = Self::parse_period_to_seconds(&self.period);
        let start = now - period_seconds as u64;
        let end = now;
        let step = Self::calculate_step_size(&self.period);

        let cancellation_receiver = cancellation_token.take_receiver();
        let endpoint = self.endpoint.clone();
        let context_clone = context.clone();

        let execution_task = async move {
            self.execute_range_query(&client, &endpoint, &query, start, end, step, &context_clone)
                .await
        };

        let result = if let Some(cancel_rx) = cancellation_receiver {
            tokio::select! {
                _ = cancel_rx => {
                    if let Some(event_bus) = &context.gc_event_bus {
                        let _ = event_bus.emit(GCEvent::BlockCancelled {
                            block_id: self.id,
                            runbook_id: context.runbook_id,
                        }).await;
                    }

                    let _ = context.emit_workflow_event(WorkflowEvent::BlockFinished { id: self.id });
                        let _ = context.send_output(
                            BlockOutput::builder()
                                .block_id(self.id)
                                .lifecycle(BlockLifecycleEvent::Cancelled)
                                .build(),
                        ).await;
                    return Err("Prometheus query execution cancelled".into());
                }
                result = execution_task => {
                    result
                }
            }
        } else {
            execution_task.await
        };

        let _ = context.emit_workflow_event(WorkflowEvent::BlockFinished { id: self.id });
        let _ = context
            .send_output(
                BlockOutput::builder()
                    .block_id(self.id)
                    .stdout("Prometheus query completed successfully".to_string())
                    .build(),
            )
            .await;

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
impl BlockBehavior for Prometheus {
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::Prometheus(self)
    }

    async fn execute(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        QueryBlockBehavior::execute_query_block(self, context).await
    }
}

#[async_trait::async_trait]
impl QueryBlockBehavior for Prometheus {
    type Connection = PrometheusConnection;
    type QueryResult = PrometheusQueryResult;
    type Error = PrometheusBlockError;

    fn resolve_query(&self, context: &ExecutionContext) -> Result<String, Self::Error> {
        context
            .context_resolver
            .resolve_template(&self.query)
            .map_err(|e| PrometheusBlockError::InvalidTemplate(e.to_string()))
    }

    fn resolve_connection_string(&self, context: &ExecutionContext) -> Result<String, Self::Error> {
        let endpoint = context
            .context_resolver
            .resolve_template(&self.endpoint)
            .map_err(|e| PrometheusBlockError::InvalidTemplate(e.to_string()))?;

        // Validate endpoint format
        if endpoint.is_empty() {
            return Err(PrometheusBlockError::InvalidEndpoint(
                "Prometheus endpoint cannot be empty".to_string(),
            ));
        }

        if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
            return Err(PrometheusBlockError::InvalidEndpoint(
                "Invalid Prometheus endpoint format. Must start with 'http://' or 'https://'"
                    .to_string(),
            ));
        }

        if url::Url::parse(&endpoint).is_err() {
            return Err(PrometheusBlockError::InvalidEndpoint(
                "Invalid URL format".to_string(),
            ));
        }

        // Encode the period into the connection string (format: endpoint|period)
        Ok(format!("{}|{}", endpoint, self.period))
    }

    async fn connect(connection_string: String) -> Result<Self::Connection, Self::Error> {
        // Parse the connection string (format: endpoint|period)
        let parts: Vec<&str> = connection_string.split('|').collect();
        let endpoint = parts[0].to_string();
        let period = parts.get(1).unwrap_or(&"1h");

        let client = ClientBuilder::new()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| {
                PrometheusBlockError::ConnectionError(format!(
                    "Failed to create HTTP client: {}",
                    e
                ))
            })?;

        // Calculate time range based on the period
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let period_seconds = Self::parse_period_to_seconds(period);
        let step = Self::calculate_step_size(period);

        let time_range = PrometheusTimeRange {
            start: now - period_seconds as u64,
            end: now,
            step,
        };

        Ok((client, endpoint, time_range))
    }

    async fn disconnect(_connection: &Self::Connection) -> Result<(), Self::Error> {
        Ok(()) // HTTP client cleanup is automatic
    }

    fn is_cancelled_error(error: &Self::Error) -> bool {
        matches!(error, PrometheusBlockError::Cancelled)
    }

    async fn execute_query(
        connection: &Self::Connection,
        query: &str,
        _context: &ExecutionContext,
    ) -> Result<Vec<Self::QueryResult>, Self::Error> {
        let (client, endpoint, time_range) = connection;

        let url = format!("{}/api/v1/query_range", endpoint.trim_end_matches('/'));

        let params = [
            ("query", query),
            ("start", &time_range.start.to_string()),
            ("end", &time_range.end.to_string()),
            ("step", &format!("{}s", time_range.step)),
        ];

        let response = client.get(&url).query(&params).send().await.map_err(|e| {
            PrometheusBlockError::QueryError(format!("Failed to send request: {}", e))
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(PrometheusBlockError::QueryError(format!(
                "HTTP {}: {}",
                status, text
            )));
        }

        let json: Value = response.json().await.map_err(|e| {
            PrometheusBlockError::QueryError(format!("Failed to parse JSON response: {}", e))
        })?;

        if json.get("status").and_then(|s| s.as_str()) != Some("success") {
            let error = json
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("Unknown Prometheus error");
            return Err(PrometheusBlockError::QueryError(format!(
                "Prometheus API error: {}",
                error
            )));
        }

        let data = json.get("data").ok_or_else(|| {
            PrometheusBlockError::QueryError("Missing data field in response".to_string())
        })?;
        let result = data.get("result").ok_or_else(|| {
            PrometheusBlockError::QueryError("Missing result field in data".to_string())
        })?;

        let mut series = Vec::new();

        if let Some(result_array) = result.as_array() {
            for series_data in result_array {
                if let (Some(metric), Some(values)) = (
                    series_data.get("metric"),
                    series_data.get("values").and_then(|v| v.as_array()),
                ) {
                    let series_name = if let Some(metric_obj) = metric.as_object() {
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
                                let timestamp = pair[0].as_f64().unwrap_or(0.0) * 1000.0;
                                let value = pair[1]
                                    .as_str()
                                    .and_then(|s| s.parse::<f64>().ok())
                                    .unwrap_or(0.0);
                                data_points.push((timestamp, value));
                            }
                        }
                    }

                    series.push(PrometheusSeries {
                        series_type: "line".to_string(),
                        show_symbol: false,
                        name: series_name,
                        data: data_points,
                    });
                }
            }
        }

        let result = PrometheusQueryResult {
            series,
            query_executed: query.to_string(),
            time_range: time_range.clone(),
        };

        Ok(vec![result])
    }
}
