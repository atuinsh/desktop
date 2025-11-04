use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex as TokioMutex, RwLock};
use typed_builder::TypedBuilder;
use uuid::Uuid;

// TODO: Uncomment when agents_sdk is available
// use agents_core::tool::Tool;
// use agents_sdk::{get_default_model, ConfigurableAgentBuilder};

use crate::runtime::blocks::document::bridge::{AgentUiEvent, DocumentBridgeMessage};
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

        // Spawn agent session
        tokio::spawn(async move {
            let result = run_agent_session(block_id, context.clone(), handle_clone.cancellation_token.clone()).await;

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
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    log::debug!("Starting agent session for block {}", block_id);

    // Create channels for agent communication
    let (user_message_tx, mut user_message_rx) = mpsc::channel::<String>(10);
    let (hitl_request_tx, _hitl_request_rx) = mpsc::channel::<HitlRequest>(10);
    let hitl_responses: Arc<TokioMutex<HashMap<String, oneshot::Sender<HitlResponse>>>> =
        Arc::new(TokioMutex::new(HashMap::new()));

    // Register agent session in a global registry so API endpoints can send messages
    // TODO: Implement AgentSessionRegistry
    // AGENT_SESSION_REGISTRY.register(block_id, context.runbook_id, user_message_tx, hitl_request_tx, hitl_responses.clone()).await;

    // Load or initialize session state from passive context
    let initial_state = load_session_state(&context, block_id).await?;
    let messages = Arc::new(TokioMutex::new(initial_state.messages));

    // Create agent tools
    let tools = create_agent_tools(block_id, context.clone());

    // Build agent
    let agent = match build_agent(tools, &context).await {
        Ok(agent) => agent,
        Err(e) => {
            log::error!("Failed to build agent: {}", e);
            return Err(format!("Failed to build agent: {}", e).into());
        }
    };

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
                            let _ = context.send_output(DocumentBridgeMessage::AgentEvent {
                                block_id,
                                event: AgentUiEvent::AssistantMessage { text: response },
                            }).await;

                            // Save state
                            save_session_state(&context, block_id, messages.clone()).await?;
                        }
                        Err(e) => {
                            log::error!("Agent processing error: {}", e);
                            let _ = context.send_output(DocumentBridgeMessage::AgentEvent {
                                block_id,
                                event: AgentUiEvent::AssistantMessage {
                                    text: format!("Error: {}", e)
                                },
                            }).await;
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
                    
                    // TODO: Unregister from session registry
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
                        let _ = context.send_output(DocumentBridgeMessage::AgentEvent {
                            block_id,
                            event: AgentUiEvent::AssistantMessage { text: response },
                        }).await;

                        // Save state
                        save_session_state(&context, block_id, messages.clone()).await?;
                    }
                    Err(e) => {
                        log::error!("Agent processing error: {}", e);
                        let _ = context.send_output(DocumentBridgeMessage::AgentEvent {
                            block_id,
                            event: AgentUiEvent::AssistantMessage {
                                text: format!("Error: {}", e)
                            },
                        }).await;
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
    _agent: &AgentStub,
    message: &str,
    context: &ExecutionContext,
    block_id: Uuid,
    _messages: Arc<TokioMutex<Vec<Message>>>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // For now, return a simple echo response
    // TODO: Integrate with agents_sdk properly
    
    let response = format!("Received: {}", message);
    
    // Stream as delta
    let _ = context.send_output(DocumentBridgeMessage::AgentEvent {
        block_id,
        event: AgentUiEvent::AssistantDelta {
            text: response.clone(),
        },
    }).await;

    Ok(response)
}

// ============================================================================
// Agent Tools (stub - TODO: implement with agents_sdk)
// ============================================================================

// Placeholder Tool trait until agents_sdk is available
#[allow(dead_code)]
trait Tool: Send + Sync {
    fn name(&self) -> &str;
}

fn create_agent_tools(
    _block_id: Uuid,
    _context: ExecutionContext,
) -> Vec<Box<dyn Tool>> {
    // TODO: Implement actual tools using agents_sdk's tool macro
    vec![]
}

// ============================================================================
// Agent Builder (stub - TODO: implement with agents_sdk)
// ============================================================================

// Placeholder Agent struct until agents_sdk is available
#[allow(dead_code)]
struct AgentStub;

async fn build_agent(
    _tools: Vec<Box<dyn Tool>>,
    context: &ExecutionContext,
) -> Result<AgentStub, Box<dyn std::error::Error + Send + Sync>> {
    // Get AI settings from context
    // TODO: Access AI settings from app state
    // let model = get_default_model()?;

    let _system_prompt = format!(
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

    // TODO: Implement when agents_sdk is available
    // let mut builder = ConfigurableAgentBuilder::new(&system_prompt).with_model(model);
    // for tool in tools {
    //     builder = builder.with_tool(tool);
    // }
    // let agent = builder.build()?;
    
    Ok(AgentStub)
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
// HITL Types (for future implementation)
// ============================================================================

#[derive(Debug, Clone)]
struct HitlRequest {
    id: String,
    prompt: String,
    options: serde_json::Value,
}

#[derive(Debug, Clone)]
struct HitlResponse {
    decision: String,
    data: Option<serde_json::Value>,
}
