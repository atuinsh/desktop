use minijinja::{Environment, UndefinedBehavior};
use serde::Serialize;
use uuid::Uuid;

use crate::ai::types::BlockInfo;

pub struct AIPrompts;

const SYS_PROMPT_SOURCE: &str = include_str!("system_prompt.minijinja.txt");

#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    #[error("Failed to process system prompt template: {0}")]
    SystemPromptTemplateError(#[from] minijinja::Error),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemPromptContext {
    prompt_type: SystemPromptType,
    block_infos: Vec<BlockInfo>,
    current_document: Option<serde_json::Value>,
    insert_after: Option<Uuid>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum SystemPromptType {
    Assistant,
    Generator,
}

impl AIPrompts {
    pub fn assistant_system_prompt(block_infos: Vec<BlockInfo>) -> Result<String, PromptError> {
        let mut env = Environment::new();
        env.set_trim_blocks(true);
        env.set_undefined_behavior(UndefinedBehavior::Strict);

        let context = SystemPromptContext {
            prompt_type: SystemPromptType::Assistant,
            block_infos,
            current_document: None,
            insert_after: None,
        };

        env.render_str(SYS_PROMPT_SOURCE, &context)
            .map_err(PromptError::SystemPromptTemplateError)
    }

    pub fn generator_system_prompt(
        block_infos: Vec<BlockInfo>,
        current_document: serde_json::Value,
        insert_after: Uuid,
    ) -> Result<String, PromptError> {
        let mut env = Environment::new();
        env.set_trim_blocks(true);
        env.set_undefined_behavior(UndefinedBehavior::Strict);

        let context = SystemPromptContext {
            prompt_type: SystemPromptType::Generator,
            block_infos,
            current_document: Some(current_document),
            insert_after: Some(insert_after),
        };

        env.render_str(SYS_PROMPT_SOURCE, &context)
            .map_err(PromptError::SystemPromptTemplateError)
    }
}
