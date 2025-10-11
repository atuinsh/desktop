use crate::runtime::blocks::handler::{ContextProvider, ExecutionContext};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Var {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub name: String,

    #[builder(setter(into))]
    pub value: String,
}

pub struct VarHandler;

#[async_trait]
impl ContextProvider for VarHandler {
    type Block = Var;

    fn block_type(&self) -> &'static str {
        "var"
    }

    async fn apply_context(
        &self,
        block: &Var,
        context: &mut ExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Validate variable name
        if block.name.is_empty() {
            return Err("Variable name cannot be empty".into());
        }

        // Check for invalid characters in variable name (only allow alphanumeric and underscore)
        if !block.name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err("Variable names can only contain letters, numbers, and underscores".into());
        }

        // Add the variable to the execution context for template substitution
        context
            .variables
            .insert(block.name.clone(), block.value.clone());
        Ok(())
    }
}

impl Var {
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

        let value = block_data
            .get("value")
            .and_then(|v| v.as_str())
            .unwrap_or("") // Default to empty string if value is missing
            .to_string();

        Ok(Var::builder().id(id).name(name).value(value).build())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // Basic functionality tests
    #[tokio::test]
    async fn test_basic_var_context() {
        let handler = VarHandler;
        let var = Var::builder()
            .id(Uuid::new_v4())
            .name("TEST_VAR")
            .value("test_value")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&var, &mut context).await.unwrap();

        assert_eq!(
            context.variables.get("TEST_VAR"),
            Some(&"test_value".to_string())
        );
    }

    #[tokio::test]
    async fn test_block_type() {
        let handler = VarHandler;
        assert_eq!(handler.block_type(), "var");
    }

    // Edge cases
    #[tokio::test]
    async fn test_empty_value() {
        let handler = VarHandler;
        let var = Var::builder()
            .id(Uuid::new_v4())
            .name("EMPTY_VAR")
            .value("")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&var, &mut context).await.unwrap();

        assert_eq!(context.variables.get("EMPTY_VAR"), Some(&"".to_string()));
    }

    #[tokio::test]
    async fn test_empty_name_fails() {
        let handler = VarHandler;
        let var = Var::builder()
            .id(Uuid::new_v4())
            .name("")
            .value("test_value")
            .build();

        let mut context = ExecutionContext::default();
        let result = handler.apply_context(&var, &mut context).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Variable name cannot be empty"));
    }

    #[tokio::test]
    async fn test_invalid_name_with_spaces() {
        let handler = VarHandler;
        let var = Var::builder()
            .id(Uuid::new_v4())
            .name("INVALID NAME")
            .value("test_value")
            .build();

        let mut context = ExecutionContext::default();
        let result = handler.apply_context(&var, &mut context).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("can only contain letters, numbers, and underscores"));
    }

    #[tokio::test]
    async fn test_invalid_name_with_special_chars() {
        let handler = VarHandler;
        let var = Var::builder()
            .id(Uuid::new_v4())
            .name("INVALID-NAME!")
            .value("test_value")
            .build();

        let mut context = ExecutionContext::default();
        let result = handler.apply_context(&var, &mut context).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("can only contain letters, numbers, and underscores"));
    }

    #[tokio::test]
    async fn test_valid_name_with_underscores() {
        let handler = VarHandler;
        let var = Var::builder()
            .id(Uuid::new_v4())
            .name("VALID_VAR_NAME_123")
            .value("test_value")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&var, &mut context).await.unwrap();

        assert_eq!(
            context.variables.get("VALID_VAR_NAME_123"),
            Some(&"test_value".to_string())
        );
    }

    #[tokio::test]
    async fn test_value_with_special_chars() {
        let handler = VarHandler;
        let var = Var::builder()
            .id(Uuid::new_v4())
            .name("SPECIAL_VAR")
            .value("value with spaces and symbols: !@#$%^&*()")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&var, &mut context).await.unwrap();

        assert_eq!(
            context.variables.get("SPECIAL_VAR"),
            Some(&"value with spaces and symbols: !@#$%^&*()".to_string())
        );
    }

    #[tokio::test]
    async fn test_multiline_value() {
        let handler = VarHandler;
        let var = Var::builder()
            .id(Uuid::new_v4())
            .name("MULTILINE_VAR")
            .value("line1\nline2\nline3")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&var, &mut context).await.unwrap();

        assert_eq!(
            context.variables.get("MULTILINE_VAR"),
            Some(&"line1\nline2\nline3".to_string())
        );
    }

    #[tokio::test]
    async fn test_unicode_value() {
        let handler = VarHandler;
        let var = Var::builder()
            .id(Uuid::new_v4())
            .name("UNICODE_VAR")
            .value("测试值 🚀 émojis")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&var, &mut context).await.unwrap();

        assert_eq!(
            context.variables.get("UNICODE_VAR"),
            Some(&"测试值 🚀 émojis".to_string())
        );
    }

    // Serialization tests
    #[tokio::test]
    async fn test_json_serialization_roundtrip() {
        let original = Var::builder()
            .id(Uuid::new_v4())
            .name("TEST_VAR")
            .value("test_value")
            .build();

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: Var = serde_json::from_str(&json).unwrap();

        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.name, deserialized.name);
        assert_eq!(original.value, deserialized.value);
    }

    #[tokio::test]
    async fn test_from_document_valid() {
        let id = Uuid::new_v4();
        let json_data = serde_json::json!({
            "id": id.to_string(),
            "name": "TEST_VAR",
            "value": "test_value",
            "type": "var"
        });

        let var = Var::from_document(&json_data).unwrap();
        assert_eq!(var.id, id);
        assert_eq!(var.name, "TEST_VAR");
        assert_eq!(var.value, "test_value");
    }

    #[tokio::test]
    async fn test_from_document_missing_value_defaults_empty() {
        let id = Uuid::new_v4();
        let json_data = serde_json::json!({
            "id": id.to_string(),
            "name": "TEST_VAR",
            "type": "var"
        });

        let var = Var::from_document(&json_data).unwrap();
        assert_eq!(var.id, id);
        assert_eq!(var.name, "TEST_VAR");
        assert_eq!(var.value, "");
    }

    #[tokio::test]
    async fn test_from_document_missing_name() {
        let json_data = serde_json::json!({
            "id": Uuid::new_v4().to_string(),
            "value": "test_value",
            "type": "var"
        });

        let result = Var::from_document(&json_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing name"));
    }

    // Integration tests
    #[tokio::test]
    async fn test_multiple_variables() {
        let handler = VarHandler;

        let var1 = Var::builder()
            .id(Uuid::new_v4())
            .name("VAR1")
            .value("value1")
            .build();

        let var2 = Var::builder()
            .id(Uuid::new_v4())
            .name("VAR2")
            .value("value2")
            .build();

        let mut context = ExecutionContext::default();

        // Apply both variables
        handler.apply_context(&var1, &mut context).await.unwrap();
        handler.apply_context(&var2, &mut context).await.unwrap();

        assert_eq!(context.variables.get("VAR1"), Some(&"value1".to_string()));
        assert_eq!(context.variables.get("VAR2"), Some(&"value2".to_string()));
    }

    #[tokio::test]
    async fn test_variable_override() {
        let handler = VarHandler;

        let var1 = Var::builder()
            .id(Uuid::new_v4())
            .name("SAME_VAR")
            .value("first_value")
            .build();

        let var2 = Var::builder()
            .id(Uuid::new_v4())
            .name("SAME_VAR")
            .value("second_value")
            .build();

        let mut context = ExecutionContext::default();

        // Apply first, then second (should override)
        handler.apply_context(&var1, &mut context).await.unwrap();
        assert_eq!(
            context.variables.get("SAME_VAR"),
            Some(&"first_value".to_string())
        );

        handler.apply_context(&var2, &mut context).await.unwrap();
        assert_eq!(
            context.variables.get("SAME_VAR"),
            Some(&"second_value".to_string())
        );
    }

    #[tokio::test]
    async fn test_var_context_preserves_other_fields() {
        let handler = VarHandler;
        let var = Var::builder()
            .id(Uuid::new_v4())
            .name("TEST_VAR")
            .value("test_value")
            .build();

        let mut context = ExecutionContext {
            runbook_id: Uuid::new_v4(),
            cwd: "/test/path".to_string(),
            env: {
                let mut env = HashMap::new();
                env.insert("ENV_VAR".to_string(), "env_value".to_string());
                env
            },
            variables: {
                let mut vars = HashMap::new();
                vars.insert("var1".to_string(), "value1".to_string());
                vars
            },
            ssh_host: Some("user@host.com".to_string()),
            document: vec![serde_json::json!({"test": "data"})],
            ssh_pool: None,
            output_storage: None,
            pty_store: None,
            event_bus: None,
        };

        let original_runbook_id = context.runbook_id;
        let original_cwd = context.cwd.clone();
        let original_env = context.env.clone();
        let original_ssh_host = context.ssh_host.clone();
        let original_document = context.document.clone();

        handler.apply_context(&var, &mut context).await.unwrap();

        // Variables should be updated
        assert_eq!(
            context.variables.get("TEST_VAR"),
            Some(&"test_value".to_string())
        );
        assert_eq!(context.variables.get("var1"), Some(&"value1".to_string())); // Original preserved

        // Other fields should be preserved
        assert_eq!(context.runbook_id, original_runbook_id);
        assert_eq!(context.cwd, original_cwd);
        assert_eq!(context.env, original_env);
        assert_eq!(context.ssh_host, original_ssh_host);
        assert_eq!(context.document, original_document);
    }
}
