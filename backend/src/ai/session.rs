//! AI Session - the driver that wraps the Agent FSM and executes effects.

use std::sync::Arc;

use futures_util::stream::StreamExt;
use genai::chat::{
    CacheControl, ChatMessage, ChatOptions, ChatRequest, ChatStreamEvent, MessageOptions, ToolCall,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch, RwLock};
use ts_rs::TS;
use uuid::Uuid;

use crate::ai::client::AtuinAIClient;
use crate::ai::fsm::State;
use crate::ai::prompts::PromptError;
use crate::ai::types::{ChargeTarget, SessionConfig, SessionKind};
use crate::secret_cache::{SecretCache, SecretCacheError};

use super::fsm::{Agent, Effect, Event, StreamChunk, ToolOutput, ToolResult};
use super::storage::{AISessionStorage, SerializedAISessionV1};
use super::types::{AIMessage, AIToolCall, ModelSelection};

#[derive(Debug, thiserror::Error)]
pub enum AISessionError {
    #[error("Failed to get credential: {0}")]
    CredentialError(#[from] SecretCacheError),

    #[error("Failed to start request: {0}")]
    RequestError(#[from] genai::Error),

    #[error("Failed to generate system prompt: {0}")]
    SystemPromptError(PromptError),

    #[error("Session event channel closed")]
    ChannelClosed,
}

/// Events emitted by the session to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum SessionEvent {
    /// State changed
    StateChanged { state: State },
    /// Stream started
    StreamStarted,
    /// Content chunk received
    Chunk { content: String },
    /// Response complete (no more chunks)
    ResponseComplete,
    /// Tools need to be executed
    ToolsRequested { calls: Vec<AIToolCall> },
    /// An error occurred
    Error { message: String },
    /// Operation was cancelled
    Cancelled,
    /// Full conversation history (sent when subscribing to a session)
    /// Includes pending tool calls if session was restored in PendingTools state
    History {
        messages: Vec<AIMessage>,
        #[serde(rename = "pendingToolCalls")]
        #[ts(rename = "pendingToolCalls")]
        pending_tool_calls: Vec<AIToolCall>,
    },
    /// Blocks were generated via submit_blocks tool (for InlineBlockGeneration sessions).
    /// The session remains in PendingTools state until the frontend sends a tool result
    /// (either via edit request or acceptance/cancellation).
    BlocksGenerated {
        blocks: Vec<serde_json::Value>,
        #[serde(rename = "toolCallId")]
        #[ts(rename = "toolCallId")]
        tool_call_id: String,
    },
}

/// Handle for sending events into the session from external sources.
#[derive(Clone)]
pub struct SessionHandle {
    pub id: Uuid,
    config: Arc<RwLock<SessionConfig>>,
    #[allow(dead_code)] // this will be used to convert between session types in the future
    kind: Arc<RwLock<SessionKind>>,
    event_tx: mpsc::Sender<Event>,
}

impl SessionHandle {
    /// Change the model of the session.
    pub async fn change_model(&self, model: ModelSelection) -> Result<(), AISessionError> {
        let mut config = self.config.write().await;
        config.model = model;
        Ok(())
    }

    /// Change the charge target of the session.
    pub async fn change_charge_target(
        &self,
        charge_target: ChargeTarget,
    ) -> Result<(), AISessionError> {
        let mut config = self.config.write().await;
        config.charge_target = charge_target;
        Ok(())
    }

    /// Change the active user of the session.
    pub async fn change_user(&self, user: String) -> Result<(), AISessionError> {
        let mut config = self.config.write().await;
        config.desktop_username = user;
        Ok(())
    }

    /// Send a user message to the session.
    pub async fn send_user_message(&self, content: String) -> Result<(), AISessionError> {
        let msg = ChatMessage::user(content);
        self.event_tx
            .send(Event::UserMessage(msg))
            .await
            .map_err(|_| AISessionError::ChannelClosed)
    }

    /// Send a tool result to the session.
    pub async fn send_tool_result(
        &self,
        call_id: String,
        success: bool,
        output: String,
    ) -> Result<(), AISessionError> {
        let result = ToolResult {
            call_id,
            output: if success {
                ToolOutput::Success(output)
            } else {
                ToolOutput::Error(output)
            },
        };
        self.event_tx
            .send(Event::ToolResult(result))
            .await
            .map_err(|_| AISessionError::ChannelClosed)
    }

    /// Cancel the current operation.
    pub async fn cancel(&self) -> Result<(), AISessionError> {
        self.event_tx
            .send(Event::Cancel)
            .await
            .map_err(|_| AISessionError::ChannelClosed)
    }

    /// Send an edit request for InlineBlockGeneration sessions.
    /// This updates the system prompt to edit mode, responds to the pending submit_blocks call,
    /// and sends the user's edit prompt to continue generation.
    pub async fn send_edit_request(
        &self,
        edit_prompt: String,
        tool_call_id: String,
    ) -> Result<(), AISessionError> {
        // 1. Update kind to is_initial_generation: false
        {
            let mut kind = self.kind.write().await;
            if let SessionKind::InlineBlockGeneration {
                is_initial_generation,
                ..
            } = &mut *kind
            {
                *is_initial_generation = false;
            }
        }

        // 2. Generate new system prompt and send UpdateSystemPrompt event
        let new_system_prompt = {
            let kind = self.kind.read().await;
            kind.system_prompt()
                .map_err(AISessionError::SystemPromptError)?
        };
        let system_msg = ChatMessage::system(new_system_prompt).with_options(MessageOptions {
            cache_control: Some(CacheControl::Ephemeral),
        });
        self.event_tx
            .send(Event::UpdateSystemPrompt(system_msg))
            .await
            .map_err(|_| AISessionError::ChannelClosed)?;

        // 3. Send user message with edit prompt first (gets queued while in PendingTools)
        let user_msg = ChatMessage::user(edit_prompt);
        self.event_tx
            .send(Event::UserMessage(user_msg))
            .await
            .map_err(|_| AISessionError::ChannelClosed)?;

        // 4. Send tool result for submit_blocks - this completes PendingTools,
        //    drains the queued edit prompt, and triggers a single LLM request
        self.event_tx
            .send(Event::ToolResult(ToolResult {
                call_id: tool_call_id,
                output: ToolOutput::Success(
                    "Blocks shown to user. User has requested changes.".to_string(),
                ),
            }))
            .await
            .map_err(|_| AISessionError::ChannelClosed)?;

        Ok(())
    }
}

/// The AI session driver.
///
/// Wraps the Agent FSM and handles effect execution.
pub struct AISession {
    id: Uuid,
    config: Arc<RwLock<SessionConfig>>,
    kind: Arc<RwLock<SessionKind>>,
    client: AtuinAIClient,
    agent: Arc<RwLock<Agent>>,
    event_tx: mpsc::Sender<Event>,
    event_rx: mpsc::Receiver<Event>,
    output_tx: mpsc::Sender<SessionEvent>,
    secret_cache: Arc<SecretCache>,
    storage: Arc<AISessionStorage>,
    /// Cancellation signal for the stream processing task.
    cancel_tx: watch::Sender<bool>,
}

impl AISession {
    /// Create a new session, returning the session and a handle for sending events.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kind: SessionKind,
        config: SessionConfig,
        output_tx: mpsc::Sender<SessionEvent>,
        secret_cache: Arc<SecretCache>,
        storage: Arc<AISessionStorage>,
    ) -> Result<(Self, SessionHandle), AISessionError> {
        let client = AtuinAIClient::new(secret_cache.clone());
        let (event_tx, event_rx) = mpsc::channel(32);
        let (cancel_tx, _cancel_rx) = watch::channel(false);

        let system_prompt = kind
            .system_prompt()
            .map_err(AISessionError::SystemPromptError)?;

        let config = Arc::new(RwLock::new(config));
        let kind = Arc::new(RwLock::new(kind));

        let session = Self {
            id: Uuid::new_v4(),
            config: config.clone(),
            kind: kind.clone(),
            client,
            agent: Arc::new(RwLock::new(Agent::new(ChatMessage::system(system_prompt)))),
            event_tx: event_tx.clone(),
            event_rx,
            output_tx,
            secret_cache,
            storage,
            cancel_tx,
        };

        let handle = SessionHandle {
            id: session.id,
            event_tx,
            config,
            kind,
        };

        Ok((session, handle))
    }

    /// Get the session ID.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Create a session from saved state, returning the session and a handle for sending events.
    #[allow(clippy::too_many_arguments)]
    pub fn from_saved(
        saved: SerializedAISessionV1,
        output_tx: mpsc::Sender<SessionEvent>,
        secret_cache: Arc<SecretCache>,
        storage: Arc<AISessionStorage>,
    ) -> Result<(Self, SessionHandle), AISessionError> {
        let client = AtuinAIClient::new(secret_cache.clone());
        let (event_tx, event_rx) = mpsc::channel(32);
        let (cancel_tx, _cancel_rx) = watch::channel(false);

        // Restore agent with saved state and context
        log::debug!("Restoring session {} from storage", saved.id);

        let system_prompt = saved
            .kind
            .system_prompt()
            .map_err(AISessionError::SystemPromptError)?;

        let config = Arc::new(RwLock::new(saved.config));
        let kind = Arc::new(RwLock::new(saved.kind));

        let agent = Agent::from_saved(
            saved.agent_state,
            saved.agent_context,
            ChatMessage::system(system_prompt),
        );

        let session = Self {
            id: saved.id,
            config: config.clone(),
            kind: kind.clone(),
            client,
            agent: Arc::new(RwLock::new(agent)),
            event_tx: event_tx.clone(),
            event_rx,
            output_tx,
            secret_cache,
            storage,
            cancel_tx,
        };

        let handle = SessionHandle {
            id: session.id,
            event_tx,
            config,
            kind,
        };

        Ok((session, handle))
    }

    /// Save the current session state to storage if the session is meant to be persisted.
    async fn save_if_persisted(&self) {
        if self.kind.read().await.persists_state() {
            self.save_state().await;
        }
    }

    /// Save the current session state to storage.
    async fn save_state(&self) {
        let (state, context) = {
            let agent = self.agent.read().await;
            (agent.state().clone(), agent.context().clone())
        };

        log::debug!("Saving session {} to storage", self.id);

        let config = self.config.read().await.clone();
        let kind = self.kind.read().await.clone();
        let runbook_id = kind.runbook_id();

        let serialized = SerializedAISessionV1 {
            id: self.id,
            config,
            kind,
            agent_state: state,
            agent_context: context,
        };

        if let Err(e) = self.storage.save(&runbook_id, &serialized).await {
            log::error!("Failed to save session state: {}", e);
        } else {
            log::debug!("Saved session {} state", self.id);
        }
    }

    /// Run the session event loop.
    ///
    /// This processes events and executes effects until the channel is closed.
    pub async fn run(mut self) {
        log::debug!("Starting session event loop for {}", self.id);

        // Save immediately so this session becomes the most recent for the runbook
        // This ensures "clear chat" followed by quit will restore a blank session
        self.save_if_persisted().await;

        while let Some(event) = self.event_rx.recv().await {
            log::trace!("Session {} received event: {:?}", self.id, event);

            // Feed event to FSM
            let transition = {
                let mut agent = self.agent.write().await;
                agent.handle(event)
            };

            // Execute effects
            for effect in transition.effects {
                if let Err(e) = self.execute_effect(effect).await {
                    log::error!("Session {} effect execution failed: {}", self.id, e);
                    let _ = self
                        .output_tx
                        .send(SessionEvent::Error {
                            message: e.to_string(),
                        })
                        .await;
                }
            }

            let _ = self
                .output_tx
                .send(SessionEvent::StateChanged {
                    state: self.agent.read().await.state().clone(),
                })
                .await;
        }

        log::debug!("Session {} event loop ended", self.id);
    }

    /// Execute a single effect.
    async fn execute_effect(&mut self, effect: Effect) -> Result<(), AISessionError> {
        match effect {
            Effect::StartRequest => {
                self.start_request().await?;
            }
            Effect::EmitChunk { content } => {
                if self.kind.read().await.emits_chunks() {
                    let _ = self.output_tx.send(SessionEvent::Chunk { content }).await;
                }
            }
            Effect::ExecuteTools { calls } => {
                // Check for submit_blocks tool call (used by InlineBlockGeneration sessions)
                if let Some(submit_call) = calls.iter().find(|c| c.fn_name == "submit_blocks") {
                    // Extract blocks from arguments and emit BlocksGenerated event
                    if let Some(blocks) = submit_call.fn_arguments.get("blocks") {
                        let blocks_vec = blocks.as_array().cloned().unwrap_or_default();
                        let _ = self
                            .output_tx
                            .send(SessionEvent::BlocksGenerated {
                                blocks: blocks_vec,
                                tool_call_id: submit_call.call_id.clone(),
                            })
                            .await;
                    }

                    // Filter out submit_blocks from the calls sent to frontend as ToolsRequested
                    // (submit_blocks is handled specially via BlocksGenerated)
                    let other_calls: Vec<AIToolCall> = calls
                        .into_iter()
                        .filter(|c| c.fn_name != "submit_blocks")
                        .map(|c| c.into())
                        .collect();

                    if !other_calls.is_empty() {
                        let _ = self
                            .output_tx
                            .send(SessionEvent::ToolsRequested { calls: other_calls })
                            .await;
                    }
                } else {
                    // Normal tool execution path
                    let ai_calls: Vec<AIToolCall> = calls.into_iter().map(|c| c.into()).collect();
                    let _ = self
                        .output_tx
                        .send(SessionEvent::ToolsRequested { calls: ai_calls })
                        .await;
                }
                // Save state when entering PendingTools
                self.save_if_persisted().await;
            }
            Effect::ToolResultReceived => {
                // Save state after each tool result for durability
                self.save_if_persisted().await;
            }
            Effect::ResponseComplete => {
                let _ = self.output_tx.send(SessionEvent::ResponseComplete).await;
                // Send updated history so frontends can update their conversation view
                // Note: We don't include pending_tool_calls here because ToolsRequested
                // already handles that, and including them would cause duplicates
                let messages = {
                    let agent = self.agent.read().await;
                    agent
                        .context()
                        .conversation
                        .iter()
                        .map(|m| AIMessage::from(m.clone()))
                        .collect()
                };
                let _ = self
                    .output_tx
                    .send(SessionEvent::History {
                        messages,
                        pending_tool_calls: vec![],
                    })
                    .await;
                // Save state on response complete
                self.save_if_persisted().await;
            }
            Effect::Error { message } => {
                let _ = self.output_tx.send(SessionEvent::Error { message }).await;
            }
            Effect::Cancelled => {
                // Signal the stream processing task to stop
                let _ = self.cancel_tx.send(true);
                let _ = self.output_tx.send(SessionEvent::Cancelled).await;
            }
        }
        Ok(())
    }

    /// Start a new request to the model.
    async fn start_request(&self) -> Result<(), AISessionError> {
        log::debug!("Starting request for session {}", self.id);

        // Build the request from conversation history (FSM uses ChatMessage directly)
        let messages = {
            let agent = self.agent.read().await;
            agent.context().conversation.clone()
        };

        let config = self.config.read().await.clone();
        let kind = self.kind.read().await.clone();
        let model_str = config.model.to_string();

        let system_prompt = kind
            .system_prompt()
            .map_err(AISessionError::SystemPromptError)?;
        let tools = kind.tools();

        let chat_request = ChatRequest::new(messages)
            .with_system(system_prompt)
            .with_tools(tools);

        let mut chat_options = ChatOptions::default().with_capture_tool_calls(true);

        // If we're using the Atuin Hub provider, add proprietary headers for auth and charge tracking
        if let ModelSelection::AtuinHub { .. } = &config.model {
            let secret = self
                .secret_cache
                .get("sh.atuin.runbooks.api", &config.desktop_username)
                .await?
                .ok_or(AISessionError::CredentialError(
                    SecretCacheError::LookupFailed {
                        service: "sh.atuin.runbooks.api".to_string(),
                        user: config.desktop_username.clone(),
                        context: "No Atuin Hub API key found".to_string(),
                    },
                ))?;

            let action = match kind {
                SessionKind::InlineBlockGeneration {
                    is_initial_generation,
                    ..
                } => {
                    if is_initial_generation {
                        "generate"
                    } else {
                        "edit"
                    }
                }
                SessionKind::AssistantChat { .. } => "proxy",
            };

            let api_key_header = "x-atuin-hub-api-key".to_string();
            let api_charge_header = "x-atuin-charge-to".to_string();
            let api_action_header = "x-atuin-action".to_string();

            let extra_headers = vec![
                (api_key_header, secret),
                (api_charge_header, config.charge_target.to_string()),
                (api_action_header, action.to_string()),
            ];

            chat_options = chat_options.with_extra_headers(extra_headers);
        }

        drop(config);
        drop(kind);

        log::debug!(
            "Executing chat stream for session {} with model {model_str}",
            self.id
        );
        let stream = self
            .client
            .exec_chat_stream(&model_str, chat_request, Some(&chat_options))
            .await?;

        // Spawn task to process the stream
        let event_tx = self.event_tx.clone();
        let session_id = self.id;
        // Reset cancellation signal before starting new stream
        let _ = self.cancel_tx.send(false);
        let cancel_rx = self.cancel_tx.subscribe();

        tokio::spawn(async move {
            log::debug!("Processing stream for session {}", session_id);
            Self::process_stream(session_id, stream, event_tx, cancel_rx).await;
        });

        Ok(())
    }

    /// Process the streaming response, feeding events back to the FSM.
    async fn process_stream(
        session_id: Uuid,
        stream_response: genai::chat::ChatStreamResponse,
        event_tx: mpsc::Sender<Event>,
        mut cancel_rx: watch::Receiver<bool>,
    ) {
        let mut stream = stream_response.stream;
        let mut tool_calls: Vec<ToolCall> = Vec::new();

        // Send StreamStart
        if event_tx.send(Event::StreamStart).await.is_err() {
            log::warn!("Session {} channel closed during stream", session_id);
            return;
        }

        loop {
            tokio::select! {
                // Check for cancellation
                _ = cancel_rx.changed() => {
                    if *cancel_rx.borrow() {
                        log::debug!("Session {} stream cancelled", session_id);
                        // Don't send StreamEnd - the FSM already handled the Cancel event
                        return;
                    }
                }
                // Process stream events
                maybe_result = stream.next() => {
                    let Some(result) = maybe_result else {
                        // Stream ended naturally
                        break;
                    };

                    match result {
                        Err(e) => {
                            log::error!("Session {} stream error: {}", session_id, e);
                            let mut message = e.to_string();

                            if let genai::Error::WebStream { error, .. } = e {
                                if let Some(genai::Error::HttpError { status, body, .. }) = error.downcast_ref::<genai::Error>() {
                                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
                                        if let Some(msg) = json.get("error").and_then(|m| m.as_str()) {
                                            message = msg.to_string();
                                        }

                                        if let Some(details) = json.get("details").and_then(|d| d.as_object()) {
                                            for (k, v) in details.iter() {
                                                message += &format!("\n  {}: {}", k, v);
                                            }
                                        }
                                    } else {
                                        message = format!("HTTP error {}: {}", status, body);
                                    }
                                }
                            }

                            let _ = event_tx
                                .send(Event::RequestFailed {
                                    error: message,
                                })
                                .await;
                            return;
                        }
                        Ok(ChatStreamEvent::Start) => {
                            log::trace!("Session {} received stream start", session_id);
                            // Already sent StreamStart above
                        }
                        Ok(ChatStreamEvent::Chunk(chunk)) => {
                            log::trace!("Session {} received chunk", session_id);
                            let _ = event_tx
                                .send(Event::StreamChunk(StreamChunk {
                                    content: chunk.content,
                                }))
                                .await;
                        }
                        Ok(ChatStreamEvent::ThoughtSignatureChunk(_)) => {
                            log::trace!("Session {} received thought signature chunk", session_id,);
                        }
                        Ok(ChatStreamEvent::ToolCallChunk(_tc_chunk)) => {
                            // Tool call chunks are accumulated by genai internally
                            // We'll get the complete tool calls in the End event
                            log::trace!("Session {} received tool call chunk", session_id);
                        }
                        Ok(ChatStreamEvent::ReasoningChunk(_)) => {
                            log::trace!("Session {} received reasoning chunk", session_id);
                            // Ignore reasoning chunks for now
                        }
                        Ok(ChatStreamEvent::End(end)) => {
                            log::trace!("Session {} received stream end", session_id);
                            // Extract tool calls from captured content
                            if let Some(content) = end.captured_content {
                                for part in content.into_parts() {
                                    if let genai::chat::ContentPart::ToolCall(tc) = part {
                                        tool_calls.push(tc);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Send StreamEnd with any tool calls
        let _ = event_tx.send(Event::StreamEnd { tool_calls }).await;
    }
}
