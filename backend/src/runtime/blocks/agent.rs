use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex as TokioMutex, RwLock};
use typed_builder::TypedBuilder;
use uuid::Uuid;

use agents_sdk::{
    agent::AgentHandle, persistence::InMemoryCheckpointer, state::AgentStateSnapshot, tool,
    ConfigurableAgentBuilder, OpenAiChatModel, OpenAiConfig,
};

// Required for #[tool] macro
extern crate agents_core;
extern crate anyhow;
extern crate async_trait;

use crate::commands::agent::{AgentSessionRegistry, HitlRequest, HitlResponse};
use crate::runtime::blocks::document::bridge::AgentUiEvent;
use crate::runtime::blocks::handler::{
    BlockErrorData, BlockFinishedData, BlockLifecycleEvent, BlockOutput, CancellationToken,
    ExecutionContext, ExecutionHandle, ExecutionStatus,
};
use crate::runtime::blocks::{Block, BlockBehavior};
use crate::runtime::events::GCEvent;
use crate::runtime::workflow::event::WorkflowEvent;

use super::FromDocument;

// ============================================================================
// Agent Block Definition
// ============================================================================

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    #[builder(setter(into))]
    pub id: Uuid,
}

impl FromDocument for Agent {
    fn from_document(block_data: &serde_json::Value) -> Result<Self, String> {
        let block_id = block_data
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or("Block has no id")?;

        let id = Uuid::parse_str(block_id).map_err(|e| e.to_string())?;

        Ok(Agent::builder().id(id).build())
    }
}

#[async_trait::async_trait]
impl BlockBehavior for Agent {
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::Agent(self)
    }

    async fn execute(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        log::trace!("Executing Agent block {id}", id = self.id);

        let handle = ExecutionHandle {
            id: self.id,
            block_id: self.id,
            cancellation_token: CancellationToken::new(),
            status: Arc::new(RwLock::new(ExecutionStatus::Running)),
            output_variable: None,
        };

        // Send started event
        let _ = context.emit_workflow_event(WorkflowEvent::BlockStarted { id: self.id });
        let _ = context
            .send_output(
                BlockOutput::builder()
                    .block_id(self.id)
                    .lifecycle(BlockLifecycleEvent::Started)
                    .build(),
            )
            .await;

        // Emit BlockStarted event via Grand Central
        if let Some(event_bus) = &context.gc_event_bus {
            let _ = event_bus
                .emit(GCEvent::BlockStarted {
                    block_id: self.id,
                    runbook_id: context.runbook_id,
                })
                .await;
        }

        let handle_clone = handle.clone();
        let block_id = self.id;

        // Get agent session registry from context
        let registry = context
            .agent_session_registry
            .clone()
            .ok_or("Agent session registry not available in execution context")?;

        // Spawn agent session
        tokio::spawn(async move {
            let result = run_agent_session(
                block_id,
                context.clone(),
                handle_clone.cancellation_token.clone(),
                registry,
            )
            .await;

            let status = match result {
                Ok(()) => {
                    // Emit BlockFinished event via Grand Central
                    if let Some(event_bus) = &context.gc_event_bus {
                        let _ = event_bus
                            .emit(GCEvent::BlockFinished {
                                block_id,
                                runbook_id: context.runbook_id,
                                success: true,
                            })
                            .await;
                    }

                    let _ = context
                        .send_output(
                            BlockOutput::builder()
                                .block_id(block_id)
                                .lifecycle(BlockLifecycleEvent::Finished(BlockFinishedData {
                                    exit_code: None,
                                    success: true,
                                }))
                                .build(),
                        )
                        .await;

                    ExecutionStatus::Success("Agent session completed".to_string())
                }
                Err(e) => {
                    log::error!("Agent session error: {}", e);

                    // Emit BlockFailed event via Grand Central
                    if let Some(event_bus) = &context.gc_event_bus {
                        let _ = event_bus
                            .emit(GCEvent::BlockFailed {
                                block_id,
                                runbook_id: context.runbook_id,
                                error: e.to_string(),
                            })
                            .await;
                    }

                    let _ = context
                        .send_output(
                            BlockOutput::builder()
                                .block_id(block_id)
                                .lifecycle(BlockLifecycleEvent::Error(BlockErrorData {
                                    message: e.to_string(),
                                }))
                                .build(),
                        )
                        .await;

                    ExecutionStatus::Failed(e.to_string())
                }
            };

            *handle_clone.status.write().await = status;
        });

        Ok(Some(handle))
    }
}

// ============================================================================
// Agent Session State
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentSessionState {
    messages: Vec<Message>,
    pending_hitl: Option<HitlState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HitlState {
    id: String,
    prompt: String,
    options: serde_json::Value,
}

// ============================================================================
// Agent Session Runner
// ============================================================================

async fn run_agent_session(
    block_id: Uuid,
    context: ExecutionContext,
    cancellation_token: CancellationToken,
    registry: Arc<AgentSessionRegistry>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    log::debug!("Starting agent session for block {}", block_id);

    // Create channels for agent communication
    let (user_message_tx, mut user_message_rx) = mpsc::channel::<String>(10);
    let (hitl_request_tx, _hitl_request_rx) = mpsc::channel::<HitlRequest>(10);
    let hitl_responses: Arc<TokioMutex<HashMap<String, oneshot::Sender<HitlResponse>>>> =
        Arc::new(TokioMutex::new(HashMap::new()));

    // Register agent session in global registry so API endpoints can send messages
    registry
        .register(
            context.runbook_id,
            block_id,
            user_message_tx.clone(),
            hitl_request_tx.clone(),
            hitl_responses.clone(),
        )
        .await;

    // Load or initialize session state from passive context
    let initial_state = load_session_state(&context, block_id).await?;
    let messages = Arc::new(TokioMutex::new(initial_state.messages));

    // Create agent tools - DISABLED until SDK tool format is fixed
    // let tools = create_agent_tools();
    let tools = Vec::new();

    // Build agent
    log::debug!("Building agent for block {}", block_id);
    let agent = match build_agent(tools, &context, block_id).await {
        Ok(agent) => {
            log::debug!("Agent built successfully for block {}", block_id);
            agent
        }
        Err(e) => {
            log::error!("Failed to build agent: {}", e);
            registry.unregister(context.runbook_id, block_id).await;
            return Err(format!("Failed to build agent: {}", e).into());
        }
    };

    log::info!(
        "Agent session ready for block {} - waiting for messages",
        block_id
    );

    // Main agent loop - wait for user messages
    let cancellation_receiver = cancellation_token.take_receiver();
    if let Some(mut cancel_rx) = cancellation_receiver {
        loop {
            tokio::select! {
                // Handle user messages
                Some(user_message) = user_message_rx.recv() => {
                    log::debug!("Received user message: {}", user_message);

                    // Add user message to history
                    {
                        let mut msgs = messages.lock().await;
                        msgs.push(Message {
                            role: "user".to_string(),
                            content: user_message.clone(),
                        });
                    }

                    // Process message with agent
                    match process_agent_message(&agent, &user_message, &context, block_id, messages.clone()).await {
                        Ok(response) => {
                            // Add assistant response to history
                            {
                                let mut msgs = messages.lock().await;
                                msgs.push(Message {
                                    role: "assistant".to_string(),
                                    content: response.clone(),
                                });
                            }

                            // Send final assistant message
                            let event_data = serde_json::to_value(AgentUiEvent::AssistantMessage { text: response })
                                .unwrap_or_default();
                            let _ = context
                                .send_output(
                                    BlockOutput::builder()
                                        .block_id(block_id)
                                        .object(event_data)
                                        .build(),
                                )
                                .await;

                            // Save state
                            save_session_state(&context, block_id, messages.clone()).await?;
                        }
                        Err(e) => {
                            log::error!("Agent processing error: {}", e);
                            let event_data = serde_json::to_value(AgentUiEvent::AssistantMessage {
                                text: format!("Error: {}", e)
                            })
                            .unwrap_or_default();
                            let _ = context
                                .send_output(
                                    BlockOutput::builder()
                                        .block_id(block_id)
                                        .object(event_data)
                                        .build(),
                                )
                                .await;
                        }
                    }
                }

                // Handle cancellation
                _ = &mut cancel_rx => {
                    log::debug!("Agent session cancelled for block {}", block_id);

                    // Emit BlockCancelled event
                    if let Some(event_bus) = &context.gc_event_bus {
                        let _ = event_bus
                            .emit(GCEvent::BlockCancelled {
                                block_id,
                                runbook_id: context.runbook_id,
                            })
                            .await;
                    }

                    let _ = context
                        .send_output(
                            BlockOutput::builder()
                                .block_id(block_id)
                                .lifecycle(BlockLifecycleEvent::Cancelled)
                                .build(),
                        )
                        .await;

                    // Unregister from session registry
                    registry.unregister(context.runbook_id, block_id).await;
                    return Ok(());
                }
            }
        }
    } else {
        // No cancellation support, run forever
        loop {
            if let Some(user_message) = user_message_rx.recv().await {
                log::debug!("Received user message: {}", user_message);

                // Add user message to history
                {
                    let mut msgs = messages.lock().await;
                    msgs.push(Message {
                        role: "user".to_string(),
                        content: user_message.clone(),
                    });
                }

                // Process message with agent
                match process_agent_message(
                    &agent,
                    &user_message,
                    &context,
                    block_id,
                    messages.clone(),
                )
                .await
                {
                    Ok(response) => {
                        // Add assistant response to history
                        {
                            let mut msgs = messages.lock().await;
                            msgs.push(Message {
                                role: "assistant".to_string(),
                                content: response.clone(),
                            });
                        }

                        // Send final assistant message
                        let event_data = serde_json::to_value(AgentUiEvent::AssistantMessage { text: response })
                            .unwrap_or_default();
                        let _ = context
                            .send_output(
                                BlockOutput::builder()
                                    .block_id(block_id)
                                    .object(event_data)
                                    .build(),
                            )
                            .await;

                        // Save state
                        save_session_state(&context, block_id, messages.clone()).await?;
                    }
                    Err(e) => {
                        log::error!("Agent processing error: {}", e);
                        let event_data = serde_json::to_value(AgentUiEvent::AssistantMessage {
                            text: format!("Error: {}", e)
                        })
                        .unwrap_or_default();
                        let _ = context
                            .send_output(
                                BlockOutput::builder()
                                    .block_id(block_id)
                                    .object(event_data)
                                    .build(),
                            )
                            .await;
                    }
                }
            }
        }
    }
}

// ============================================================================
// Agent Processing
// ============================================================================

async fn process_agent_message(
    agent: &Arc<dyn AgentHandle>,
    message: &str,
    context: &ExecutionContext,
    block_id: Uuid,
    _messages: Arc<TokioMutex<Vec<Message>>>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    use agents_sdk::messaging::{AgentMessage, MessageContent, MessageRole};
    use agents_core::llm::StreamChunk;
    use futures::StreamExt;

    log::debug!("Processing agent message: {}", message);

    let agent_message = AgentMessage {
        role: MessageRole::User,
        content: MessageContent::Text(message.to_string()),
        metadata: None,
    };

    log::debug!("Calling agent.handle_message_stream...");
    let mut stream = agent
        .handle_message_stream(agent_message, Arc::new(AgentStateSnapshot::default()))
        .await
        .map_err(|e| {
            log::error!("Agent handle_message_stream error: {:?}", e);
            e
        })?;

    let mut full_text = String::new();

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(StreamChunk::TextDelta(delta)) => {
                full_text.push_str(&delta);
                
                log::debug!("Sending AssistantDelta: {}", delta);
                let event_data = serde_json::to_value(AgentUiEvent::AssistantDelta { text: delta })
                    .unwrap_or_default();
                
                let _ = context
                    .send_output(
                        BlockOutput::builder()
                            .block_id(block_id)
                            .object(event_data)
                            .build(),
                    )
                    .await;
            }
            Ok(StreamChunk::Done { .. }) => {
                log::debug!("Stream completed, full text length: {}", full_text.len());
                break;
            }
            Ok(StreamChunk::Error(e)) => {
                log::error!("Stream error: {}", e);
                return Err(e.into());
            }
            Err(e) => {
                log::error!("Stream chunk error: {:?}", e);
                return Err(e.into());
            }
        }
    }

    Ok(full_text)
}

// ============================================================================
// Agent Event Broadcaster
// ============================================================================

struct AgentEventBroadcaster {
    block_id: Uuid,
    context: ExecutionContext,
}

#[async_trait::async_trait]
impl agents_core::events::EventBroadcaster for AgentEventBroadcaster {
    fn id(&self) -> &str {
        "atuin-agent-broadcaster"
    }

    async fn broadcast(&self, event: &agents_core::events::AgentEvent) -> anyhow::Result<()> {
        use agents_core::events::AgentEvent;

        match event {
            AgentEvent::PlanningComplete(e) => {
                log::debug!("Planning: {} - {}", e.action_type, e.action_summary);
                let event_data = serde_json::to_value(AgentUiEvent::ToolCall {
                    name: format!("Planning: {}", e.action_type),
                    args_json: e.action_summary.clone(),
                })
                .unwrap_or_default();
                
                let _ = self.context
                    .send_output(
                        BlockOutput::builder()
                            .block_id(self.block_id)
                            .object(event_data)
                            .build(),
                    )
                    .await;
            }
            AgentEvent::ToolStarted(e) => {
                log::debug!("Tool started: {}", e.tool_name);
                let event_data = serde_json::to_value(AgentUiEvent::ToolCall {
                    name: e.tool_name.clone(),
                    args_json: e.input_summary.clone(),
                })
                .unwrap_or_default();
                
                let _ = self.context
                    .send_output(
                        BlockOutput::builder()
                            .block_id(self.block_id)
                            .object(event_data)
                            .build(),
                    )
                    .await;
            }
            AgentEvent::ToolCompleted(e) => {
                log::debug!("Tool completed: {} in {}ms", e.tool_name, e.duration_ms);
                let event_data = serde_json::to_value(AgentUiEvent::ToolResult {
                    name: e.tool_name.clone(),
                    ok: true,
                    result_json: e.result_summary.clone(),
                })
                .unwrap_or_default();
                
                let _ = self.context
                    .send_output(
                        BlockOutput::builder()
                            .block_id(self.block_id)
                            .object(event_data)
                            .build(),
                    )
                    .await;
            }
            AgentEvent::ToolFailed(e) => {
                log::error!("Tool failed: {} - {}", e.tool_name, e.error_message);
                let event_data = serde_json::to_value(AgentUiEvent::ToolResult {
                    name: e.tool_name.clone(),
                    ok: false,
                    result_json: e.error_message.clone(),
                })
                .unwrap_or_default();
                
                let _ = self.context
                    .send_output(
                        BlockOutput::builder()
                            .block_id(self.block_id)
                            .object(event_data)
                            .build(),
                    )
                    .await;
            }
            _ => {}
        }

        Ok(())
    }
}

// ============================================================================
// Agent Tools - Document Editing
// ============================================================================

// Note: Tools need access to context for document operations
// Since agents_sdk tools are static, we'll return placeholder responses
// and implement actual document editing via a different mechanism

#[tool("Get the value of a template variable from the current runbook")]
fn get_template_variable(name: String) -> String {
    format!("To get variable '{}', use the template system. This is a stub - actual implementation needs context access.", name)
}

#[tool("Insert blocks at a specific position in the document")]
fn insert_blocks(blocks_json: String, position: i32, placement: String) -> String {
    format!(
        "To insert blocks at position {} {}, the blocks are: {}. This is a stub - actual implementation needs DocumentHandle.",
        position, placement, blocks_json
    )
}

#[tool("Update a block's properties by position")]
fn update_block(position: i32, props_json: String) -> String {
    format!(
        "To update block at position {} with props: {}. This is a stub - actual implementation needs DocumentHandle.",
        position, props_json
    )
}

#[tool("Remove blocks by their positions")]
fn remove_blocks(positions_json: String) -> String {
    format!(
        "To remove blocks at positions: {}. This is a stub - actual implementation needs DocumentHandle.",
        positions_json
    )
}

fn create_agent_tools() -> Vec<Arc<dyn agents_core::Tool>> {
    vec![
        GetTemplateVariableTool::as_tool(),
        InsertBlocksTool::as_tool(),
        UpdateBlockTool::as_tool(),
        RemoveBlocksTool::as_tool(),
    ]
}

// ============================================================================
// Agent Builder
// ============================================================================

async fn build_agent(
    tools: Vec<Arc<dyn agents_core::Tool>>,
    context: &ExecutionContext,
    block_id: Uuid,
) -> Result<Arc<dyn AgentHandle>, Box<dyn std::error::Error + Send + Sync>> {
    // Get AI settings from RuntimeConfig
    let ai_config = context.runtime_config.ai_config().clone();
    log::debug!("building agent with config: {ai_config:?}");
    let api_key = get_ai_api_key(context).await?;
    let model_name = get_ai_model(context).await?;

    let config = OpenAiConfig::new(api_key, &model_name).with_api_url(ai_config.api_endpoint);
    let model = Arc::new(OpenAiChatModel::new(config)?);

    let system_prompt = format!(
        r#"You are an expert runbook editor AI agent. You can read variables and edit the document using these tools:
- get_template_variable: Read variable values from the current runbook
- insert_blocks: Add new blocks at a specific position (before/after)
- update_block: Modify a block's props by position
- remove_blocks: Delete blocks by positions (cannot delete yourself)

Current runbook ID: {}

BLOCK STRUCTURE:
When creating blocks, use this exact JSON structure:
{{
  "type": "block_type",
  "props": {{ /* block-specific props */ }},
  "content": /* optional content array */
}}

AVAILABLE BLOCK TYPES (with exact props):

1. run - Terminal command (PREFERRED for commands)
   {{"type": "run", "props": {{"code": "command here", "name": "Step name"}}}}

2. script - Multi-line shell script
   {{"type": "script", "props": {{"code": "script content", "name": "Script name", "lang": "bash"}}}}

3. postgres - PostgreSQL query
   {{"type": "postgres", "props": {{"query": "SELECT * FROM users", "name": "Query name", "uri": "postgresql://..."}}}}

4. http - HTTP request
   {{"type": "http", "props": {{"url": "https://api.example.com", "verb": "GET", "name": "API call"}}}}

5. var - Template variable (synced)
   {{"type": "var", "props": {{"name": "variable_name", "value": "default value"}}}}

TEMPLATE VARIABLES:
- Use {{{{ var.variable_name }}}} syntax in code, queries, URLs, etc.
- Read variable values with get_template_variable tool
- Variables can store command output, API responses, user input

Be precise with block structure and helpful in your responses."#,
        context.runbook_id
    );

    let checkpointer = Arc::new(InMemoryCheckpointer::new());

    let broadcaster = Arc::new(AgentEventBroadcaster {
        block_id,
        context: context.clone(),
    });

    let mut builder = ConfigurableAgentBuilder::new(&system_prompt)
        .with_model(model)
        .with_checkpointer(checkpointer)
        .with_event_broadcasters(vec![broadcaster]);

    for tool in tools {
        builder = builder.with_tool(tool);
    }

    let agent = builder.build()?;
    Ok(Arc::new(agent))
}

// ============================================================================
// State Persistence
// ============================================================================

async fn load_session_state(
    _context: &ExecutionContext,
    _block_id: Uuid,
) -> Result<AgentSessionState, Box<dyn std::error::Error + Send + Sync>> {
    // TODO: Load from passive context
    Ok(AgentSessionState {
        messages: Vec::new(),
        pending_hitl: None,
    })
}

async fn save_session_state(
    _context: &ExecutionContext,
    _block_id: Uuid,
    _messages: Arc<TokioMutex<Vec<Message>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // TODO: Save to passive context
    Ok(())
}

// ============================================================================
// AI Settings from RuntimeConfig
// ============================================================================

async fn get_ai_api_key(
    context: &ExecutionContext,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let ai_config = &context.runtime_config.ai_config();

    if !ai_config.enabled {
        return Err("AI is not enabled. Please enable AI in settings.".into());
    }

    ai_config
        .api_key
        .clone()
        .ok_or_else(|| "No API key configured. Please set your API key in Settings.".into())
}

async fn get_ai_model(
    context: &ExecutionContext,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let ai_config = &context.runtime_config.ai_config();
    Ok(ai_config
        .model
        .clone()
        .unwrap_or_else(|| "gpt-4o-mini".to_string()))
}

// Note: HitlRequest and HitlResponse are defined in commands/agent.rs
// and re-exported via crate::commands::agent
