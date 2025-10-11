use crate::runtime::blocks::handler::{ContextProvider, ExecutionContext};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct LocalVar {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub name: String,
}

pub struct LocalVarHandler;

#[async_trait]
impl ContextProvider for LocalVarHandler {
    type Block = LocalVar;

    fn block_type(&self) -> &'static str {
        "local-var"
    }

    async fn apply_context(
        &self,
        block: &LocalVar,
        context: &mut ExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !block.name.is_empty() {
            // Get the value from output storage (same as get_template_var command)
            let value = if let Some(output_storage) = &context.output_storage {
                output_storage
                    .read()
                    .await
                    .get(&context.runbook_id.to_string())
                    .and_then(|vars| vars.get(&block.name))
                    .cloned()
                    .unwrap_or_default()
            } else {
                // Fallback to empty if no output storage available
                String::new()
            };

            // Add the variable to the execution context for template substitution
            context.variables.insert(block.name.clone(), value);
        }
        Ok(())
    }
}

impl LocalVar {
    #[allow(dead_code)] // Used for JSON parsing but not currently called
    pub fn from_document(block_data: &serde_json::Value) -> Result<Self, String> {
        let id = block_data
            .get("id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or("Invalid or missing id")?;

        let name = block_data
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or("Missing name")?
            .to_string();

        Ok(LocalVar::builder().id(id).name(name).build())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_local_var_handler_empty_name() {
        let handler = LocalVarHandler;
        let local_var = LocalVar::builder().id(Uuid::new_v4()).name("").build();

        let mut context = ExecutionContext::default();
        handler
            .apply_context(&local_var, &mut context)
            .await
            .unwrap();

        // Should not add anything to variables for empty name
        assert!(context.variables.is_empty());
    }

    #[tokio::test]
    async fn test_local_var_handler_with_name() {
        let handler = LocalVarHandler;
        let local_var = LocalVar::builder()
            .id(Uuid::new_v4())
            .name("test_var")
            .build();

        let mut context = ExecutionContext::default();
        handler
            .apply_context(&local_var, &mut context)
            .await
            .unwrap();

        // Should add empty value for the variable (since no stored value in test)
        assert_eq!(context.variables.get("test_var"), Some(&String::new()));
    }

    #[tokio::test]
    async fn test_local_var_handler_block_type() {
        let handler = LocalVarHandler;
        assert_eq!(handler.block_type(), "local-var");
    }

    #[tokio::test]
    async fn test_local_var_serialization() {
        let local_var = LocalVar::builder()
            .id(Uuid::new_v4())
            .name("test_var")
            .build();

        // Test serialization roundtrip
        let json = serde_json::to_string(&local_var).unwrap();
        let deserialized: LocalVar = serde_json::from_str(&json).unwrap();

        assert_eq!(local_var.name, deserialized.name);
        assert_eq!(local_var.id, deserialized.id);
    }

    #[tokio::test]
    async fn test_local_var_valid_name_pattern() {
        let handler = LocalVarHandler;

        // Test valid variable names (same pattern as frontend validation)
        let valid_names = vec!["test_var", "TEST123", "var_name_123", "a", "A"];

        for name in valid_names {
            let local_var = LocalVar::builder().id(Uuid::new_v4()).name(name).build();

            let mut context = ExecutionContext::default();
            let result = handler.apply_context(&local_var, &mut context).await;

            assert!(result.is_ok(), "Should handle valid name: {}", name);
            assert_eq!(context.variables.get(name), Some(&String::new()));
        }
    }
}
