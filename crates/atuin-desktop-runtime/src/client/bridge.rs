use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::context::ResolvedContext;
use crate::execution::BlockOutput;

/// Messages sent from the runtime to the client application
///
/// These messages communicate execution state, output, and context updates
/// to the desktop application frontend.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum DocumentBridgeMessage {
    BlockContextUpdate {
        #[serde(rename = "blockId")]
        block_id: Uuid,
        context: ResolvedContext,
    },

    BlockStateChanged {
        #[serde(rename = "blockId")]
        block_id: Uuid,
        state: serde_json::Value,
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

/// Visual variant for prompt options (buttons)
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

/// Color scheme for prompt options (buttons)
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

/// A button option in a client prompt dialog
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct PromptOption {
    label: String,
    value: String,
    variant: Option<PromptOptionVariant>,
    color: Option<PromptOptionColor>,
}

impl PromptOption {
    /// Create a new prompt option with label and value
    pub fn new(label: &str, value: &str) -> Self {
        Self {
            label: label.to_string(),
            value: value.to_string(),
            variant: None,
            color: None,
        }
    }

    /// Set the visual variant for this option
    pub fn variant(mut self, variant: PromptOptionVariant) -> Self {
        self.variant = Some(variant);
        self
    }

    /// Set the color scheme for this option
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

/// Icon types for client prompts
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

/// Input types for client prompts
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
#[ts(export)]
pub enum PromptInput {
    String,
    Text,
    Dropdown(Vec<(String, String)>),
}

/// A prompt displayed to the user in the client application
///
/// Prompts can include text input fields, dropdowns, and action buttons.
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
    /// Create a new client prompt with a title and message
    pub fn new(title: &str, prompt: &str) -> Self {
        Self {
            title: title.to_string(),
            prompt: prompt.to_string(),
            icon: None,
            input: None,
            options: Vec::new(),
        }
    }

    /// Set the icon for this prompt
    pub fn icon(mut self, icon: PromptIcon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Set the input type for this prompt
    pub fn input(mut self, input: PromptInput) -> Self {
        self.input = Some(input);
        self
    }

    /// Add an option (button) to this prompt
    pub fn option(mut self, option: PromptOption) -> Self {
        self.options.push(option);
        self
    }
}

/// The result from a client prompt interaction
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ClientPromptResult {
    /// The value of the button that was clicked
    pub button: String,
    /// The value entered in an input field, if any
    pub value: Option<String>,
}
