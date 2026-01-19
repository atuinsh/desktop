# Agent Block Specification

## Overview

The Agent block enables runbooks to invoke AI agents—either external ACP-compatible agents (Claude Code, Gemini, Codex) or Atuin's internal agent. The block sends a templated prompt to an agent and captures the response as output for downstream blocks.

## Design Principles

1. **Flexibility** - Support both ACP subprocess agents and internal Atuin agent
2. **Simplicity** - Expose text output; let other blocks parse structured data
3. **Safety** - Tool calls require confirmation (with opt-in always-allow)
4. **Consistency** - Follow existing block patterns exactly

---

## Block Type

```
type: "agent"
```

---

## Configuration Schema

### Rust Struct

**File:** `crates/atuin-desktop-runtime/src/blocks/agent.rs`

```rust
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

/// Agent source: either an external ACP agent or the internal Atuin agent
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentSource {
    /// External ACP-compatible agent (Claude Code, Gemini, Codex, etc.)
    Acp {
        /// Command to spawn the agent (e.g., "claude", "gemini", "codex")
        command: String,
        /// Arguments to pass to the agent command
        #[serde(default)]
        args: Vec<String>,
        /// Environment variables for the agent process
        #[serde(default)]
        env: std::collections::HashMap<String, String>,
    },
    /// Atuin's internal agent (uses existing AI infrastructure)
    Internal {
        /// Model selection (defaults to workspace/global setting if None)
        model: Option<ModelSelection>,
    },
}

/// Capabilities the agent is allowed to use
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    /// Allow agent to read files
    #[serde(default)]
    pub read_files: bool,
    /// Allow agent to write files
    #[serde(default)]
    pub write_files: bool,
    /// Allow agent to execute shell commands
    #[serde(default)]
    pub execute_commands: bool,
    /// Always allow tool calls without confirmation (user opt-in)
    #[serde(default)]
    pub always_allow_tools: bool,
}

/// Main Agent block configuration
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub id: Uuid,

    #[builder(default = "Agent".to_string())]
    pub name: String,

    /// Where to send the prompt (ACP agent or internal)
    pub source: AgentSource,

    /// Prompt template (MiniJinja syntax supported)
    /// Example: "Analyze this data: {{ doc.named['query'].output.rows }}"
    pub prompt: String,

    /// Optional system prompt/instructions
    #[builder(default)]
    pub system_prompt: Option<String>,

    /// Capabilities granted to the agent
    #[builder(default)]
    pub capabilities: AgentCapabilities,

    /// Timeout in seconds (None = no timeout)
    #[builder(default)]
    pub timeout_seconds: Option<u32>,

    /// Working directory for the agent (defaults to runbook directory)
    #[builder(default)]
    pub working_directory: Option<String>,
}
```

### JSON Document Format

```json
{
  "id": "uuid-here",
  "type": "agent",
  "props": {
    "name": "Analyze Logs",
    "source": {
      "type": "acp",
      "command": "claude",
      "args": ["--dangerously-skip-permissions"],
      "env": {}
    },
    "prompt": "Analyze these error logs and identify patterns:\n\n{{ doc.named['fetch_logs'].output.stdout }}",
    "systemPrompt": "You are a log analysis expert. Be concise.",
    "capabilities": {
      "readFiles": true,
      "writeFiles": false,
      "executeCommands": false,
      "alwaysAllowTools": false
    },
    "timeoutSeconds": 300,
    "workingDirectory": null
  }
}
```

---

## Block State

Track execution progress for UI feedback.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentState {
    pub status: AgentStatus,
    /// Tokens consumed (if available from agent)
    pub token_usage: Option<TokenUsage>,
    /// Current tool call awaiting approval (if any)
    pub pending_tool_call: Option<PendingToolCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AgentStatus {
    #[default]
    Idle,
    /// Initializing connection to agent
    Initializing,
    /// Waiting for agent response
    Waiting,
    /// Streaming response from agent
    Streaming,
    /// Waiting for user to approve tool call
    PendingToolApproval,
    /// Agent is executing an approved tool
    ExecutingTool,
    /// Completed successfully
    Completed,
    /// Failed with error
    Failed,
    /// Cancelled by user
    Cancelled,
    /// Timed out
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingToolCall {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Human-readable summary of what the tool will do
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

impl BlockState for AgentState {}
```

---

## Block Output

Expose agent response for template access.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct AgentExecutionOutput {
    /// Full text response from the agent
    pub text: String,

    /// Duration of execution in seconds
    pub duration_seconds: f64,

    /// Whether the agent completed successfully
    pub success: bool,

    /// Error message if failed
    pub error: Option<String>,

    /// Token usage statistics (if available)
    pub token_usage: Option<TokenUsage>,

    /// Tool calls that were executed (for audit trail)
    pub tool_calls_executed: Vec<ExecutedToolCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutedToolCall {
    pub name: String,
    pub approved: bool,
    pub result_summary: Option<String>,
}

impl BlockExecutionOutput for AgentExecutionOutput {
    fn get_template_value(&self, key: &str) -> Option<minijinja::Value> {
        match key {
            "text" => Some(minijinja::Value::from(self.text.clone())),
            "success" => Some(minijinja::Value::from(self.success)),
            "error" => self.error.as_ref().map(|e| minijinja::Value::from(e.clone())),
            "durationSeconds" => Some(minijinja::Value::from(self.duration_seconds)),
            "inputTokens" => self.token_usage.as_ref()
                .and_then(|t| t.input_tokens)
                .map(|v| minijinja::Value::from(v as i64)),
            "outputTokens" => self.token_usage.as_ref()
                .and_then(|t| t.output_tokens)
                .map(|v| minijinja::Value::from(v as i64)),
            _ => None,
        }
    }

    fn enumerate_template_keys(&self) -> minijinja::value::Enumerator {
        minijinja::value::Enumerator::Str(&[
            "text",
            "success",
            "error",
            "durationSeconds",
            "inputTokens",
            "outputTokens",
        ])
    }
}
```

### Template Access Examples

```jinja
{# Access agent response text #}
{{ doc.named['analyze'].output.text }}

{# Check if successful #}
{% if doc.named['analyze'].output.success %}
  Analysis complete in {{ doc.named['analyze'].output.durationSeconds }}s
{% else %}
  Analysis failed: {{ doc.named['analyze'].output.error }}
{% endif %}

{# Token usage #}
Used {{ doc.named['analyze'].output.inputTokens }} input tokens
```

---

## Execution Flow

### ACP Agent Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                        Agent Block Execute                       │
├─────────────────────────────────────────────────────────────────┤
│  1. block_started()                                              │
│  2. Render prompt template with context                          │
│  3. Spawn agent subprocess (command + args)                      │
│  4. Initialize ACP connection (session/initialize)               │
│  5. Create session (session/new)                                 │
│  6. Send prompt (session/prompt)                                 │
│  7. Handle streaming response:                                   │
│     ├─ Text chunks → send_output(stdout)                        │
│     ├─ Tool calls → if always_allow: execute                    │
│     │               else: update_state(PendingToolApproval)     │
│     │                     wait for user approval                 │
│     │                     execute or reject tool                 │
│     └─ Continue until session complete                          │
│  8. Collect final response text                                  │
│  9. set_block_output(AgentExecutionOutput)                      │
│  10. send_output(object: full response JSON)                    │
│  11. block_finished()                                            │
└─────────────────────────────────────────────────────────────────┘
```

### Internal Agent Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                    Internal Agent Execute                        │
├─────────────────────────────────────────────────────────────────┤
│  1. block_started()                                              │
│  2. Render prompt template with context                          │
│  3. Create internal AI request (reuse existing AI infra)         │
│  4. Send to model via existing ModelSelection/provider system    │
│  5. Stream response chunks → send_output(stdout)                │
│  6. Tool calls: NOT SUPPORTED for internal agent in v1           │
│     (Internal agent is prompt-in, text-out only)                │
│  7. Collect final response text                                  │
│  8. set_block_output(AgentExecutionOutput)                      │
│  9. block_finished()                                             │
└─────────────────────────────────────────────────────────────────┘
```

---

## BlockBehavior Implementation

```rust
#[async_trait]
impl BlockBehavior for Agent {
    fn into_block(self) -> Block {
        Block::Agent(self)
    }

    fn id(&self) -> Uuid {
        self.id
    }

    fn create_state(&self) -> Option<Box<dyn BlockState>> {
        Some(Box::new(AgentState::default()))
    }

    async fn execute(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        let block_id = self.id;

        tokio::spawn(async move {
            let result = execute_agent(self, &context).await;

            match result {
                Ok(output) => {
                    let success = output.success;
                    let _ = context.set_block_output(output.clone()).await;
                    let _ = context.send_output(
                        StreamingBlockOutput::builder()
                            .block_id(block_id)
                            .object(serde_json::to_value(&output).ok())
                            .build()
                    ).await;
                    let _ = context.block_finished(None, success).await;
                }
                Err(e) => {
                    let _ = context.block_failed(e.to_string()).await;
                }
            }
        });

        Ok(Some(context.handle()))
    }
}

async fn execute_agent(
    agent: Agent,
    context: &ExecutionContext,
) -> Result<AgentExecutionOutput, Box<dyn std::error::Error + Send + Sync>> {
    let _ = context.block_started().await;

    // Update state to initializing
    context.update_block_state::<AgentState, _>(|state| {
        state.status = AgentStatus::Initializing;
    }).await;

    // Render the prompt template
    let rendered_prompt = context.render_template(&agent.prompt)?;
    let rendered_system = agent.system_prompt
        .as_ref()
        .map(|s| context.render_template(s))
        .transpose()?;

    let start_time = std::time::Instant::now();

    // Execute based on source type
    let result = match &agent.source {
        AgentSource::Acp { command, args, env } => {
            execute_acp_agent(
                context,
                command,
                args,
                env,
                &rendered_prompt,
                rendered_system.as_deref(),
                &agent.capabilities,
                agent.timeout_seconds,
                agent.working_directory.as_deref(),
            ).await
        }
        AgentSource::Internal { model } => {
            execute_internal_agent(
                context,
                model.as_ref(),
                &rendered_prompt,
                rendered_system.as_deref(),
                agent.timeout_seconds,
            ).await
        }
    };

    let duration = start_time.elapsed().as_secs_f64();

    match result {
        Ok((text, token_usage, tool_calls)) => {
            context.update_block_state::<AgentState, _>(|state| {
                state.status = AgentStatus::Completed;
                state.token_usage = token_usage.clone();
            }).await;

            Ok(AgentExecutionOutput::builder()
                .text(text)
                .duration_seconds(duration)
                .success(true)
                .error(None)
                .token_usage(token_usage)
                .tool_calls_executed(tool_calls)
                .build())
        }
        Err(e) => {
            context.update_block_state::<AgentState, _>(|state| {
                state.status = AgentStatus::Failed;
            }).await;

            Ok(AgentExecutionOutput::builder()
                .text(String::new())
                .duration_seconds(duration)
                .success(false)
                .error(Some(e.to_string()))
                .token_usage(None)
                .tool_calls_executed(vec![])
                .build())
        }
    }
}
```

---

## ACP Client Implementation

### Dependencies

Add to `Cargo.toml`:

```toml
[dependencies]
agent-client-protocol = "0.10"  # Official ACP Rust SDK
```

### ACP Execution

```rust
use agent_client_protocol as acp;

async fn execute_acp_agent(
    context: &ExecutionContext,
    command: &str,
    args: &[String],
    env: &std::collections::HashMap<String, String>,
    prompt: &str,
    system_prompt: Option<&str>,
    capabilities: &AgentCapabilities,
    timeout_seconds: Option<u32>,
    working_directory: Option<&str>,
) -> Result<(String, Option<TokenUsage>, Vec<ExecutedToolCall>), Box<dyn std::error::Error + Send + Sync>> {

    // 1. Spawn agent subprocess
    let mut cmd = tokio::process::Command::new(command);
    cmd.args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .envs(env);

    if let Some(cwd) = working_directory {
        cmd.current_dir(cwd);
    }

    let mut child = cmd.spawn()?;

    let stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");

    // 2. Create ACP connection
    let client = AgentBlockClient::new(context.clone(), capabilities.clone());
    let (reader, writer) = /* wrap stdin/stdout for async */;

    let conn = acp::ClientSideConnection::new(client, reader, writer);

    // 3. Initialize
    context.update_block_state::<AgentState, _>(|state| {
        state.status = AgentStatus::Initializing;
    }).await;

    let init_response = conn.initialize(acp::InitializeRequest {
        protocol_version: acp::V1,
        client_capabilities: build_client_capabilities(capabilities),
        client_info: acp::Implementation {
            name: "atuin-runbook".to_string(),
            title: Some("Atuin Runbook".to_string()),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
    }).await?;

    // 4. Create session
    let session = conn.new_session(acp::NewSessionRequest {
        // Include MCP servers if relevant
        mcp_servers: vec![],
        working_directory: working_directory.map(|s| s.to_string()),
        ..Default::default()
    }).await?;

    // 5. Send prompt
    context.update_block_state::<AgentState, _>(|state| {
        state.status = AgentStatus::Waiting;
    }).await;

    conn.prompt(acp::PromptRequest {
        session_id: session.session_id,
        prompt: prompt.to_string(),
        system_prompt: system_prompt.map(|s| s.to_string()),
    }).await?;

    // 6. Collect response (streaming handled via client callbacks)
    // The client implementation handles:
    // - Streaming text chunks → context.send_output()
    // - Tool call requests → approval flow
    // - Final completion

    // 7. Wait for completion or timeout
    let result = if let Some(timeout) = timeout_seconds {
        tokio::time::timeout(
            std::time::Duration::from_secs(timeout as u64),
            conn.wait_for_completion()
        ).await.map_err(|_| "Agent timed out")?
    } else {
        conn.wait_for_completion().await
    };

    // 8. Clean up
    let _ = child.kill().await;

    result
}

/// ACP Client implementation for handling agent requests
struct AgentBlockClient {
    context: ExecutionContext,
    capabilities: AgentCapabilities,
    response_text: std::sync::Arc<tokio::sync::Mutex<String>>,
    tool_calls: std::sync::Arc<tokio::sync::Mutex<Vec<ExecutedToolCall>>>,
}

impl acp::Client for AgentBlockClient {
    // Handle streaming text chunks
    async fn session_notification(&self, notification: acp::SessionNotification) {
        if let Some(content) = notification.text_chunk {
            // Append to response
            self.response_text.lock().await.push_str(&content);

            // Stream to UI
            let _ = self.context.send_output(
                StreamingBlockOutput::builder()
                    .block_id(self.context.block_id)
                    .stdout(Some(content))
                    .build()
            ).await;

            // Update state
            self.context.update_block_state::<AgentState, _>(|state| {
                state.status = AgentStatus::Streaming;
            }).await;
        }
    }

    // Handle tool call permission requests
    async fn request_tool_permission(
        &self,
        request: acp::ToolPermissionRequest,
    ) -> Result<acp::ToolPermissionResponse, acp::Error> {
        // If always_allow is enabled, auto-approve
        if self.capabilities.always_allow_tools {
            return Ok(acp::ToolPermissionResponse { approved: true });
        }

        // Check if tool matches granted capabilities
        let auto_approve = match request.tool_name.as_str() {
            name if name.contains("read") => self.capabilities.read_files,
            name if name.contains("write") => self.capabilities.write_files,
            name if name.contains("exec") || name.contains("bash") || name.contains("shell")
                => self.capabilities.execute_commands,
            _ => false,
        };

        if auto_approve {
            return Ok(acp::ToolPermissionResponse { approved: true });
        }

        // Request user approval
        self.context.update_block_state::<AgentState, _>(|state| {
            state.status = AgentStatus::PendingToolApproval;
            state.pending_tool_call = Some(PendingToolCall {
                id: request.call_id.clone(),
                name: request.tool_name.clone(),
                description: request.description.unwrap_or_default(),
                summary: format!("Agent wants to: {}", request.tool_name),
            });
        }).await;

        // Emit event for UI to show approval dialog
        let _ = self.context.emit_gc_event(GCEvent::AgentToolApprovalRequired {
            block_id: self.context.block_id,
            runbook_id: self.context.runbook_id,
            tool_call_id: request.call_id.clone(),
            tool_name: request.tool_name.clone(),
            description: request.description,
        }).await;

        // Wait for user response (via channel or similar mechanism)
        let approved = self.wait_for_tool_approval(&request.call_id).await;

        // Record the tool call
        self.tool_calls.lock().await.push(ExecutedToolCall {
            name: request.tool_name,
            approved,
            result_summary: None,
        });

        // Clear pending state
        self.context.update_block_state::<AgentState, _>(|state| {
            state.status = if approved {
                AgentStatus::ExecutingTool
            } else {
                AgentStatus::Waiting
            };
            state.pending_tool_call = None;
        }).await;

        Ok(acp::ToolPermissionResponse { approved })
    }

    // File system capabilities (delegate to context/environment)
    async fn read_text_file(
        &self,
        request: acp::ReadTextFileRequest,
    ) -> Result<acp::ReadTextFileResponse, acp::Error> {
        if !self.capabilities.read_files {
            return Err(acp::Error::method_not_found());
        }
        // Implement file reading via context
        todo!("Implement file reading")
    }

    async fn write_text_file(
        &self,
        request: acp::WriteTextFileRequest,
    ) -> Result<acp::WriteTextFileResponse, acp::Error> {
        if !self.capabilities.write_files {
            return Err(acp::Error::method_not_found());
        }
        // Implement file writing via context
        todo!("Implement file writing")
    }
}
```

---

## Internal Agent Implementation

```rust
async fn execute_internal_agent(
    context: &ExecutionContext,
    model: Option<&ModelSelection>,
    prompt: &str,
    system_prompt: Option<&str>,
    timeout_seconds: Option<u32>,
) -> Result<(String, Option<TokenUsage>, Vec<ExecutedToolCall>), Box<dyn std::error::Error + Send + Sync>> {

    context.update_block_state::<AgentState, _>(|state| {
        state.status = AgentStatus::Waiting;
    }).await;

    // Build messages for the AI request
    let mut messages = Vec::new();

    if let Some(system) = system_prompt {
        messages.push(AIMessage {
            role: Role::System,
            content: vec![ContentPart::Text { text: system.to_string() }],
        });
    }

    messages.push(AIMessage {
        role: Role::User,
        content: vec![ContentPart::Text { text: prompt.to_string() }],
    });

    // Get model selection (use provided or fall back to workspace/global default)
    let model_selection = model.cloned()
        .or_else(|| context.get_default_model_selection())
        .ok_or("No model configured")?;

    // Create streaming request using existing AI infrastructure
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);

    // Spawn the AI request (reuse existing provider infrastructure)
    let request_handle = spawn_ai_request(
        model_selection,
        messages,
        tx,
        timeout_seconds,
    ).await?;

    // Collect streaming response
    let mut response_text = String::new();
    let mut token_usage = None;

    while let Some(event) = rx.recv().await {
        match event {
            AIStreamEvent::Chunk { content } => {
                response_text.push_str(&content);

                // Stream to UI
                let _ = context.send_output(
                    StreamingBlockOutput::builder()
                        .block_id(context.block_id)
                        .stdout(Some(content))
                        .build()
                ).await;

                context.update_block_state::<AgentState, _>(|state| {
                    state.status = AgentStatus::Streaming;
                }).await;
            }
            AIStreamEvent::Complete { usage } => {
                token_usage = usage.map(|u| TokenUsage {
                    input_tokens: Some(u.input_tokens),
                    output_tokens: Some(u.output_tokens),
                });
                break;
            }
            AIStreamEvent::Error { message } => {
                return Err(message.into());
            }
        }
    }

    // Internal agent doesn't support tool calls in v1
    Ok((response_text, token_usage, vec![]))
}
```

---

## FromDocument Implementation

```rust
impl FromDocument for Agent {
    fn from_document(block_data: &serde_json::Value) -> Result<Self, String> {
        let block_id = block_data
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or("Missing block id")?;

        let props = block_data
            .get("props")
            .and_then(|p| p.as_object())
            .ok_or("Missing props")?;

        let id = Uuid::parse_str(block_id)
            .map_err(|e| format!("Invalid block id: {}", e))?;

        // Parse source
        let source = props
            .get("source")
            .ok_or("Missing source")?;

        let source: AgentSource = serde_json::from_value(source.clone())
            .map_err(|e| format!("Invalid source: {}", e))?;

        // Parse capabilities
        let capabilities = props
            .get("capabilities")
            .map(|c| serde_json::from_value(c.clone()))
            .transpose()
            .map_err(|e| format!("Invalid capabilities: {}", e))?
            .unwrap_or_default();

        let agent = Agent::builder()
            .id(id)
            .name(
                props.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Agent")
                    .to_string()
            )
            .source(source)
            .prompt(
                props.get("prompt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            )
            .system_prompt(
                props.get("systemPrompt")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            )
            .capabilities(capabilities)
            .timeout_seconds(
                props.get("timeoutSeconds")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32)
            )
            .working_directory(
                props.get("workingDirectory")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            )
            .build();

        Ok(agent)
    }
}
```

---

## Block Registration

### Update Block Enum

**File:** `crates/atuin-desktop-runtime/src/blocks/mod.rs`

```rust
// Add to Block enum
pub enum Block {
    // ... existing variants ...
    Agent(agent::Agent),
}

// Add to Block::from_document match
"agent" => Ok(Block::Agent(agent::Agent::from_document(block_data)?)),

// Add to Block::id() match
Block::Agent(b) => b.id(),

// Add to Block::name() match
Block::Agent(b) => &b.name,

// Add to Block::execute() match
Block::Agent(b) => b.execute(context).await,

// Add module declaration
pub mod agent;
```

---

## Grand Central Events

Add new event type for tool approval flow.

**File:** `crates/atuin-desktop-runtime/src/gc/events.rs`

```rust
// Add to GCEvent enum
pub enum GCEvent {
    // ... existing variants ...

    /// Agent is requesting approval for a tool call
    AgentToolApprovalRequired {
        block_id: Uuid,
        runbook_id: Uuid,
        tool_call_id: String,
        tool_name: String,
        description: Option<String>,
    },

    /// User responded to tool approval request
    AgentToolApprovalResponse {
        block_id: Uuid,
        tool_call_id: String,
        approved: bool,
        /// If true, auto-approve this tool for rest of session
        remember: bool,
    },
}
```

---

## TypeScript Types

Generated automatically via ts-rs, but for reference:

**File:** `src/rs-bindings/AgentBlock.ts`

```typescript
export interface AgentSource {
  type: "acp" | "internal";
  // For ACP
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  // For Internal
  model?: ModelSelection;
}

export interface AgentCapabilities {
  readFiles: boolean;
  writeFiles: boolean;
  executeCommands: boolean;
  alwaysAllowTools: boolean;
}

export interface AgentState {
  status: AgentStatus;
  tokenUsage: TokenUsage | null;
  pendingToolCall: PendingToolCall | null;
}

export type AgentStatus =
  | "idle"
  | "initializing"
  | "waiting"
  | "streaming"
  | "pendingToolApproval"
  | "executingTool"
  | "completed"
  | "failed"
  | "cancelled"
  | "timedOut";

export interface PendingToolCall {
  id: string;
  name: string;
  description: string;
  summary: string;
}

export interface AgentExecutionOutput {
  text: string;
  durationSeconds: number;
  success: boolean;
  error: string | null;
  tokenUsage: TokenUsage | null;
  toolCallsExecuted: ExecutedToolCall[];
}

export interface ExecutedToolCall {
  name: string;
  approved: boolean;
  resultSummary: string | null;
}
```

---

## UI Components Required

### 1. Block Config Panel

**File:** `src/components/runbooks/editor/blocks/AgentBlockConfig.tsx`

- Source selector (ACP vs Internal)
- For ACP: command input, args array, env vars
- For Internal: model selector (reuse existing)
- Prompt textarea (with template variable autocomplete)
- System prompt textarea (optional)
- Capabilities checkboxes
- Timeout input
- Working directory input

### 2. Block Execution View

**File:** `src/components/runbooks/editor/blocks/AgentBlockView.tsx`

- Status indicator (icon + text for each AgentStatus)
- Streaming response display (monospace, scrolling)
- Token usage badge (if available)
- Duration display

### 3. Tool Approval Dialog

**File:** `src/components/runbooks/editor/ui/AgentToolApprovalDialog.tsx`

- Tool name and description
- "Approve" / "Deny" buttons
- "Always allow this tool" checkbox
- Auto-dismiss on timeout (deny)

---

## Global Settings

Add agent configuration to global settings.

**File:** Settings schema

```typescript
interface AgentSettings {
  /** Available ACP agents */
  agents: {
    [key: string]: {
      command: string;
      args?: string[];
      description?: string;
    };
  };

  /** Default timeout for agent blocks (seconds) */
  defaultTimeout: number;

  /** Default capabilities for new agent blocks */
  defaultCapabilities: AgentCapabilities;
}

// Example default settings
const defaultAgentSettings: AgentSettings = {
  agents: {
    "claude": {
      command: "claude",
      description: "Claude Code (Anthropic)"
    },
    "gemini": {
      command: "gemini",
      description: "Gemini CLI (Google)"
    },
    "codex": {
      command: "codex",
      description: "Codex CLI (OpenAI)"
    }
  },
  defaultTimeout: 300,
  defaultCapabilities: {
    readFiles: true,
    writeFiles: false,
    executeCommands: false,
    alwaysAllowTools: false
  }
};
```

---

## Testing Strategy

### Unit Tests

1. `Agent::from_document()` - Parse various JSON configs
2. `AgentExecutionOutput` template value access
3. Capability matching logic for auto-approve

### Integration Tests

1. Internal agent execution (mock AI provider)
2. ACP connection lifecycle (mock subprocess)
3. Tool approval flow (GC event emission and response)
4. Timeout handling

### Manual Testing

1. Claude Code integration (requires claude CLI installed)
2. Streaming response display
3. Tool approval dialog UX
4. Cancellation during execution

---

## Implementation Order

1. **Backend Core**
   - [ ] `agent.rs` - Structs, FromDocument, BlockBehavior scaffold
   - [ ] Register in Block enum and dispatcher
   - [ ] AgentState and AgentExecutionOutput

2. **Internal Agent**
   - [ ] `execute_internal_agent()` using existing AI infra
   - [ ] Streaming to UI

3. **ACP Integration**
   - [ ] Add `agent-client-protocol` dependency
   - [ ] `AgentBlockClient` implementation
   - [ ] `execute_acp_agent()` subprocess management

4. **Tool Approval Flow**
   - [ ] GC events for approval request/response
   - [ ] Channel-based wait for approval
   - [ ] Capability-based auto-approve

5. **Frontend**
   - [ ] TypeScript types (auto-generated)
   - [ ] Block config component
   - [ ] Block execution view
   - [ ] Tool approval dialog

6. **Settings**
   - [ ] Agent settings schema
   - [ ] Settings UI for agent configuration

---

## Open Questions for Implementer

1. **MCP Server passthrough** - Should agent blocks pass through MCP servers configured in Atuin to the ACP agent? (e.g., database access)

2. **Session persistence** - Should ACP sessions be resumable across runbook executions, or always fresh?

3. **Stderr handling** - Stream agent stderr to block output, or capture separately for debugging?

4. **Working directory** - Default to runbook file location, workspace root, or require explicit config?

---

## References

- [Agent Client Protocol GitHub](https://github.com/agentclientprotocol/agent-client-protocol)
- [ACP Rust SDK](https://github.com/agentclientprotocol/rust-sdk)
- [Crates.io: agent-client-protocol](https://crates.io/crates/agent-client-protocol)
- Existing block implementations: `http.rs`, `sub_runbook.rs`, `script.rs`
- Existing AI infrastructure: `backend/src/ai/`
