use reqwest::{Client, ClientBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use ts_rs::TS;
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::blocks::{Block, BlockBehavior, BlockExecutionError, FromDocument, QueryBlockBehavior};
use crate::context::BlockExecutionOutput;
use crate::execution::{ExecutionContext, ExecutionHandle};

// Prometheus-specific types for QueryBlockBehavior
pub type PrometheusConnection = (Client, String, PrometheusTimeRange);

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PrometheusQueryResult {
    /// Columnar data for uPlot: [timestamps, series1_values, series2_values, ...]
    /// Timestamps are in seconds (Unix epoch)
    data: Vec<Vec<f64>>,
    /// Series names in order (index i corresponds to data[i+1])
    series_names: Vec<String>,
    query_executed: String,
    time_range: PrometheusTimeRange,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PrometheusTimeRange {
    start: u64,
    end: u64,
    step: u32,
}

/// Output structure for Prometheus blocks that implements BlockExecutionOutput
/// for template access to query results.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PrometheusBlockOutput {
    /// All query results from the Prometheus execution
    pub results: Vec<PrometheusQueryResult>,
    /// Total number of series across all results
    pub total_series: usize,
}

impl PrometheusBlockOutput {
    /// Create a new PrometheusBlockOutput from query results
    pub fn new(results: Vec<PrometheusQueryResult>) -> Self {
        let total_series = results.iter().map(|r| r.series_names.len()).sum();
        Self {
            results,
            total_series,
        }
    }

    /// Get the first result (convenience for single-query access)
    pub fn first_result(&self) -> Option<&PrometheusQueryResult> {
        self.results.first()
    }
}

impl BlockExecutionOutput for PrometheusBlockOutput {
    fn get_template_value(&self, key: &str) -> Option<minijinja::Value> {
        match key {
            // Primary access
            "results" => Some(minijinja::Value::from_serialize(&self.results)),
            "first" => self.first_result().map(minijinja::Value::from_serialize),

            // Aggregated data
            "total_series" => Some(minijinja::Value::from(self.total_series)),
            "result_count" => Some(minijinja::Value::from(self.results.len())),

            // Convenience accessors for first result
            "series_names" => self
                .first_result()
                .map(|r| minijinja::Value::from_serialize(&r.series_names)),
            "data" => self
                .first_result()
                .map(|r| minijinja::Value::from_serialize(&r.data)),
            "query_executed" => self
                .first_result()
                .map(|r| minijinja::Value::from(r.query_executed.clone())),
            "time_range" => self
                .first_result()
                .map(|r| minijinja::Value::from_serialize(&r.time_range)),

            _ => None,
        }
    }

    fn enumerate_template_keys(&self) -> minijinja::value::Enumerator {
        minijinja::value::Enumerator::Str(&[
            "results",
            "first",
            "total_series",
            "result_count",
            "series_names",
            "data",
            "query_executed",
            "time_range",
        ])
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PrometheusBlockError {
    #[error("Operation timed out")]
    Timeout,

    #[error("Serialization error: {0}")]
    SerializationError(String),

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
}

impl BlockExecutionError for PrometheusBlockError {
    fn cancelled() -> Self {
        PrometheusBlockError::Cancelled
    }

    fn timeout(_message: String) -> Self {
        PrometheusBlockError::Timeout
    }

    fn serialization_error(message: String) -> Self {
        PrometheusBlockError::SerializationError(message)
    }

    fn is_cancelled(&self) -> bool {
        matches!(self, PrometheusBlockError::Cancelled)
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

        // No need to encode period anymore - we have access to self in connect()
        Ok(endpoint)
    }

    async fn connect(&self, endpoint: String) -> Result<Self::Connection, Self::Error> {
        let client = ClientBuilder::new()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| {
                PrometheusBlockError::ConnectionError(format!(
                    "Failed to create HTTP client: {}",
                    e
                ))
            })?;

        // Calculate time range based on the period (now available from self)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let period_seconds = Self::parse_period_to_seconds(&self.period);
        let step = Self::calculate_step_size(&self.period);

        let time_range = PrometheusTimeRange {
            start: now - period_seconds as u64,
            end: now,
            step,
        };

        Ok((client, endpoint, time_range))
    }

    async fn disconnect(&self, _connection: &Self::Connection) -> Result<(), Self::Error> {
        Ok(()) // HTTP client cleanup is automatic
    }

    async fn execute_query(
        &self,
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

        let mut series_names: Vec<String> = Vec::new();
        let mut all_values: Vec<Vec<f64>> = Vec::new();
        let mut timestamps: Vec<f64> = Vec::new();

        if let Some(result_array) = result.as_array() {
            for series_data in result_array {
                if let (Some(metric), Some(values)) = (
                    series_data.get("metric"),
                    series_data.get("values").and_then(|v| v.as_array()),
                ) {
                    // Build series name from metric labels
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

                    // Extract timestamps (only from first series - all share same timestamps)
                    // and values for this series
                    let mut series_values: Vec<f64> = Vec::new();
                    for value_pair in values {
                        if let Some(pair) = value_pair.as_array() {
                            if pair.len() == 2 {
                                // Timestamps in seconds (uPlot native format)
                                let ts = pair[0].as_f64().unwrap_or(0.0);
                                let value = pair[1]
                                    .as_str()
                                    .and_then(|s| s.parse::<f64>().ok())
                                    .unwrap_or(0.0);

                                // Only collect timestamps from first series
                                if series_names.is_empty() {
                                    timestamps.push(ts);
                                }
                                series_values.push(value);
                            }
                        }
                    }

                    series_names.push(series_name);
                    all_values.push(series_values);
                }
            }
        }

        // Build columnar data: [timestamps, series1_values, series2_values, ...]
        let mut data: Vec<Vec<f64>> = Vec::with_capacity(1 + all_values.len());
        data.push(timestamps);
        data.extend(all_values);

        let result = PrometheusQueryResult {
            data,
            series_names,
            query_executed: query.to_string(),
            time_range: time_range.clone(),
        };

        Ok(vec![result])
    }

    fn create_output(
        &self,
        results: &[Self::QueryResult],
    ) -> Option<Box<dyn BlockExecutionOutput>> {
        Some(Box::new(PrometheusBlockOutput::new(results.to_vec())))
    }
}
