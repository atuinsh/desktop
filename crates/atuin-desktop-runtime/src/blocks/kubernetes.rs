use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::process::Command;
use ts_rs::TS;
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::blocks::{Block, BlockBehavior};
use crate::context::BlockExecutionOutput;
use crate::execution::{ExecutionContext, ExecutionHandle, StreamingBlockOutput};

use super::FromDocument;

#[derive(Debug, thiserror::Error)]
pub enum KubernetesError {
    #[error("Command execution failed: {0}")]
    ExecutionError(String),
    #[error("Template evaluation error: {0}")]
    Template(#[from] minijinja::Error),
    #[error("JSON parsing error: {0}")]
    JsonParsing(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Output structure for Kubernetes blocks that implements BlockExecutionOutput
/// for template access to kubectl results.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KubernetesBlockOutput {
    /// The parsed table data as rows (each row is a list of cell values)
    pub data: Vec<Vec<Value>>,
    /// Column definitions for the table
    pub columns: Vec<KubernetesColumn>,
    /// Number of items/rows returned
    pub item_count: usize,
    /// The resource kind if detected (e.g., "pod", "service", "deployment")
    pub resource_kind: Option<String>,
    /// Raw stdout from the command (if not parsed as JSON)
    pub raw_output: Option<String>,
    /// Stderr output if any
    pub stderr: Option<String>,
}

impl KubernetesBlockOutput {
    /// Create a new output from parsed table data
    pub fn from_table(
        data: Vec<Vec<Value>>,
        columns: Vec<KubernetesColumn>,
        resource_kind: Option<String>,
    ) -> Self {
        let item_count = data.len();
        Self {
            data,
            columns,
            item_count,
            resource_kind,
            raw_output: None,
            stderr: None,
        }
    }

    /// Create a new output from raw (non-JSON) output
    pub fn from_raw(raw_output: String, stderr: Option<String>) -> Self {
        let line_count = raw_output.lines().count();
        Self {
            data: vec![],
            columns: vec![],
            item_count: line_count,
            resource_kind: None,
            raw_output: Some(raw_output),
            stderr,
        }
    }

    /// Get the first row of data
    pub fn first_row(&self) -> Option<&Vec<Value>> {
        self.data.first()
    }
}

impl BlockExecutionOutput for KubernetesBlockOutput {
    fn get_template_value(&self, key: &str) -> Option<minijinja::Value> {
        match key {
            // Table data access
            "data" => Some(minijinja::Value::from_serialize(&self.data)),
            "columns" => Some(minijinja::Value::from_serialize(&self.columns)),
            "first_row" => self.first_row().map(minijinja::Value::from_serialize),

            // Metadata
            "item_count" => Some(minijinja::Value::from(self.item_count)),
            "resource_kind" => self
                .resource_kind
                .as_ref()
                .map(|k| minijinja::Value::from(k.clone())),

            // Raw output
            "raw_output" => self
                .raw_output
                .as_ref()
                .map(|s| minijinja::Value::from(s.clone())),
            "stderr" => self
                .stderr
                .as_ref()
                .map(|s| minijinja::Value::from(s.clone())),

            // Convenience: check if we have structured data
            "has_table" => Some(minijinja::Value::from(!self.data.is_empty())),

            _ => None,
        }
    }

    fn enumerate_template_keys(&self) -> minijinja::value::Enumerator {
        minijinja::value::Enumerator::Str(&[
            "data",
            "columns",
            "first_row",
            "item_count",
            "resource_kind",
            "raw_output",
            "stderr",
            "has_table",
        ])
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Kubernetes {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub name: String,

    #[builder(setter(into))]
    pub command: String,

    #[builder(default = "bash".to_string(), setter(into))]
    pub interpreter: String,

    #[builder(default)]
    pub namespace: String,

    #[builder(default)]
    pub context: String,

    #[builder(default = false)]
    pub auto_refresh: bool,

    #[builder(default = 0)]
    pub refresh_interval: u32,
}

impl FromDocument for Kubernetes {
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

        let kubernetes = Kubernetes::builder()
            .id(id)
            .name(
                props
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Kubernetes Query")
                    .to_string(),
            )
            .command(
                props
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            )
            .interpreter(
                props
                    .get("interpreter")
                    .and_then(|v| v.as_str())
                    .unwrap_or("bash")
                    .to_string(),
            )
            .namespace(
                props
                    .get("namespace")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            )
            .context(
                props
                    .get("context")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            )
            .auto_refresh(
                props
                    .get("autoRefresh")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            )
            .refresh_interval(
                props
                    .get("refreshInterval")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
            )
            .build();

        Ok(kubernetes)
    }
}

#[async_trait::async_trait]
impl BlockBehavior for Kubernetes {
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::Kubernetes(self)
    }

    async fn execute(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        tracing::trace!("Executing Kubernetes block {id}", id = self.id);

        let _ = context.block_started().await;

        let block_id = self.id;
        let result = self.execute_kubectl_command(&context).await;

        if let Err(e) = result {
            tracing::error!("{e}");
            let error_message = e.to_string();
            let _ = context.block_failed(error_message).await;
            return Err(e.into());
        }

        let (stdout, stderr) = result.unwrap();
        let stderr_for_output = if stderr.trim().is_empty() {
            None
        } else {
            Some(stderr.clone())
        };

        // Send stdout if present
        if !stdout.trim().is_empty() {
            // Try to parse as JSON kubectl output
            if let Ok((parsed_output, block_output)) =
                self.parse_kubectl_output_with_block_output(&stdout, stderr_for_output.clone())
            {
                let _ = context
                    .send_output(
                        StreamingBlockOutput::builder()
                            .block_id(block_id)
                            .object(parsed_output)
                            .build(),
                    )
                    .await;

                // Store structured output for template access
                let _ = context.set_block_output(block_output).await;
            } else {
                // If not JSON, send as plain stdout
                let _ = context
                    .send_output(
                        StreamingBlockOutput::builder()
                            .block_id(block_id)
                            .object(json!({
                                "type": "kubernetes",
                                "data": stdout.lines().map(|line| vec![line]).collect::<Vec<_>>(),
                                "columns": [
                                    {
                                        "id": "raw-output",
                                        "title": "Raw Output",
                                        "width": 600
                                    }
                                ]
                            }))
                            .build(),
                    )
                    .await;

                // Store raw output for template access
                let _ = context
                    .set_block_output(KubernetesBlockOutput::from_raw(
                        stdout.clone(),
                        stderr_for_output,
                    ))
                    .await;
            }
        } else if stderr_for_output.is_some() {
            // No stdout but we have stderr - still store it
            let _ = context
                .set_block_output(KubernetesBlockOutput::from_raw(
                    String::new(),
                    stderr_for_output,
                ))
                .await;
        }

        // Send stderr if present
        if !stderr.trim().is_empty() {
            let _ = context
                .send_output(
                    StreamingBlockOutput::builder()
                        .block_id(block_id)
                        .stderr(stderr)
                        .build(),
                )
                .await;
        }

        let _ = context.block_finished(None, true).await;

        Ok(Some(context.handle()))
    }
}

impl Kubernetes {
    async fn execute_kubectl_command(
        &self,
        context: &ExecutionContext,
    ) -> Result<(String, String), KubernetesError> {
        // Resolve the command template
        let mut command = context.context_resolver.resolve_template(&self.command)?;

        // Add namespace and context flags to kubectl commands if specified
        if command.contains("kubectl") {
            if !self.namespace.is_empty() {
                let namespace = context.context_resolver.resolve_template(&self.namespace)?;
                if !namespace.trim().is_empty() {
                    command = format!("{} --namespace {}", command, namespace.trim());
                }
            }

            if !self.context.is_empty() {
                let kube_context = context.context_resolver.resolve_template(&self.context)?;
                if !kube_context.trim().is_empty() {
                    command = format!("{} --context {}", command, kube_context.trim());
                }
            }
        }

        // Execute the command using the specified interpreter
        let output = Command::new(&self.interpreter)
            .arg("-c")
            .arg(&command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            return Err(KubernetesError::ExecutionError(format!(
                "Command failed with exit code {:?}: {}",
                output.status.code(),
                stderr
            )));
        }

        Ok((stdout, stderr))
    }

    /// Parse kubectl output and return both the streaming Value and the BlockOutput
    fn parse_kubectl_output_with_block_output(
        &self,
        output: &str,
        stderr: Option<String>,
    ) -> Result<(Value, KubernetesBlockOutput), KubernetesError> {
        let parsed: Value = serde_json::from_str(output)
            .map_err(|e| KubernetesError::JsonParsing(e.to_string()))?;

        // Check if this is a kubectl JSON list output
        if let Some(items) = parsed.get("items").and_then(|v| v.as_array()) {
            if items.is_empty() {
                let streaming_output = json!({
                    "type": "kubernetes",
                    "data": [],
                    "columns": []
                });
                let block_output = KubernetesBlockOutput::from_table(vec![], vec![], None);
                return Ok((streaming_output, block_output));
            }

            // Detect resource type and create appropriate columns
            let kind = items[0]
                .get("kind")
                .and_then(|k| k.as_str())
                .unwrap_or("unknown")
                .to_lowercase();

            let (data, columns) = match kind.as_str() {
                "pod" => self.parse_pods(items),
                "service" => self.parse_services(items),
                "deployment" => self.parse_deployments(items),
                "configmap" => self.parse_configmaps(items),
                "secret" => self.parse_secrets(items),
                "node" => self.parse_nodes(items),
                "namespace" => self.parse_namespaces(items),
                _ => self.parse_generic(items),
            };

            let streaming_output = json!({
                "type": "kubernetes",
                "data": data,
                "columns": columns
            });

            let mut block_output =
                KubernetesBlockOutput::from_table(data, columns, Some(kind.clone()));
            block_output.stderr = stderr;

            Ok((streaming_output, block_output))
        } else {
            // Not a kubectl list - return raw JSON with the parsed value
            let block_output = KubernetesBlockOutput {
                data: vec![],
                columns: vec![],
                item_count: 1,
                resource_kind: parsed
                    .get("kind")
                    .and_then(|k| k.as_str())
                    .map(String::from),
                raw_output: Some(output.to_string()),
                stderr,
            };
            Ok((parsed, block_output))
        }
    }

    fn parse_pods(&self, items: &[Value]) -> (Vec<Vec<Value>>, Vec<KubernetesColumn>) {
        let data: Vec<Vec<Value>> = items
            .iter()
            .map(|item| {
                vec![
                    json!(item["metadata"]["name"].as_str().unwrap_or("Unknown")),
                    json!(item["metadata"]["namespace"].as_str().unwrap_or("default")),
                    json!(Self::get_ready_status(item)),
                    json!(item["status"]["phase"].as_str().unwrap_or("Unknown")),
                    json!(Self::get_restart_count(item)),
                    json!(Self::get_age(
                        item["metadata"]["creationTimestamp"].as_str().unwrap_or("")
                    )),
                    json!(item["status"]["podIP"].as_str().unwrap_or("")),
                    json!(item["spec"]["nodeName"].as_str().unwrap_or("")),
                ]
            })
            .collect();

        let columns = vec![
            KubernetesColumn::new("name", "Name", 200),
            KubernetesColumn::new("namespace", "Namespace", 120),
            KubernetesColumn::new("ready", "Ready", 80),
            KubernetesColumn::new("status", "Status", 100),
            KubernetesColumn::new("restarts", "Restarts", 80),
            KubernetesColumn::new("age", "Age", 80),
            KubernetesColumn::new("ip", "IP", 120),
            KubernetesColumn::new("node", "Node", 150),
        ];

        (data, columns)
    }

    fn parse_services(&self, items: &[Value]) -> (Vec<Vec<Value>>, Vec<KubernetesColumn>) {
        let data: Vec<Vec<Value>> = items
            .iter()
            .map(|item| {
                vec![
                    json!(item["metadata"]["name"].as_str().unwrap_or("Unknown")),
                    json!(item["metadata"]["namespace"].as_str().unwrap_or("default")),
                    json!(item["spec"]["type"].as_str().unwrap_or("ClusterIP")),
                    json!(item["spec"]["clusterIP"].as_str().unwrap_or("")),
                    json!(Self::get_external_ip(item)),
                    json!(Self::get_ports(item["spec"]["ports"].as_array())),
                    json!(Self::get_age(
                        item["metadata"]["creationTimestamp"].as_str().unwrap_or("")
                    )),
                ]
            })
            .collect();

        let columns = vec![
            KubernetesColumn::new("name", "Name", 200),
            KubernetesColumn::new("namespace", "Namespace", 120),
            KubernetesColumn::new("type", "Type", 100),
            KubernetesColumn::new("clusterIP", "Cluster IP", 120),
            KubernetesColumn::new("externalIP", "External IP", 120),
            KubernetesColumn::new("ports", "Ports", 150),
            KubernetesColumn::new("age", "Age", 80),
        ];

        (data, columns)
    }

    fn parse_deployments(&self, items: &[Value]) -> (Vec<Vec<Value>>, Vec<KubernetesColumn>) {
        let data: Vec<Vec<Value>> = items
            .iter()
            .map(|item| {
                let ready = item["status"]["readyReplicas"].as_u64().unwrap_or(0);
                let desired = item["spec"]["replicas"].as_u64().unwrap_or(0);
                vec![
                    json!(item["metadata"]["name"].as_str().unwrap_or("Unknown")),
                    json!(item["metadata"]["namespace"].as_str().unwrap_or("default")),
                    json!(format!("{}/{}", ready, desired)),
                    json!(item["status"]["updatedReplicas"].as_u64().unwrap_or(0)),
                    json!(item["status"]["availableReplicas"].as_u64().unwrap_or(0)),
                    json!(Self::get_age(
                        item["metadata"]["creationTimestamp"].as_str().unwrap_or("")
                    )),
                ]
            })
            .collect();

        let columns = vec![
            KubernetesColumn::new("name", "Name", 200),
            KubernetesColumn::new("namespace", "Namespace", 120),
            KubernetesColumn::new("ready", "Ready", 100),
            KubernetesColumn::new("upToDate", "Up-to-date", 100),
            KubernetesColumn::new("available", "Available", 100),
            KubernetesColumn::new("age", "Age", 80),
        ];

        (data, columns)
    }

    fn parse_configmaps(&self, items: &[Value]) -> (Vec<Vec<Value>>, Vec<KubernetesColumn>) {
        let data: Vec<Vec<Value>> = items
            .iter()
            .map(|item| {
                let data_count = item["data"].as_object().map(|obj| obj.len()).unwrap_or(0);
                vec![
                    json!(item["metadata"]["name"].as_str().unwrap_or("Unknown")),
                    json!(item["metadata"]["namespace"].as_str().unwrap_or("default")),
                    json!(data_count),
                    json!(Self::get_age(
                        item["metadata"]["creationTimestamp"].as_str().unwrap_or("")
                    )),
                ]
            })
            .collect();

        let columns = vec![
            KubernetesColumn::new("name", "Name", 300),
            KubernetesColumn::new("namespace", "Namespace", 120),
            KubernetesColumn::new("data", "Data", 80),
            KubernetesColumn::new("age", "Age", 80),
        ];

        (data, columns)
    }

    fn parse_secrets(&self, items: &[Value]) -> (Vec<Vec<Value>>, Vec<KubernetesColumn>) {
        let data: Vec<Vec<Value>> = items
            .iter()
            .map(|item| {
                let data_count = item["data"].as_object().map(|obj| obj.len()).unwrap_or(0);
                vec![
                    json!(item["metadata"]["name"].as_str().unwrap_or("Unknown")),
                    json!(item["metadata"]["namespace"].as_str().unwrap_or("default")),
                    json!(item["type"].as_str().unwrap_or("Opaque")),
                    json!(data_count),
                    json!(Self::get_age(
                        item["metadata"]["creationTimestamp"].as_str().unwrap_or("")
                    )),
                ]
            })
            .collect();

        let columns = vec![
            KubernetesColumn::new("name", "Name", 300),
            KubernetesColumn::new("namespace", "Namespace", 120),
            KubernetesColumn::new("type", "Type", 150),
            KubernetesColumn::new("data", "Data", 80),
            KubernetesColumn::new("age", "Age", 80),
        ];

        (data, columns)
    }

    fn parse_nodes(&self, items: &[Value]) -> (Vec<Vec<Value>>, Vec<KubernetesColumn>) {
        let data: Vec<Vec<Value>> = items
            .iter()
            .map(|item| {
                vec![
                    json!(item["metadata"]["name"].as_str().unwrap_or("Unknown")),
                    json!(Self::get_node_status(item)),
                    json!(Self::get_node_roles(item)),
                    json!(Self::get_age(
                        item["metadata"]["creationTimestamp"].as_str().unwrap_or("")
                    )),
                    json!(item["status"]["nodeInfo"]["kubeletVersion"]
                        .as_str()
                        .unwrap_or("")),
                ]
            })
            .collect();

        let columns = vec![
            KubernetesColumn::new("name", "Name", 200),
            KubernetesColumn::new("status", "Status", 100),
            KubernetesColumn::new("roles", "Roles", 150),
            KubernetesColumn::new("age", "Age", 80),
            KubernetesColumn::new("version", "Version", 120),
        ];

        (data, columns)
    }

    fn parse_namespaces(&self, items: &[Value]) -> (Vec<Vec<Value>>, Vec<KubernetesColumn>) {
        let data: Vec<Vec<Value>> = items
            .iter()
            .map(|item| {
                vec![
                    json!(item["metadata"]["name"].as_str().unwrap_or("Unknown")),
                    json!(item["status"]["phase"].as_str().unwrap_or("Unknown")),
                    json!(Self::get_age(
                        item["metadata"]["creationTimestamp"].as_str().unwrap_or("")
                    )),
                ]
            })
            .collect();

        let columns = vec![
            KubernetesColumn::new("name", "Name", 300),
            KubernetesColumn::new("status", "Status", 100),
            KubernetesColumn::new("age", "Age", 80),
        ];

        (data, columns)
    }

    fn parse_generic(&self, items: &[Value]) -> (Vec<Vec<Value>>, Vec<KubernetesColumn>) {
        let data: Vec<Vec<Value>> = items
            .iter()
            .map(|item| {
                vec![
                    json!(item["metadata"]["name"].as_str().unwrap_or("Unknown")),
                    json!(item["metadata"]["namespace"].as_str().unwrap_or("cluster")),
                    json!(item["kind"].as_str().unwrap_or("Unknown")),
                    json!(Self::get_age(
                        item["metadata"]["creationTimestamp"].as_str().unwrap_or("")
                    )),
                ]
            })
            .collect();

        let columns = vec![
            KubernetesColumn::new("name", "Name", 250),
            KubernetesColumn::new("namespace", "Namespace", 120),
            KubernetesColumn::new("kind", "Kind", 120),
            KubernetesColumn::new("age", "Age", 80),
        ];

        (data, columns)
    }

    // Helper functions
    fn get_ready_status(pod: &Value) -> String {
        let container_statuses = pod["status"]["containerStatuses"].as_array();
        if let Some(statuses) = container_statuses {
            let ready = statuses
                .iter()
                .filter(|c| c["ready"].as_bool().unwrap_or(false))
                .count();
            let total = statuses.len();
            format!("{}/{}", ready, total)
        } else {
            "0/0".to_string()
        }
    }

    fn get_restart_count(pod: &Value) -> u64 {
        let container_statuses = pod["status"]["containerStatuses"].as_array();
        if let Some(statuses) = container_statuses {
            statuses
                .iter()
                .map(|c| c["restartCount"].as_u64().unwrap_or(0))
                .sum()
        } else {
            0
        }
    }

    fn get_age(creation_timestamp: &str) -> String {
        if creation_timestamp.is_empty() {
            return "Unknown".to_string();
        }

        use chrono::{DateTime, Utc};
        if let Ok(created) = creation_timestamp.parse::<DateTime<Utc>>() {
            let now = Utc::now();
            let duration = now.signed_duration_since(created);

            let days = duration.num_days();
            let hours = duration.num_hours();
            let minutes = duration.num_minutes();

            if days > 0 {
                format!("{}d", days)
            } else if hours > 0 {
                format!("{}h", hours)
            } else if minutes > 0 {
                format!("{}m", minutes)
            } else {
                "<1m".to_string()
            }
        } else {
            "Unknown".to_string()
        }
    }

    fn get_external_ip(service: &Value) -> String {
        if let Some(ingress) = service["status"]["loadBalancer"]["ingress"].as_array() {
            if !ingress.is_empty() {
                return ingress[0]["ip"]
                    .as_str()
                    .or_else(|| ingress[0]["hostname"].as_str())
                    .unwrap_or("")
                    .to_string();
            }
        }
        if let Some(external_ips) = service["spec"]["externalIPs"].as_array() {
            return external_ips
                .iter()
                .filter_map(|ip| ip.as_str())
                .collect::<Vec<_>>()
                .join(",");
        }
        String::new()
    }

    fn get_ports(ports: Option<&Vec<Value>>) -> String {
        if let Some(ports) = ports {
            ports
                .iter()
                .map(|p| {
                    let port = p["port"].as_u64().unwrap_or(0);
                    let protocol = p["protocol"].as_str().unwrap_or("TCP");
                    format!("{}/{}", port, protocol)
                })
                .collect::<Vec<_>>()
                .join(",")
        } else {
            String::new()
        }
    }

    fn get_node_status(node: &Value) -> String {
        if let Some(conditions) = node["status"]["conditions"].as_array() {
            for condition in conditions {
                if condition["type"].as_str() == Some("Ready") {
                    return if condition["status"].as_str() == Some("True") {
                        "Ready".to_string()
                    } else {
                        "NotReady".to_string()
                    };
                }
            }
        }
        "Unknown".to_string()
    }

    fn get_node_roles(node: &Value) -> String {
        let labels = node["metadata"]["labels"].as_object();
        if let Some(labels) = labels {
            let mut roles = Vec::new();

            if labels.contains_key("node-role.kubernetes.io/control-plane")
                || labels.contains_key("node-role.kubernetes.io/master")
            {
                roles.push("control-plane");
            }
            if labels.contains_key("node-role.kubernetes.io/worker") {
                roles.push("worker");
            }

            if roles.is_empty() {
                "<none>".to_string()
            } else {
                roles.join(",")
            }
        } else {
            "<none>".to_string()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KubernetesColumn {
    pub id: String,
    pub title: String,
    pub width: u32,
}

impl KubernetesColumn {
    fn new(id: &str, title: &str, width: u32) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
            width,
        }
    }
}
