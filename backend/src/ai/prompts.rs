use minijinja::{Environment, UndefinedBehavior};
use serde::Serialize;

pub struct AIPrompts;

const SYS_PROMPT_SOURCE: &str = include_str!("system_prompt.minijinja.txt");

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct SystemPromptContext {
    prompt_type: SystemPromptType,
    block_summary: String,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum SystemPromptType {
    Assistant,
    Generator,
}

impl AIPrompts {
    pub fn assistant_system_prompt(block_summary: &str) -> Result<String, minijinja::Error> {
        let mut env = Environment::new();
        env.set_trim_blocks(true);
        env.set_undefined_behavior(UndefinedBehavior::Strict);

        let context = SystemPromptContext {
            prompt_type: SystemPromptType::Assistant,
            block_summary: block_summary.to_string(),
        };

        env.render_str(SYS_PROMPT_SOURCE, &context)
    }

    pub fn generator_system_prompt(block_summary: &str) -> String {
        todo!()
    }
}
