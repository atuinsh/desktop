use std::{fmt, ops::Deref, sync::Arc};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export)]
pub enum ModelSelection {
    AtuinHub { model: String, uri: Option<String> },
    Claude { model: String },
    OpenAI { model: String, uri: Option<String> },
    Ollama { model: String, uri: Option<String> },
}

impl fmt::Display for ModelSelection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelSelection::AtuinHub { model, uri } => match uri {
                Some(uri) => write!(f, "atuinhub::{model}::{}", uri.deref()),
                None => write!(f, "atuinhub::{model}::default"),
            },
            ModelSelection::Claude { model } => write!(f, "claude::{model}::default"),
            ModelSelection::OpenAI { model, uri } => match uri {
                Some(uri) => write!(f, "openai::{model}::{}", uri.deref()),
                None => write!(f, "openai::{model}::default"),
            },
            ModelSelection::Ollama { model, uri } => match uri {
                Some(uri) => write!(f, "ollama::{model}::{}", uri.deref()),
                None => write!(f, "ollama::{model}::http://localhost:11434"),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AIMessage {
    pub role: AIMessageRole,
    pub content: AIMessageContent,
}

impl From<genai::chat::ChatMessage> for AIMessage {
    fn from(message: genai::chat::ChatMessage) -> Self {
        Self {
            role: message.role.into(),
            content: message.content.into(),
        }
    }
}

impl From<AIMessageContentPart> for AIMessage {
    fn from(part: AIMessageContentPart) -> Self {
        Self {
            role: AIMessageRole::User,
            content: AIMessageContent::from_parts(vec![part]),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AIMessageRole {
    System,
    User,
    Assistant,
    Tool,
}

impl From<genai::chat::ChatRole> for AIMessageRole {
    fn from(role: genai::chat::ChatRole) -> Self {
        match role {
            genai::chat::ChatRole::System => AIMessageRole::System,
            genai::chat::ChatRole::User => AIMessageRole::User,
            genai::chat::ChatRole::Assistant => AIMessageRole::Assistant,
            genai::chat::ChatRole::Tool => AIMessageRole::Tool,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AIMessageContent {
    parts: Vec<AIMessageContentPart>,
}

impl AIMessageContent {
    pub fn from_parts(parts: Vec<AIMessageContentPart>) -> Self {
        Self { parts }
    }
}

impl From<genai::chat::MessageContent> for AIMessageContent {
    fn from(content: genai::chat::MessageContent) -> Self {
        Self {
            parts: content
                .into_parts()
                .into_iter()
                .map(|part| part.into())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export)]
pub enum AIMessageContentPart {
    Text(String),
    Binary(AIBinary),
    ToolCall(AIToolCall),
    ToolResponse(AIToolResponse),
    ThoughtSignature(String),
}

impl From<genai::chat::ContentPart> for AIMessageContentPart {
    fn from(part: genai::chat::ContentPart) -> Self {
        match part {
            genai::chat::ContentPart::Text(text) => AIMessageContentPart::Text(text.to_string()),
            genai::chat::ContentPart::Binary(binary) => AIMessageContentPart::Binary(binary.into()),
            genai::chat::ContentPart::ToolCall(tool_call) => {
                AIMessageContentPart::ToolCall(tool_call.into())
            }
            genai::chat::ContentPart::ToolResponse(tool_response) => {
                AIMessageContentPart::ToolResponse(tool_response.into())
            }
            genai::chat::ContentPart::ThoughtSignature(thought_signature) => {
                AIMessageContentPart::ThoughtSignature(thought_signature)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AIBinary {
    content_type: String,
    source: AIBinarySource,
    name: Option<String>,
}

impl From<genai::chat::Binary> for AIBinary {
    fn from(binary: genai::chat::Binary) -> Self {
        Self {
            content_type: binary.content_type,
            source: binary.source.into(),
            name: binary.name,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export)]
pub enum AIBinarySource {
    Url(String),
    Base64(Arc<str>),
}

impl From<genai::chat::BinarySource> for AIBinarySource {
    fn from(source: genai::chat::BinarySource) -> Self {
        match source {
            genai::chat::BinarySource::Url(url) => AIBinarySource::Url(url),
            genai::chat::BinarySource::Base64(base64) => AIBinarySource::Base64(base64),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AIToolCall {
    pub id: String,
    pub name: String,
    pub args: Value,
}

impl From<genai::chat::ToolCall> for AIToolCall {
    fn from(tool_call: genai::chat::ToolCall) -> Self {
        Self {
            id: tool_call.call_id,
            name: tool_call.fn_name,
            args: tool_call.fn_arguments,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AIToolResponse {
    call_id: String,
    result: String,
}

impl From<genai::chat::ToolResponse> for AIToolResponse {
    fn from(tool_response: genai::chat::ToolResponse) -> Self {
        Self {
            call_id: tool_response.call_id,
            result: tool_response.content,
        }
    }
}
