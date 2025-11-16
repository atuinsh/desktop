use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::context::ResolvedContext;
use crate::execution::BlockOutput;

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
        output: BlockOutput,
    },

    ClientPrompt {
        #[serde(rename = "executionId")]
        execution_id: Uuid,
        #[serde(rename = "promptId")]
        prompt_id: Uuid,
        prompt: ClientPrompt,
    },
}

impl From<BlockOutput> for DocumentBridgeMessage {
    fn from(output: BlockOutput) -> Self {
        DocumentBridgeMessage::BlockOutput {
            block_id: output.block_id,
            output,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export)]
pub enum PromptOptionVariant {
    Flat,
    Light,
    Shadow,
    Solid,
    Bordered,
    Faded,
    Ghost,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export)]
pub enum PromptOptionColor {
    Default,
    Primary,
    Secondary,
    Success,
    Warning,
    Danger,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct PromptOption {
    label: String,
    value: String,
    variant: Option<PromptOptionVariant>,
    color: Option<PromptOptionColor>,
}

impl PromptOption {
    pub fn new(label: &str, value: &str) -> Self {
        Self {
            label: label.to_string(),
            value: value.to_string(),
            variant: None,
            color: None,
        }
    }

    pub fn variant(mut self, variant: PromptOptionVariant) -> Self {
        self.variant = Some(variant);
        self
    }

    pub fn color(mut self, color: PromptOptionColor) -> Self {
        self.color = Some(color);
        self
    }
}

impl From<(&str, &str)> for PromptOption {
    fn from((label, value): (&str, &str)) -> Self {
        Self::new(label, value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export)]
pub enum PromptIcon {
    Info,
    Warning,
    Error,
    Success,
    Question,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export)]
pub enum PromptInput {
    String,
    Text,
    Dropdown(Vec<(String, String)>),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ClientPrompt {
    title: String,
    prompt: String,
    icon: Option<PromptIcon>,
    input: Option<PromptInput>,
    options: Vec<PromptOption>,
}

impl ClientPrompt {
    pub fn new(title: &str, prompt: &str) -> Self {
        Self {
            title: title.to_string(),
            prompt: prompt.to_string(),
            icon: None,
            input: None,
            options: Vec::new(),
        }
    }

    pub fn icon(mut self, icon: PromptIcon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn input(mut self, input: PromptInput) -> Self {
        self.input = Some(input);
        self
    }

    pub fn option(mut self, option: PromptOption) -> Self {
        self.options.push(option);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ClientPromptResult {
    pub button: String,
    pub value: Option<String>,
}
