//! AI Session - the driver that wraps the Agent FSM and executes effects.

use std::sync::Arc;

use futures_util::stream::StreamExt;
use genai::chat::{ChatMessage, ChatOptions, ChatRequest, ChatStreamEvent, ToolCall};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch, RwLock};
use ts_rs::TS;
use uuid::Uuid;

use crate::ai::fsm::State;
use crate::ai::prompts::AIPrompts;
use crate::ai::tools::AITools;
use crate::secret_cache::{SecretCache, SecretCacheError};

use super::fsm::{Agent, Effect, Event, StreamChunk, ToolOutput, ToolResult};
use super::types::{AIToolCall, ModelSelection};

#[derive(Debug, thiserror::Error)]
pub enum AISessionError {
    #[error("Failed to get credential: {0}")]
    CredentialError(#[from] SecretCacheError),

    #[error("Failed to start request: {0}")]
    RequestError(#[from] genai::Error),

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
    model: ModelSelection,
    client: genai::Client,
    block_types: Vec<String>,
    block_summary: String,
    agent: Arc<RwLock<Agent>>,
    event_tx: mpsc::Sender<Event>,
    event_rx: mpsc::Receiver<Event>,
    output_tx: mpsc::Sender<SessionEvent>,
    secret_cache: Arc<SecretCache>,
    desktop_username: String,
    charge_target: ChargeTarget,
    /// Cancellation signal for the stream processing task.
    cancel_tx: watch::Sender<bool>,
}

impl AISession {
    /// Create a new session, returning the session and a handle for sending events.
    pub fn new(
        model: ModelSelection,
        output_tx: mpsc::Sender<SessionEvent>,
        block_types: Vec<String>,
        block_summary: String,
        secret_cache: Arc<SecretCache>,
    ) -> (Self, SessionHandle) {
        let client = genai::Client::builder().build();
        let (event_tx, event_rx) = mpsc::channel(32);
        let (cancel_tx, _cancel_rx) = watch::channel(false);

        let session = Self {
            id: Uuid::new_v4(),
            model,
            client,
            block_types,
            block_summary,
            agent: Arc::new(RwLock::new(Agent::new())),
            event_tx: event_tx.clone(),
            event_rx,
            output_tx,
            secret_cache,
            cancel_tx,
        };

        let handle = SessionHandle { event_tx };

        (session, handle)
    }

    /// Get the session ID.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Run the session event loop.
    ///
    /// This processes events and executes effects until the channel is closed.
    pub async fn run(mut self) {
        log::debug!("Starting session event loop for {}", self.id);

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
            }
            Effect::ResponseComplete => {
                let _ = self.output_tx.send(SessionEvent::ResponseComplete).await;
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
                            let _ = event_tx
                                .send(Event::RequestFailed {
                                    error: e.to_string(),
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
