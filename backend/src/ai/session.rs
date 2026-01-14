//! AI Session - the driver that wraps the Agent FSM and executes effects.

use std::fmt::Display;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use futures_util::stream::StreamExt;
use genai::adapter::AdapterKind;
use genai::chat::{ChatMessage, ChatOptions, ChatRequest, ChatStreamEvent, ToolCall};
use genai::resolver::{AuthData, Endpoint, ServiceTargetResolver};
use genai::{ClientConfig, ModelIden, ServiceTarget};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch, RwLock};
use ts_rs::TS;
use uuid::Uuid;

use crate::ai::fsm::State;
use crate::ai::prompts::AIPrompts;
use crate::ai::tools::AITools;
use crate::secret_cache::{SecretCache, SecretCacheError};

use super::fsm::{Agent, Effect, Event, StreamChunk, ToolOutput, ToolResult};
use super::storage::{AISessionStorage, SerializedAISession};
use super::types::{AIMessage, AIToolCall, ModelSelection};

#[derive(Debug, thiserror::Error)]
pub enum AISessionError {
    #[error("Failed to get credential: {0}")]
    CredentialError(#[from] SecretCacheError),

    #[error("Failed to start request: {0}")]
    RequestError(#[from] genai::Error),

    #[error("Session event channel closed")]
    ChannelClosed,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ChargeTarget {
    User,
    Org(String),
}

impl Display for ChargeTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChargeTarget::User => write!(f, "user"),
            ChargeTarget::Org(org) => write!(f, "org:{}", org),
        }
    }
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
}

/// Handle for sending events into the session from external sources.
#[derive(Clone)]
pub struct SessionHandle {
    event_tx: mpsc::Sender<Event>,
}

impl SessionHandle {
    /// Change the model of the session.
    pub async fn change_model(&self, model: ModelSelection) -> Result<(), AISessionError> {
        self.event_tx
            .send(Event::ModelChange(model))
            .await
            .map_err(|_| AISessionError::ChannelClosed)
    }

    /// Change the charge target of the session.
    pub async fn change_charge_target(
        &self,
        charge_target: ChargeTarget,
    ) -> Result<(), AISessionError> {
        self.event_tx
            .send(Event::ChargeTargetChange(charge_target))
            .await
            .map_err(|_| AISessionError::ChannelClosed)
    }

    /// Change the active user of the session.
    pub async fn change_user(&self, user: String) -> Result<(), AISessionError> {
        self.event_tx
            .send(Event::UserChange(user))
            .await
            .map_err(|_| AISessionError::ChannelClosed)
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
}

/// The AI session driver.
///
/// Wraps the Agent FSM and handles effect execution.
pub struct AISession {
    id: Uuid,
    runbook_id: Uuid,
    model: ModelSelection,
    client: genai::Client,
    block_types: Vec<String>,
    block_summary: String,
    agent: Arc<RwLock<Agent>>,
    event_tx: mpsc::Sender<Event>,
    event_rx: mpsc::Receiver<Event>,
    output_tx: mpsc::Sender<SessionEvent>,
    secret_cache: Arc<SecretCache>,
    storage: Arc<AISessionStorage>,
    desktop_username: String,
    charge_target: ChargeTarget,
    /// Cancellation signal for the stream processing task.
    cancel_tx: watch::Sender<bool>,
}

fn resolve_service_target(
    mut service_target: ServiceTarget,
) -> Pin<Box<dyn Future<Output = Result<ServiceTarget, genai::resolver::Error>> + Send>> {
    Box::pin(async move {
        let model_name = service_target.model.model_name.to_string();
        let parts = model_name.splitn(3, "::").collect::<Vec<&str>>();

        if parts.len() != 3 {
            return Err(genai::resolver::Error::Custom(format!(
                "Invalid Atuin Desktop model identifier format: {}",
                model_name
            )));
        }

        let adapter_kind = match parts[0] {
            "atuinhub" => AdapterKind::Anthropic,
            "claude" => AdapterKind::Anthropic,
            "openai" => AdapterKind::OpenAI,
            "ollama" => AdapterKind::Ollama,
            _ => {
                return Err(genai::resolver::Error::Custom(format!(
                    "Invalid model identifier: {}",
                    parts[0]
                )))
            }
        };

        let auth = AuthData::Key(
            AISession::get_api_key(adapter_kind, parts[0] == "atuinhub")
                .await
                .map_err(|e| genai::resolver::Error::Custom(e.to_string()))?,
        );
        service_target.auth = auth;

        let model_id = ModelIden::new(adapter_kind, parts[1]);
        service_target.model = model_id;

        if parts[2] != "default" {
            service_target.endpoint = Endpoint::from_owned(parts[2].to_string());
        }

        Ok(service_target)
    })
}

impl AISession {
    /// Create a new session, returning the session and a handle for sending events.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        runbook_id: Uuid,
        model: ModelSelection,
        output_tx: mpsc::Sender<SessionEvent>,
        block_types: Vec<String>,
        block_summary: String,
        desktop_username: String,
        charge_target: ChargeTarget,
        secret_cache: Arc<SecretCache>,
        storage: Arc<AISessionStorage>,
    ) -> (Self, SessionHandle) {
        let target_resolver = ServiceTargetResolver::from_resolver_async_fn(resolve_service_target);

        let client = genai::Client::builder()
            .with_config(ClientConfig::default().with_service_target_resolver(target_resolver))
            .build();
        let (event_tx, event_rx) = mpsc::channel(32);
        let (cancel_tx, _cancel_rx) = watch::channel(false);

        let session = Self {
            id: Uuid::new_v4(),
            runbook_id,
            model,
            client,
            block_types,
            block_summary,
            agent: Arc::new(RwLock::new(Agent::new())),
            event_tx: event_tx.clone(),
            event_rx,
            output_tx,
            desktop_username,
            charge_target,
            secret_cache,
            storage,
            cancel_tx,
        };

        let handle = SessionHandle { event_tx };

        (session, handle)
    }

    /// Get the session ID.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Create a session from saved state, returning the session and a handle for sending events.
    #[allow(clippy::too_many_arguments)]
    pub fn from_saved(
        saved: SerializedAISession,
        output_tx: mpsc::Sender<SessionEvent>,
        block_types: Vec<String>,
        block_summary: String,
        desktop_username: String,
        charge_target: ChargeTarget,
        secret_cache: Arc<SecretCache>,
        storage: Arc<AISessionStorage>,
    ) -> (Self, SessionHandle) {
        let target_resolver = ServiceTargetResolver::from_resolver_async_fn(resolve_service_target);

        let client = genai::Client::builder()
            .with_config(ClientConfig::default().with_service_target_resolver(target_resolver))
            .build();
        let (event_tx, event_rx) = mpsc::channel(32);
        let (cancel_tx, _cancel_rx) = watch::channel(false);

        // Restore agent with saved state and context
        let agent = Agent::from_saved(saved.agent_state, saved.agent_context);

        let session = Self {
            id: saved.id,
            runbook_id: saved.runbook_id,
            model: saved.model,
            client,
            block_types,
            block_summary,
            agent: Arc::new(RwLock::new(agent)),
            event_tx: event_tx.clone(),
            event_rx,
            output_tx,
            desktop_username,
            charge_target,
            secret_cache,
            storage,
            cancel_tx,
        };

        let handle = SessionHandle { event_tx };

        (session, handle)
    }

    /// Save the current session state to storage.
    async fn save_state(&self) {
        let (state, context) = {
            let agent = self.agent.read().await;
            (agent.state().clone(), agent.context().clone())
        };

        log::debug!(
            "Saving session {} - state: {:?}, pending_tools: {:?}",
            self.id,
            state,
            context.pending_tools.keys().collect::<Vec<_>>()
        );

        let serialized = SerializedAISession {
            id: self.id,
            runbook_id: self.runbook_id,
            model: self.model.clone(),
            agent_state: state,
            agent_context: context,
            updated_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
        };

        if let Err(e) = self.storage.save(&serialized).await {
            log::error!("Failed to save session state: {}", e);
        } else {
            log::debug!("Saved session {} state", self.id);
        }
    }

    async fn get_api_key(
        _adapter_kind: AdapterKind,
        is_hub: bool,
    ) -> Result<String, AISessionError> {
        if is_hub {
            return Ok("".to_string());
        }

        Ok("".to_string())
    }

    /// Run the session event loop.
    ///
    /// This processes events and executes effects until the channel is closed.
    pub async fn run(mut self) {
        log::debug!("Starting session event loop for {}", self.id);

        // Save immediately so this session becomes the most recent for the runbook
        // This ensures "clear chat" followed by quit will restore a blank session
        self.save_state().await;

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
            Effect::ModelChange(model) => {
                self.model = model;
            }
            Effect::ChargeTargetChange(charge_target) => {
                self.charge_target = charge_target;
            }
            Effect::UserChange(user) => {
                self.desktop_username = user;
            }
            Effect::StartRequest => {
                self.start_request().await?;
            }
            Effect::EmitChunk { content } => {
                let _ = self.output_tx.send(SessionEvent::Chunk { content }).await;
            }
            Effect::ExecuteTools { calls } => {
                // Convert genai ToolCall to AIToolCall for frontend
                let ai_calls: Vec<AIToolCall> = calls.into_iter().map(|c| c.into()).collect();
                let _ = self
                    .output_tx
                    .send(SessionEvent::ToolsRequested { calls: ai_calls })
                    .await;
                // Save state when entering PendingTools
                self.save_state().await;
            }
            Effect::ToolResultReceived => {
                // Save state after each tool result for durability
                self.save_state().await;
            }
            Effect::ResponseComplete => {
                let _ = self.output_tx.send(SessionEvent::ResponseComplete).await;
                // Save state on response complete
                self.save_state().await;
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

        let chat_request = ChatRequest::new(messages)
            .with_system(AIPrompts::system_prompt(&self.block_summary))
            .with_tools(vec![
                AITools::get_runboook_document(),
                AITools::get_block_docs(&self.block_types),
                AITools::get_default_shell(),
                AITools::insert_blocks(&self.block_types),
                AITools::update_block(),
                AITools::replace_blocks(),
            ]);

        let mut chat_options = ChatOptions::default().with_capture_tool_calls(true);

        if let ModelSelection::AtuinHub { .. } = self.model {
            let secret = self
                .secret_cache
                .get("sh.atuin.runbooks.api", &self.desktop_username)
                .await?
                .ok_or(AISessionError::CredentialError(
                    SecretCacheError::LookupFailed {
                        service: "sh.atuin.runbooks.api".to_string(),
                        user: self.desktop_username.clone(),
                        context: "No Atuin Hub API key found".to_string(),
                    },
                ))?;

            let extra_headers = vec![
                ("x-atuin-hub-api-key".to_string(), secret),
                (
                    "x-atuin-charge-to".to_string(),
                    self.charge_target.to_string(),
                ),
            ];
            chat_options = chat_options.with_extra_headers(extra_headers);
        }

        let stream = self
            .client
            .exec_chat_stream(&self.model.to_string(), chat_request, Some(&chat_options))
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
                        Ok(ChatStreamEvent::ToolCallChunk(tc_chunk)) => {
                            // Tool call chunks are accumulated by genai internally
                            // We'll get the complete tool calls in the End event
                            log::trace!(
                                "Session {} received tool call chunk: {:?}",
                                session_id,
                                tc_chunk
                            );
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
