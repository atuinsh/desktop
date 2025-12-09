//! Pause block implementation
//!
//! The Pause block halts serial workflow execution at a designated point,
//! allowing users to perform manual tasks before resuming. The workflow
//! stops (not suspends) - it's a clean termination at a known point that
//! can be resumed.

use crate::blocks::{Block, BlockBehavior, FromDocument};
use crate::execution::{ExecutionContext, ExecutionHandle};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

/// A block that pauses serial workflow execution
///
/// The pause block can operate in two modes:
/// 1. Always pause: When `pause_if_truthy` is false (default), always pause
/// 2. Conditional pause: When `pause_if_truthy` is true, pause only if
///    the condition evaluates to a truthy value
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Pause {
    #[builder(setter(into))]
    pub id: Uuid,

    /// Optional MiniJinja expression to evaluate
    /// If empty, the block always pauses (unconditional)
    #[builder(default, setter(into))]
    pub condition: String,

    /// Whether to pause if the condition evaluates to truthy
    /// Only relevant when condition is non-empty
    #[builder(default)]
    pub pause_if_truthy: bool,
}

impl FromDocument for Pause {
    fn from_document(block_data: &serde_json::Value) -> Result<Self, String> {
        let id = block_data
            .get("id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or("Invalid or missing id")?;

        let props = block_data
            .get("props")
            .and_then(|p| p.as_object())
            .ok_or("Invalid or missing props")?;

        let condition = props
            .get("condition")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let pause_if_truthy = props
            .get("pauseIfTruthy")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        Ok(Pause::builder()
            .id(id)
            .condition(condition)
            .pause_if_truthy(pause_if_truthy)
            .build())
    }
}

/// Check if a string value is "truthy"
///
/// Truthy values are: "true", "1", "yes", or any non-zero number
/// Falsy values are: "false", "0", "no", "", or anything else
fn is_truthy(value: &str) -> bool {
    let trimmed = value.trim().to_lowercase();
    match trimmed.as_str() {
        "true" | "1" | "yes" => true,
        "false" | "0" | "no" | "" => false,
        _ => {
            // Try to parse as number - non-zero is truthy
            trimmed.parse::<f64>().map(|n| n != 0.0).unwrap_or(false)
        }
    }
}

#[async_trait]
impl BlockBehavior for Pause {
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::Pause(self)
    }

    async fn execute(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        tracing::trace!("Executing Pause block {id}", id = self.id);

        let _ = context.block_started().await;

        // Determine if we should pause
        // - If pause_if_truthy is false (UI shows "Always") -> always pause
        // - If pause_if_truthy is true (UI shows "If condition") -> pause only if condition is truthy
        let should_pause = if !self.pause_if_truthy {
            // "Always" mode - unconditionally pause
            true
        } else {
            // "If condition" mode - evaluate the condition
            if self.condition.trim().is_empty() {
                // No condition provided - treat as falsy (don't pause)
                false
            } else {
                match context.context_resolver.resolve_template(&self.condition) {
                    Ok(result) => is_truthy(&result),
                    Err(e) => {
                        // Template evaluation failed - report error and don't pause
                        tracing::error!("Pause block condition evaluation failed: {e}");
                        let _ = context
                            .block_failed(format!("Condition evaluation failed: {e}"))
                            .await;
                        return Err(e.into());
                    }
                }
            }
        };

        if should_pause {
            tracing::debug!("Pause block {id} is pausing execution", id = self.id);
            let _ = context.block_paused().await;
        } else {
            tracing::debug!(
                "Pause block {id} is not pausing (condition not met)",
                id = self.id
            );
            let _ = context.block_finished(None, true).await;
        }

        Ok(Some(context.handle()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_truthy() {
        // Truthy values
        assert!(is_truthy("true"));
        assert!(is_truthy("TRUE"));
        assert!(is_truthy("True"));
        assert!(is_truthy("1"));
        assert!(is_truthy("yes"));
        assert!(is_truthy("YES"));
        assert!(is_truthy("42"));
        assert!(is_truthy("-1"));
        assert!(is_truthy("3.14"));

        // Falsy values
        assert!(!is_truthy("false"));
        assert!(!is_truthy("FALSE"));
        assert!(!is_truthy("0"));
        assert!(!is_truthy("no"));
        assert!(!is_truthy("NO"));
        assert!(!is_truthy(""));
        assert!(!is_truthy("   "));
        assert!(!is_truthy("random string"));
        assert!(!is_truthy("0.0"));
    }

    #[test]
    fn test_pause_from_document_defaults() {
        let id = Uuid::new_v4();
        let json_data = serde_json::json!({
            "id": id.to_string(),
            "props": {},
            "type": "pause"
        });

        let pause = Pause::from_document(&json_data).unwrap();
        assert_eq!(pause.id, id);
        assert_eq!(pause.condition, "");
        assert!(!pause.pause_if_truthy);
    }

    #[test]
    fn test_pause_from_document_with_values() {
        let id = Uuid::new_v4();
        let json_data = serde_json::json!({
            "id": id.to_string(),
            "props": {
                "condition": "{{ var.error_count > 0 }}",
                "pauseIfTruthy": true
            },
            "type": "pause"
        });

        let pause = Pause::from_document(&json_data).unwrap();
        assert_eq!(pause.id, id);
        assert_eq!(pause.condition, "{{ var.error_count > 0 }}");
        assert!(pause.pause_if_truthy);
    }

    #[test]
    fn test_pause_from_document_missing_id() {
        let json_data = serde_json::json!({
            "props": {
            },
            "type": "pause"
        });

        let result = Pause::from_document(&json_data);
        assert!(result.is_err());
    }

    #[test]
    fn test_pause_serialization_roundtrip() {
        let original = Pause::builder()
            .id(Uuid::new_v4())
            .condition("{{ var.ready }}")
            .pause_if_truthy(true)
            .build();

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: Pause = serde_json::from_str(&json).unwrap();

        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.condition, deserialized.condition);
        assert_eq!(original.pause_if_truthy, deserialized.pause_if_truthy);
    }
}
