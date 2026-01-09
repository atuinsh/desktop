//! Agent conversation FSM.
//!
//! Pure state machine that returns effects as data.
//! Caller is responsible for executing effects and feeding events back.
//!
//! Uses genai types internally so the driver doesn't need to convert.

use std::collections::{HashMap, VecDeque};

use genai::chat::{ChatMessage, ContentPart, MessageContent, ToolCall, ToolResponse};

// ============================================================================
// FSM State
// ============================================================================

/// The discrete states of the agent FSM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Waiting for user input. No active request.
    Idle,

    /// Request sent to model, waiting for stream to begin.
    Sending,

    /// Actively receiving streamed response.
    Streaming,
}

// ============================================================================
// Context
// ============================================================================

/// A tool result provided by the caller.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub call_id: String,
    pub output: ToolOutput,
}

#[derive(Debug, Clone)]
pub enum ToolOutput {
    Success(String),
    Error(String),
}

/// A chunk of streamed response.
#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub content: String,
}

/// Shared context across all states.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// Messages queued while a request is in flight.
    pub queued_messages: VecDeque<ChatMessage>,

    /// Tool calls we're waiting on. Map from call_id to the call.
    pub pending_tools: HashMap<String, ToolCall>,

    /// Accumulated tool results.
    pub tool_results: Vec<ToolResult>,

    /// The full conversation history (for building requests).
    pub conversation: Vec<ChatMessage>,

    /// Accumulated content from current stream (for building assistant message).
    pub current_response: String,

    /// Accumulated tool calls from current stream.
    pub current_tool_calls: Vec<ToolCall>,
}

impl Context {
    /// Take all queued messages, leaving the queue empty.
    pub fn take_queued(&mut self) -> Vec<ChatMessage> {
        self.queued_messages.drain(..).collect()
    }

    /// Take all tool results, leaving the vec empty.
    pub fn take_tool_results(&mut self) -> Vec<ToolResult> {
        std::mem::take(&mut self.tool_results)
    }
}

// ============================================================================
// Events (inputs to the FSM)
// ============================================================================

/// Events that drive state transitions.
#[derive(Debug, Clone)]
pub enum Event {
    /// User submitted a message.
    UserMessage(ChatMessage),

    /// Stream has started (connection established, first response).
    StreamStart,

    /// Received a chunk of streamed content.
    StreamChunk(StreamChunk),

    /// Stream ended. May include tool calls.
    StreamEnd { tool_calls: Vec<ToolCall> },

    /// A tool finished executing.
    ToolResult(ToolResult),

    /// Request failed (network error, API error, etc.)
    RequestFailed { error: String },

    /// User explicitly cancelled the current operation.
    Cancel,
}

// ============================================================================
// Effects (outputs from the FSM)
// ============================================================================

/// Side effects the caller should execute.
///
/// The FSM returns these; it never executes them directly.
#[derive(Debug, Clone)]
pub enum Effect {
    /// Start a new request to the model.
    /// Caller should use context.conversation to build the actual request.
    StartRequest,

    /// Emit a content chunk to the UI.
    EmitChunk { content: String },

    /// Request tool execution. Caller executes and feeds back ToolResult events.
    ExecuteTools { calls: Vec<ToolCall> },

    /// Notify that response is complete (for UI state).
    ResponseComplete,

    /// Notify of an error (for UI/logging).
    Error { message: String },

    /// Operation was cancelled.
    Cancelled,
}

impl PartialEq for Effect {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Effect::StartRequest, Effect::StartRequest) => true,
            (Effect::EmitChunk { content: a }, Effect::EmitChunk { content: b }) => a == b,
            (Effect::ExecuteTools { calls: a }, Effect::ExecuteTools { calls: b }) => {
                // Compare tool calls by call_id and fn_name (not fn_arguments for simplicity)
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|(x, y)| x.call_id == y.call_id && x.fn_name == y.fn_name)
            }
            (Effect::ResponseComplete, Effect::ResponseComplete) => true,
            (Effect::Error { message: a }, Effect::Error { message: b }) => a == b,
            (Effect::Cancelled, Effect::Cancelled) => true,
            _ => false,
        }
    }
}

// ============================================================================
// Transition Result
// ============================================================================

/// Result of handling an event.
#[derive(Debug, Clone, Default)]
pub struct Transition {
    /// Effects to execute (in order).
    pub effects: Vec<Effect>,
}

impl Transition {
    fn none() -> Self {
        Transition { effects: vec![] }
    }

    fn with(effects: Vec<Effect>) -> Self {
        Transition { effects }
    }

    fn single(effect: Effect) -> Self {
        Transition {
            effects: vec![effect],
        }
    }
}

// ============================================================================
// The Agent FSM
// ============================================================================

/// The agent finite state machine.
///
/// # Usage
///
/// ```ignore
/// let mut agent = Agent::new();
///
/// // User sends a message
/// let transition = agent.handle(Event::UserMessage(msg));
/// for effect in transition.effects {
///     execute_effect(effect, &agent); // caller implements this
/// }
///
/// // Stream starts
/// let transition = agent.handle(Event::StreamStart);
/// // ... etc
/// ```
#[derive(Debug, Clone)]
pub struct Agent {
    state: State,
    context: Context,
}

impl Agent {
    /// Create a new agent in Idle state.
    pub fn new() -> Self {
        Agent {
            state: State::Idle,
            context: Context::default(),
        }
    }

    /// Current state (for UI, debugging, assertions).
    pub fn state(&self) -> &State {
        &self.state
    }

    /// Read-only access to context.
    pub fn context(&self) -> &Context {
        &self.context
    }

    /// Mutable access to context (for caller to read conversation, etc.)
    pub fn context_mut(&mut self) -> &mut Context {
        &mut self.context
    }

    /// Check if the agent is idle (can accept new conversations).
    pub fn is_idle(&self) -> bool {
        self.state == State::Idle
    }

    /// Check if a request is in flight.
    pub fn is_busy(&self) -> bool {
        !self.is_idle()
    }

    /// Check if we're waiting for tool results.
    pub fn is_awaiting_tools(&self) -> bool {
        self.state == State::Idle && !self.context.pending_tools.is_empty()
    }

    /// Push accumulated tool results to conversation as tool response messages.
    fn push_tool_results_to_conversation(&mut self) {
        for result in self.context.tool_results.drain(..) {
            let result_str = match result.output {
                ToolOutput::Success(s) => s,
                ToolOutput::Error(e) => format!("Error: {}", e),
            };
            let response = ToolResponse::new(result.call_id, result_str);
            // ToolResponse can be converted into ChatMessage
            self.context.conversation.push(response.into());
        }
    }

    /// Handle an event, returning effects to execute.
    ///
    /// This is the core state machine logic.
    /// Mutates internal state, returns effects as data.
    pub fn handle(&mut self, event: Event) -> Transition {
        // State-specific transitions
        match (&self.state, event) {
            // ================================================================
            // User messages
            // ================================================================
            (State::Idle, Event::UserMessage(msg)) => {
                self.context.conversation.push(msg);
                self.state = State::Sending;
                return Transition::single(Effect::StartRequest);
            }

            (_, Event::UserMessage(msg)) => {
                self.context.queued_messages.push_back(msg);
                return Transition::none();
            }

            // ================================================================
            // Tool results
            // ================================================================
            (State::Idle, Event::ToolResult(result)) => {
                self.context.pending_tools.remove(&result.call_id);
                self.context.tool_results.push(result);

                if self.context.pending_tools.is_empty() {
                    // Push tool result messages to conversation
                    self.push_tool_results_to_conversation();
                    self.state = State::Sending;
                    Transition::single(Effect::StartRequest)
                } else {
                    Transition::none()
                }
            }

            (_, Event::ToolResult(result)) => {
                self.context.pending_tools.remove(&result.call_id);
                self.context.tool_results.push(result);
                Transition::none()
            }

            // ================================================================
            // Stream events
            // ================================================================
            (State::Sending, Event::StreamStart) => {
                self.context.current_response.clear();
                self.state = State::Streaming;
                Transition::none()
            }

            (_, Event::StreamStart) => Transition::none(),

            (State::Streaming, Event::StreamChunk(chunk)) => {
                self.context.current_response.push_str(&chunk.content);
                Transition::single(Effect::EmitChunk {
                    content: chunk.content,
                })
            }

            // A stream chunk when we're not streaming is ignored.
            (_, Event::StreamChunk(_)) => Transition::none(),

            (State::Streaming, Event::StreamEnd { tool_calls }) => {
                // Build assistant message with text content AND tool calls
                let mut parts = vec![];
                let response_text = std::mem::take(&mut self.context.current_response);
                if !response_text.is_empty() {
                    parts.push(ContentPart::Text(response_text));
                }
                for call in &tool_calls {
                    parts.push(ContentPart::ToolCall(call.clone()));
                }
                if !parts.is_empty() {
                    let content = MessageContent::from_parts(parts);
                    let assistant_msg = ChatMessage::assistant(content);
                    self.context.conversation.push(assistant_msg);
                }

                // Check for queued messages BEFORE draining
                let had_queued_messages = !self.context.queued_messages.is_empty();

                // Always drain queued messages to conversation (they'll be included in next request)
                for msg in self.context.queued_messages.drain(..) {
                    self.context.conversation.push(msg);
                }

                // Add new tools to pending
                let has_new_tools = !tool_calls.is_empty();
                let tools_to_execute: Vec<ToolCall> = tool_calls
                    .into_iter()
                    .map(|call| {
                        let cloned = call.clone();
                        self.context
                            .pending_tools
                            .insert(call.call_id.clone(), call);
                        cloned
                    })
                    .collect();

                // Decide next action
                if has_new_tools {
                    // New tools to execute - go to Idle and request execution
                    // unless we have queued messages
                    if had_queued_messages {
                        self.state = State::Sending;
                        Transition::with(vec![
                            Effect::ResponseComplete,
                            Effect::ExecuteTools {
                                calls: tools_to_execute,
                            },
                            Effect::StartRequest,
                        ])
                    } else {
                        self.state = State::Idle;
                        Transition::with(vec![
                            Effect::ResponseComplete,
                            Effect::ExecuteTools {
                                calls: tools_to_execute,
                            },
                        ])
                    }
                } else if had_queued_messages {
                    // No new tools, but we had queued messages - continue the conversation
                    self.state = State::Sending;
                    Transition::with(vec![Effect::ResponseComplete, Effect::StartRequest])
                } else if self.context.pending_tools.is_empty()
                    && !self.context.tool_results.is_empty()
                {
                    // No new tools, no queued messages, but we have completed tool results
                    // (from ToolResult events that arrived while streaming)
                    self.push_tool_results_to_conversation();
                    self.state = State::Sending;
                    Transition::with(vec![Effect::ResponseComplete, Effect::StartRequest])
                } else {
                    // No tools, no queued messages, no pending results - done
                    self.state = State::Idle;
                    Transition::single(Effect::ResponseComplete)
                }
            }

            // ================================================================
            // Request failed
            // ================================================================
            (State::Idle, Event::RequestFailed { error }) => {
                self.state = State::Idle;
                Transition::single(Effect::Error { message: error })
            }

            (State::Sending, Event::RequestFailed { error }) => {
                self.state = State::Idle;
                Transition::single(Effect::Error { message: error })
            }

            (State::Streaming, Event::RequestFailed { error }) => {
                self.context.current_response.clear();
                self.state = State::Idle;
                Transition::single(Effect::Error { message: error })
            }

            // ================================================================
            // Cancel - only cancels current request/stream, not pending tools
            // ================================================================
            (State::Sending, Event::Cancel) => {
                self.state = State::Idle;
                Transition::single(Effect::Cancelled)
            }

            (State::Streaming, Event::Cancel) => {
                self.context.current_response.clear();
                self.state = State::Idle;
                Transition::single(Effect::Cancelled)
            }

            (_, _) => Transition::none(),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use crate::ai::prompts::AIPrompts;

    use super::*;
    use genai::chat::ChatRole;

    fn user_msg(content: &str) -> ChatMessage {
        ChatMessage::user(content)
    }

    fn make_tool_call(id: &str, name: &str) -> ToolCall {
        ToolCall {
            call_id: id.to_string(),
            fn_name: name.to_string(),
            fn_arguments: serde_json::json!({}),
            thought_signatures: None,
        }
    }

    #[test]
    fn idle_to_sending_on_user_message() {
        let mut agent = Agent::new();
        assert_eq!(agent.state(), &State::Idle);

        let t = agent.handle(Event::UserMessage(user_msg("hello")));

        assert_eq!(agent.state(), &State::Sending);
        assert_eq!(t.effects, vec![Effect::StartRequest]);
        assert_eq!(agent.context().conversation.len(), 1);
    }

    #[test]
    fn messages_queued_during_sending() {
        let mut agent = Agent::new();
        agent.handle(Event::UserMessage(user_msg("first")));

        let t = agent.handle(Event::UserMessage(user_msg("second")));

        assert_eq!(agent.state(), &State::Sending);
        assert!(t.effects.is_empty());
        assert_eq!(agent.context().queued_messages.len(), 1);
    }

    #[test]
    fn sending_to_streaming_on_stream_start() {
        let mut agent = Agent::new();
        agent.handle(Event::UserMessage(user_msg("hello")));

        let t = agent.handle(Event::StreamStart);

        assert_eq!(agent.state(), &State::Streaming);
        assert!(t.effects.is_empty());
    }

    #[test]
    fn streaming_emits_chunks() {
        let mut agent = Agent::new();
        agent.handle(Event::UserMessage(user_msg("hello")));
        agent.handle(Event::StreamStart);

        let t = agent.handle(Event::StreamChunk(StreamChunk {
            content: "Hi there!".to_string(),
        }));

        assert_eq!(agent.state(), &State::Streaming);
        assert_eq!(
            t.effects,
            vec![Effect::EmitChunk {
                content: "Hi there!".to_string()
            }]
        );
        assert_eq!(agent.context().current_response, "Hi there!");
    }

    #[test]
    fn stream_end_no_tools_goes_idle() {
        let mut agent = Agent::new();
        agent.handle(Event::UserMessage(user_msg("hello")));
        agent.handle(Event::StreamStart);
        agent.handle(Event::StreamChunk(StreamChunk {
            content: "Hi!".to_string(),
        }));

        let t = agent.handle(Event::StreamEnd { tool_calls: vec![] });

        assert_eq!(agent.state(), &State::Idle);
        assert_eq!(t.effects, vec![Effect::ResponseComplete]);
        assert_eq!(agent.context().conversation.len(), 2); // user + assistant
    }

    #[test]
    fn stream_end_with_queued_message_goes_sending() {
        let mut agent = Agent::new();
        agent.handle(Event::UserMessage(user_msg("first")));
        agent.handle(Event::UserMessage(user_msg("second"))); // queued
        agent.handle(Event::StreamStart);
        agent.handle(Event::StreamChunk(StreamChunk {
            content: "Response".to_string(),
        }));

        let t = agent.handle(Event::StreamEnd { tool_calls: vec![] });

        assert_eq!(agent.state(), &State::Sending);
        assert_eq!(
            t.effects,
            vec![Effect::ResponseComplete, Effect::StartRequest]
        );
        assert!(agent.context().queued_messages.is_empty());
        assert_eq!(agent.context().conversation.len(), 3); // first + response + second
    }

    #[test]
    fn stream_end_with_tools_goes_idle_with_pending() {
        let mut agent = Agent::new();
        agent.handle(Event::UserMessage(user_msg("what time is it?")));
        agent.handle(Event::StreamStart);
        agent.handle(Event::StreamChunk(StreamChunk {
            content: "Let me check.".to_string(),
        }));

        let tc = make_tool_call("call_123", "get_time");
        let t = agent.handle(Event::StreamEnd {
            tool_calls: vec![tc.clone()],
        });

        assert_eq!(agent.state(), &State::Idle);
        assert!(agent.is_awaiting_tools());
        assert_eq!(
            t.effects,
            vec![
                Effect::ResponseComplete,
                Effect::ExecuteTools { calls: vec![tc] }
            ]
        );
        assert_eq!(agent.context().pending_tools.len(), 1);
    }

    #[test]
    fn tool_result_completes_and_starts_request() {
        let mut agent = Agent::new();
        agent.handle(Event::UserMessage(user_msg("what time is it?")));
        agent.handle(Event::StreamStart);
        agent.handle(Event::StreamEnd {
            tool_calls: vec![make_tool_call("call_123", "get_time")],
        });

        // Now in Idle with pending tools
        assert!(agent.is_awaiting_tools());

        let t = agent.handle(Event::ToolResult(ToolResult {
            call_id: "call_123".to_string(),
            output: ToolOutput::Success("12:34 PM".to_string()),
        }));

        assert_eq!(agent.state(), &State::Sending);
        assert!(!agent.is_awaiting_tools());
        assert_eq!(t.effects, vec![Effect::StartRequest]);
        assert!(agent.context().pending_tools.is_empty());
        // Tool results are now pushed to conversation as ToolResponse messages
        assert!(agent.context().tool_results.is_empty());
        // Conversation: user msg, assistant msg (with tool call), tool response
        assert_eq!(agent.context().conversation.len(), 3);
    }

    #[test]
    fn multiple_tools_waits_for_all() {
        let mut agent = Agent::new();
        agent.handle(Event::UserMessage(user_msg("query")));
        agent.handle(Event::StreamStart);
        agent.handle(Event::StreamEnd {
            tool_calls: vec![
                make_tool_call("call_1", "tool_a"),
                make_tool_call("call_2", "tool_b"),
            ],
        });

        // First result - still waiting
        let t = agent.handle(Event::ToolResult(ToolResult {
            call_id: "call_1".to_string(),
            output: ToolOutput::Success("result_a".to_string()),
        }));

        assert_eq!(agent.state(), &State::Idle);
        assert!(agent.is_awaiting_tools());
        assert!(t.effects.is_empty());

        // Second result - now complete
        let t = agent.handle(Event::ToolResult(ToolResult {
            call_id: "call_2".to_string(),
            output: ToolOutput::Success("result_b".to_string()),
        }));

        assert_eq!(agent.state(), &State::Sending);
        assert!(!agent.is_awaiting_tools());
        assert_eq!(t.effects, vec![Effect::StartRequest]);
    }

    #[test]
    fn cancel_from_streaming() {
        let mut agent = Agent::new();
        agent.handle(Event::UserMessage(user_msg("hello")));
        agent.handle(Event::StreamStart);

        let t = agent.handle(Event::Cancel);

        assert_eq!(agent.state(), &State::Idle);
        assert_eq!(t.effects, vec![Effect::Cancelled]);
    }

    #[test]
    fn request_failed_goes_idle() {
        let mut agent = Agent::new();
        agent.handle(Event::UserMessage(user_msg("hello")));

        let t = agent.handle(Event::RequestFailed {
            error: "network error".to_string(),
        });

        assert_eq!(agent.state(), &State::Idle);
        assert_eq!(
            t.effects,
            vec![Effect::Error {
                message: "network error".to_string()
            }]
        );
    }

    #[test]
    fn assistant_message_contains_tool_calls() {
        let mut agent = Agent::new();
        agent.handle(Event::UserMessage(user_msg("what time is it?")));
        agent.handle(Event::StreamStart);
        agent.handle(Event::StreamChunk(StreamChunk {
            content: "Let me check.".to_string(),
        }));

        let tc = make_tool_call("call_123", "get_time");
        agent.handle(Event::StreamEnd {
            tool_calls: vec![tc.clone()],
        });

        // Check the assistant message in conversation has both text and tool call
        let assistant_msg = &agent.context().conversation[1];
        assert!(matches!(assistant_msg.role, ChatRole::Assistant));
        let parts = assistant_msg.content.clone().into_parts();
        assert_eq!(parts.len(), 2);
        assert!(matches!(&parts[0], ContentPart::Text(t) if t == "Let me check."));
        assert!(
            matches!(&parts[1], ContentPart::ToolCall(c) if c.call_id == tc.call_id && c.fn_name == tc.fn_name)
        );
    }

    #[test]
    fn queued_messages_drained_even_with_new_tools() {
        let mut agent = Agent::new();
        agent.handle(Event::UserMessage(user_msg("first")));
        agent.handle(Event::UserMessage(user_msg("second"))); // queued
        agent.handle(Event::StreamStart);
        agent.handle(Event::StreamChunk(StreamChunk {
            content: "Response".to_string(),
        }));

        let tc = make_tool_call("call_123", "some_tool");
        agent.handle(Event::StreamEnd {
            tool_calls: vec![tc],
        });

        // Even though there are new tool calls, queued messages should be drained
        assert!(agent.context().queued_messages.is_empty());
        // Conversation: first msg, assistant msg, second msg (the queued one)
        assert_eq!(agent.context().conversation.len(), 3);
        // Verify the third message is the queued "second"
        let third_msg = &agent.context().conversation[2];
        assert!(matches!(third_msg.role, ChatRole::User));
    }

    #[test]
    fn tool_results_while_streaming_triggers_request_at_stream_end() {
        let mut agent = Agent::new();
        agent.handle(Event::UserMessage(user_msg("query")));
        agent.handle(Event::StreamStart);
        agent.handle(Event::StreamEnd {
            tool_calls: vec![make_tool_call("call_1", "tool_a")],
        });

        // In Idle awaiting tools
        assert!(agent.is_awaiting_tools());

        // User sends new message while we're idle with pending tools
        agent.handle(Event::UserMessage(user_msg("continue")));

        // Now should be in Sending (user message triggers new request)
        assert_eq!(agent.state(), &State::Sending);

        // Simulate new stream
        agent.handle(Event::StreamStart);
        agent.handle(Event::StreamChunk(StreamChunk {
            content: "More response".to_string(),
        }));

        // Tool result arrives WHILE streaming
        agent.handle(Event::ToolResult(ToolResult {
            call_id: "call_1".to_string(),
            output: ToolOutput::Success("result_a".to_string()),
        }));

        // Still streaming - tool result just accumulated
        assert_eq!(agent.state(), &State::Streaming);
        assert!(agent.context().pending_tools.is_empty());
        assert_eq!(agent.context().tool_results.len(), 1);

        // Stream ends with no new tools
        let t = agent.handle(Event::StreamEnd { tool_calls: vec![] });

        // Should push tool results to conversation and start new request
        assert_eq!(agent.state(), &State::Sending);
        assert_eq!(
            t.effects,
            vec![Effect::ResponseComplete, Effect::StartRequest]
        );
        assert!(agent.context().tool_results.is_empty());
    }
}
