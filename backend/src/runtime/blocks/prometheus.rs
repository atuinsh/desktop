use reqwest::{Client, ClientBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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
                    .unwrap_or("")
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
    async fn execute_range_query(
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

        if let Some(ref ch) = context.output_channel {
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

        if let Some(ref ch) = context.output_channel {
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

        let query = self.template_prometheus_query(&context).unwrap_or_else(|e| {
            eprintln!("Template error in Prometheus query {}: {}", block_id, e);
            self.query.clone()
        });

        if let Err(e) = Self::validate_prometheus_endpoint(&self.endpoint) {
            if let Some(ref ch) = context.output_channel {
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

        if let Some(ref ch) = context.output_channel {
            let _ = ch.send(BlockOutput {
                stdout: Some(format!("Connecting to Prometheus: {}", self.endpoint)),
                stderr: None,
                binary: None,
                object: None,
                lifecycle: None,
            });
        }

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
            Self::execute_range_query(&client, &endpoint, &query, start, end, step, &context_clone)
                .await
        };

        let result = if let Some(cancel_rx) = cancellation_receiver {
            tokio::select! {
                _ = cancel_rx => {
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
                    return Err("Prometheus query execution cancelled".into());
                }
                result = execution_task => {
                    result
                }
            }
        } else {
            execution_task.await
        };

        let _ = context
            .event_sender
            .send(WorkflowEvent::BlockFinished { id: block_id });
        if let Some(ref ch) = context.output_channel {
            let _ = ch.send(BlockOutput {
                stdout: Some("Prometheus query completed successfully".to_string()),
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
impl BlockBehavior for Prometheus {
    fn into_block(self) -> Block {
        Block::Prometheus(self)
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
                .run_prometheus_query(context_clone.clone(), handle_clone.cancellation_token.clone())
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

                    ExecutionStatus::Success(
                        "Prometheus query completed successfully".to_string(),
                    )
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
