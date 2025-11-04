use serde::{Deserialize, Serialize};

/// Runtime configuration for block execution
/// 
/// This holds settings that are resolved at the Tauri command layer
/// where we have access to app handle and KV store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// AI settings for agent blocks
    pub ai: Option<AiConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    /// Whether AI is enabled
    pub enabled: bool,
    /// API key for the AI provider (e.g., OpenAI)
    pub api_key: Option<String>,
    /// Base URL for the AI API (optional, for custom endpoints)
    pub api_endpoint: Option<String>,
    /// Model name to use (e.g., "gpt-4o-mini")
    pub model: Option<String>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            ai: None,
        }
    }
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_key: None,
            api_endpoint: None,
            model: Some("gpt-4o-mini".to_string()),
        }
    }
}

impl RuntimeConfig {
    /// Create a new runtime config with AI settings
    pub fn with_ai(ai_config: AiConfig) -> Self {
        Self {
            ai: Some(ai_config),
        }
    }

    /// Get AI config, returning default if not set
    pub fn ai_config(&self) -> AiConfig {
        self.ai.clone().unwrap_or_default()
    }
}
