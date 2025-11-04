use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::runtime::blocks::{document::block_context::ResolvedContext, handler};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum DocumentBridgeMessage {
    BlockContextUpdate {
        #[serde(rename = "blockId")]
        block_id: Uuid,
        context: ResolvedContext,
    },

    BlockOutput {
        #[serde(rename = "blockId")]
        block_id: Uuid,
        output: handler::BlockOutput,
    },

    AgentEvent {
        #[serde(rename = "blockId")]
        block_id: Uuid,
        event: AgentUiEvent,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum AgentUiEvent {
    AssistantDelta { text: String },
    AssistantMessage { text: String },
    ToolCall { name: String, args_json: String },
    ToolResult { name: String, ok: bool, result_json: String },
    HitlRequested { id: String, prompt: String, options_json: String },
    HitlResolved { id: String, decision: String, data_json: Option<String> },
}

impl From<handler::BlockOutput> for DocumentBridgeMessage {
    fn from(output: handler::BlockOutput) -> Self {
        DocumentBridgeMessage::BlockOutput {
            block_id: output.block_id,
            output,
        }
    }
}
