# Agent Block Specification

## Overview

The Agent block invokes Atuin's internal AI agent with a templated prompt and captures the response as output for downstream blocks.

---

## Existing Infrastructure to Reuse

**Reuse existing AI infrastructure. Don't rebuild.**

| Component | Location | Reuse |
|-----------|----------|-------|
| **FSM Engine** | `backend/src/ai/fsm.rs` | As-is |
| **Session Driver** | `backend/src/ai/session.rs` | Add `new_for_agent_block()`, `wait_for_idle()` |
| **Streaming** | `backend/src/ai/session.rs:523` | As-is |
| **Multi-Provider** | `ModelSelection` enum | As-is |
| **Commands** | `backend/src/commands/ai.rs` | Add `ai_agent_execute()` |

---

## Block Type

```
type: "agent"
```

---

## Configuration

**File:** `crates/atuin-desktop-runtime/src/blocks/agent.rs`

```rust
#[derive(Debug, Serialize, Deserialize, Clone, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub id: Uuid,

    #[builder(default = "Agent".to_string())]
    pub name: String,

    /// Model selection (defaults to workspace/global setting if None)
    pub model: Option<ModelSelection>,

    /// Prompt template (MiniJinja syntax)
    pub prompt: String,

    /// Optional system prompt
    #[builder(default)]
    pub system_prompt: Option<String>,

    /// Auto-approve all tool calls without confirmation
    #[builder(default)]
    pub always_allow_tools: bool,

    /// Timeout in seconds (None = no timeout)
    #[builder(default)]
    pub timeout_seconds: Option<u32>,
}
```

### JSON Format

```json
{
  "id": "uuid-here",
  "type": "agent",
  "props": {
    "name": "Analyze Logs",
    "model": null,
    "prompt": "Analyze these logs:\n\n{{ doc.named['fetch_logs'].output.stdout }}",
    "systemPrompt": "Be concise.",
    "alwaysAllowTools": false,
    "timeoutSeconds": 300
  }
}
```

---

## Block State

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentState {
    pub status: AgentStatus,
    pub token_usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AgentStatus {
    #[default]
    Idle,
    Waiting,
    Streaming,
    PendingToolApproval,
    Completed,
    Failed,
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

impl BlockState for AgentState {}
```

---

## Block Output

```rust
#[derive(Debug, Clone, Serialize, Deserialize, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct AgentExecutionOutput {
    pub text: String,
    pub duration_seconds: f64,
    pub success: bool,
    pub error: Option<String>,
    pub token_usage: Option<TokenUsage>,
}

impl BlockExecutionOutput for AgentExecutionOutput {
    fn get_template_value(&self, key: &str) -> Option<minijinja::Value> {
        match key {
            "text" => Some(minijinja::Value::from(self.text.clone())),
            "success" => Some(minijinja::Value::from(self.success)),
            "error" => self.error.as_ref().map(|e| minijinja::Value::from(e.clone())),
            "durationSeconds" => Some(minijinja::Value::from(self.duration_seconds)),
            _ => None,
        }
    }
}
```

### Template Access

```jinja
{{ doc.named['analyze'].output.text }}
{{ doc.named['analyze'].output.success }}
{{ doc.named['analyze'].output.error }}
```

---

## Execution

```rust
#[async_trait]
impl BlockBehavior for Agent {
    fn into_block(self) -> Block { Block::Agent(self) }
    fn id(&self) -> Uuid { self.id }
    fn create_state(&self) -> Option<Box<dyn BlockState>> {
        Some(Box::new(AgentState::default()))
    }

    async fn execute(self, context: ExecutionContext) -> Result<Option<ExecutionHandle>, _> {
        let block_id = self.id;

        tokio::spawn(async move {
            let _ = context.block_started().await;

            context.update_block_state::<AgentState, _>(|s| {
                s.status = AgentStatus::Waiting;
            }).await;

            let prompt = context.render_template(&self.prompt)?;
            let system_prompt = self.system_prompt.as_ref()
                .map(|s| context.render_template(s)).transpose()?;

            let start = std::time::Instant::now();

            let result = ai_agent_execute(
                context.runbook_id,
                prompt,
                system_prompt,
                self.model,
                self.always_allow_tools,
                self.timeout_seconds,
                |chunk| {
                    context.send_output(StreamingBlockOutput::builder()
                        .block_id(block_id)
                        .stdout(Some(chunk))
                        .build()
                    );
                    context.update_block_state::<AgentState, _>(|s| {
                        s.status = AgentStatus::Streaming;
                    });
                },
            ).await;

            let duration = start.elapsed().as_secs_f64();

            let output = match result {
                Ok(r) => {
                    context.update_block_state::<AgentState, _>(|s| {
                        s.status = AgentStatus::Completed;
                        s.token_usage = r.token_usage.clone();
                    }).await;
                    AgentExecutionOutput {
                        text: r.text,
                        duration_seconds: duration,
                        success: true,
                        error: None,
                        token_usage: r.token_usage,
                    }
                }
                Err(e) => {
                    context.update_block_state::<AgentState, _>(|s| {
                        s.status = AgentStatus::Failed;
                    }).await;
                    AgentExecutionOutput {
                        text: String::new(),
                        duration_seconds: duration,
                        success: false,
                        error: Some(e.to_string()),
                        token_usage: None,
                    }
                }
            };

            let _ = context.set_block_output(output.clone()).await;
            let _ = context.block_finished(None, output.success).await;
        });

        Ok(Some(context.handle()))
    }
}
```

---

## Backend Changes

### 1. New Tauri Command

**File:** `backend/src/commands/ai.rs`

```rust
#[tauri::command]
pub async fn ai_agent_execute(
    state: tauri::State<'_, AtuinState>,
    runbook_id: Uuid,
    prompt: String,
    system_prompt: Option<String>,
    model: Option<ModelSelection>,
    always_allow_tools: bool,
    timeout_seconds: Option<u32>,
    on_chunk: Channel<String>,
) -> Result<AgentResult, String> {
    let model = model
        .or_else(|| get_workspace_default_model(runbook_id))
        .or_else(|| get_global_default_model())
        .ok_or("No model configured")?;

    let session = AISession::new_for_agent_block(
        runbook_id,
        model,
        system_prompt,
        always_allow_tools,
    ).await?;

    // Forward chunks
    let rx = session.subscribe();
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if let SessionEvent::Chunk { content } = event {
                let _ = on_chunk.send(content);
            }
        }
    });

    session.send_message(prompt).await?;

    let result = match timeout_seconds {
        Some(t) => tokio::time::timeout(
            Duration::from_secs(t as u64),
            session.wait_for_idle()
        ).await.map_err(|_| "Timed out")?,
        None => session.wait_for_idle().await,
    }?;

    Ok(AgentResult {
        text: result.final_response,
        token_usage: result.token_usage,
    })
}
```

### 2. Session Extension

**File:** `backend/src/ai/session.rs`

```rust
impl AISession {
    pub async fn new_for_agent_block(
        runbook_id: Uuid,
        model: ModelSelection,
        system_prompt: Option<String>,
        always_allow_tools: bool,
    ) -> Result<Self, Error> {
        let mut session = Self::new(runbook_id, model).await?;

        if let Some(prompt) = system_prompt {
            session.set_system_prompt(prompt);
        }

        if always_allow_tools {
            session.set_auto_approve_all_tools(true);
        }

        // Read-only tools only
        session.set_available_tools(vec![
            "get_runbook_document",
            "get_block_docs",
        ]);

        Ok(session)
    }

    pub async fn wait_for_idle(&self) -> Result<CompletedResponse, Error> {
        loop {
            match self.get_state().await {
                State::Idle => return Ok(self.get_completed_response().await),
                State::PendingTools => self.wait_for_tool_resolution().await?,
                _ => self.wait_for_state_change().await,
            }
        }
    }
}
```

### 3. FSM Extension

**File:** `backend/src/ai/fsm.rs`

```rust
pub struct Agent {
    pub state: State,
    pub context: Context,
    pub auto_approve_all_tools: bool,  // NEW
}

impl Agent {
    fn should_auto_approve_tool(&self, _tool_name: &str) -> bool {
        self.auto_approve_all_tools
    }
}
```

---

## Block Registration

**File:** `crates/atuin-desktop-runtime/src/blocks/mod.rs`

```rust
pub enum Block {
    // ... existing ...
    Agent(agent::Agent),
}

// Add match arms:
"agent" => Ok(Block::Agent(agent::Agent::from_document(block_data)?)),
Block::Agent(b) => b.id(),
Block::Agent(b) => &b.name,
Block::Agent(b) => b.execute(context).await,

pub mod agent;
```

---

## FromDocument

```rust
impl FromDocument for Agent {
    fn from_document(block_data: &serde_json::Value) -> Result<Self, String> {
        let id = block_data.get("id").and_then(|v| v.as_str())
            .ok_or("Missing id")?;
        let props = block_data.get("props").and_then(|p| p.as_object())
            .ok_or("Missing props")?;

        Ok(Agent::builder()
            .id(Uuid::parse_str(id).map_err(|e| e.to_string())?)
            .name(props.get("name").and_then(|v| v.as_str()).unwrap_or("Agent").to_string())
            .model(props.get("model").and_then(|v| serde_json::from_value(v.clone()).ok()))
            .prompt(props.get("prompt").and_then(|v| v.as_str()).unwrap_or("").to_string())
            .system_prompt(props.get("systemPrompt").and_then(|v| v.as_str()).map(String::from))
            .always_allow_tools(props.get("alwaysAllowTools").and_then(|v| v.as_bool()).unwrap_or(false))
            .timeout_seconds(props.get("timeoutSeconds").and_then(|v| v.as_u64()).map(|v| v as u32))
            .build())
    }
}
```

---

## Frontend

### TypeScript Types

```typescript
export interface AgentState {
  status: "idle" | "waiting" | "streaming" | "pendingToolApproval" | "completed" | "failed" | "timedOut";
  tokenUsage: { inputTokens?: number; outputTokens?: number } | null;
}

export interface AgentExecutionOutput {
  text: string;
  durationSeconds: number;
  success: boolean;
  error: string | null;
  tokenUsage: { inputTokens?: number; outputTokens?: number } | null;
}
```

### UI Components

**AgentBlockConfig.tsx:**
- Model selector (reuse existing)
- Prompt textarea with template autocomplete
- System prompt textarea
- "Always allow tools" checkbox
- Timeout input

**AgentBlockView.tsx:**
- Status indicator
- Streaming response display
- Token usage badge
- Duration display

---

## Implementation Checklist

- [ ] `agent.rs` - `Agent`, `AgentState`, `AgentExecutionOutput`
- [ ] `FromDocument` implementation
- [ ] Register in `Block` enum
- [ ] `ai/session.rs` - `new_for_agent_block()`, `wait_for_idle()`
- [ ] `ai/fsm.rs` - `auto_approve_all_tools` field
- [ ] `commands/ai.rs` - `ai_agent_execute()` command
- [ ] TypeScript types (auto-generated)
- [ ] `AgentBlockConfig.tsx`
- [ ] `AgentBlockView.tsx`
- [ ] Reuse existing tool approval from AIAssistant
- [ ] Unit tests
- [ ] Manual testing

---

## References

- Existing blocks: `http.rs`, `sub_runbook.rs`, `script.rs`
- Existing AI: `backend/src/ai/`
