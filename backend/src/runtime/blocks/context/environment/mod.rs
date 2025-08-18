use crate::runtime::blocks::handler::{ContextProvider, ExecutionContext};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub name: String,

    #[builder(setter(into))]
    pub value: String,
}

pub struct EnvironmentHandler;

#[async_trait]
impl ContextProvider for EnvironmentHandler {
    type Block = Environment;

    fn block_type(&self) -> &'static str {
        "env"
    }

    async fn apply_context(
        &self,
        block: &Environment,
        context: &mut ExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Validate environment variable name
        if block.name.is_empty() {
            return Err("Environment variable name cannot be empty".into());
        }

        // Check for invalid characters in env var name (basic validation)
        if block.name.contains('=') || block.name.contains('\0') {
            return Err("Environment variable name contains invalid characters".into());
        }

        context.env.insert(block.name.clone(), block.value.clone());
        Ok(())
    }
}

impl Environment {
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

        Ok(Environment::builder()
            .id(id)
            .name(name)
            .value(value)
            .build())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // Basic functionality tests
    #[tokio::test]
    async fn test_basic_environment_context() {
        let handler = EnvironmentHandler;
        let env = Environment::builder()
            .id(Uuid::new_v4())
            .name("TEST_VAR")
            .value("test_value")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&env, &mut context).await.unwrap();

        assert_eq!(context.env.get("TEST_VAR"), Some(&"test_value".to_string()));
    }

    #[tokio::test]
    async fn test_block_type() {
        let handler = EnvironmentHandler;
        assert_eq!(handler.block_type(), "env");
    }

    // Edge cases
    #[tokio::test]
    async fn test_empty_value() {
        let handler = EnvironmentHandler;
        let env = Environment::builder()
            .id(Uuid::new_v4())
            .name("EMPTY_VAR")
            .value("")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&env, &mut context).await.unwrap();

        assert_eq!(context.env.get("EMPTY_VAR"), Some(&"".to_string()));
    }

    #[tokio::test]
    async fn test_empty_name_fails() {
        let handler = EnvironmentHandler;
        let env = Environment::builder()
            .id(Uuid::new_v4())
            .name("")
            .value("test_value")
            .build();

        let mut context = ExecutionContext::default();
        let result = handler.apply_context(&env, &mut context).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Environment variable name cannot be empty"));
    }

    #[tokio::test]
    async fn test_invalid_name_with_equals() {
        let handler = EnvironmentHandler;
        let env = Environment::builder()
            .id(Uuid::new_v4())
            .name("INVALID=NAME")
            .value("test_value")
            .build();

        let mut context = ExecutionContext::default();
        let result = handler.apply_context(&env, &mut context).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid characters"));
    }

    #[tokio::test]
    async fn test_invalid_name_with_null() {
        let handler = EnvironmentHandler;
        let env = Environment::builder()
            .id(Uuid::new_v4())
            .name("INVALID\0NAME")
            .value("test_value")
            .build();

        let mut context = ExecutionContext::default();
        let result = handler.apply_context(&env, &mut context).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid characters"));
    }

    #[tokio::test]
    async fn test_value_with_special_chars() {
        let handler = EnvironmentHandler;
        let env = Environment::builder()
            .id(Uuid::new_v4())
            .name("SPECIAL_VAR")
            .value("value with spaces and symbols: !@#$%^&*()")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&env, &mut context).await.unwrap();

        assert_eq!(
            context.env.get("SPECIAL_VAR"),
            Some(&"value with spaces and symbols: !@#$%^&*()".to_string())
        );
    }

    #[tokio::test]
    async fn test_multiline_value() {
        let handler = EnvironmentHandler;
        let env = Environment::builder()
            .id(Uuid::new_v4())
            .name("MULTILINE_VAR")
            .value("line1\nline2\nline3")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&env, &mut context).await.unwrap();

        assert_eq!(
            context.env.get("MULTILINE_VAR"),
            Some(&"line1\nline2\nline3".to_string())
        );
    }

    #[tokio::test]
    async fn test_unicode_value() {
        let handler = EnvironmentHandler;
        let env = Environment::builder()
            .id(Uuid::new_v4())
            .name("UNICODE_VAR")
            .value("测试值 🚀 émojis")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&env, &mut context).await.unwrap();

        assert_eq!(
            context.env.get("UNICODE_VAR"),
            Some(&"测试值 🚀 émojis".to_string())
        );
    }

    // Serialization tests
    #[tokio::test]
    async fn test_json_serialization_roundtrip() {
        let original = Environment::builder()
            .id(Uuid::new_v4())
            .name("TEST_VAR")
            .value("test_value")
            .build();

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: Environment = serde_json::from_str(&json).unwrap();

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
            "type": "env"
        });

        let env = Environment::from_document(&json_data).unwrap();
        assert_eq!(env.id, id);
        assert_eq!(env.name, "TEST_VAR");
        assert_eq!(env.value, "test_value");
    }

    #[tokio::test]
    async fn test_from_document_missing_value_defaults_empty() {
        let id = Uuid::new_v4();
        let json_data = serde_json::json!({
            "id": id.to_string(),
            "name": "TEST_VAR",
            "type": "env"
        });

        let env = Environment::from_document(&json_data).unwrap();
        assert_eq!(env.id, id);
        assert_eq!(env.name, "TEST_VAR");
        assert_eq!(env.value, "");
    }

    #[tokio::test]
    async fn test_from_document_missing_name() {
        let json_data = serde_json::json!({
            "id": Uuid::new_v4().to_string(),
            "value": "test_value",
            "type": "env"
        });

        let result = Environment::from_document(&json_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing name"));
    }

    // Integration tests
    #[tokio::test]
    async fn test_multiple_environment_variables() {
        let handler = EnvironmentHandler;

        let env1 = Environment::builder()
            .id(Uuid::new_v4())
            .name("VAR1")
            .value("value1")
            .build();

        let env2 = Environment::builder()
            .id(Uuid::new_v4())
            .name("VAR2")
            .value("value2")
            .build();

        let mut context = ExecutionContext::default();

        // Apply both environment variables
        handler.apply_context(&env1, &mut context).await.unwrap();
        handler.apply_context(&env2, &mut context).await.unwrap();

        assert_eq!(context.env.get("VAR1"), Some(&"value1".to_string()));
        assert_eq!(context.env.get("VAR2"), Some(&"value2".to_string()));
    }

    #[tokio::test]
    async fn test_environment_variable_override() {
        let handler = EnvironmentHandler;

        let env1 = Environment::builder()
            .id(Uuid::new_v4())
            .name("SAME_VAR")
            .value("first_value")
            .build();

        let env2 = Environment::builder()
            .id(Uuid::new_v4())
            .name("SAME_VAR")
            .value("second_value")
            .build();

        let mut context = ExecutionContext::default();

        // Apply first, then second (should override)
        handler.apply_context(&env1, &mut context).await.unwrap();
        assert_eq!(
            context.env.get("SAME_VAR"),
            Some(&"first_value".to_string())
        );

        handler.apply_context(&env2, &mut context).await.unwrap();
        assert_eq!(
            context.env.get("SAME_VAR"),
            Some(&"second_value".to_string())
        );
    }

    #[tokio::test]
    async fn test_environment_context_preserves_other_fields() {
        let handler = EnvironmentHandler;
        let env = Environment::builder()
            .id(Uuid::new_v4())
            .name("TEST_VAR")
            .value("test_value")
            .build();

        let mut context = ExecutionContext {
            runbook_id: Uuid::new_v4(),
            cwd: "/test/path".to_string(),
            env: HashMap::new(),
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
        let original_variables = context.variables.clone();
        let original_ssh_host = context.ssh_host.clone();
        let original_document = context.document.clone();

        handler.apply_context(&env, &mut context).await.unwrap();

        // Environment should be updated
        assert_eq!(context.env.get("TEST_VAR"), Some(&"test_value".to_string()));

        // Other fields should be preserved
        assert_eq!(context.runbook_id, original_runbook_id);
        assert_eq!(context.cwd, original_cwd);
        assert_eq!(context.variables, original_variables);
        assert_eq!(context.ssh_host, original_ssh_host);
        assert_eq!(context.document, original_document);
    }

    #[tokio::test]
    async fn test_common_environment_variable_patterns() {
        let handler = EnvironmentHandler;
        let test_cases = vec![
            ("PATH", "/usr/bin:/bin"),
            ("HOME", "/home/user"),
            ("USER", "testuser"),
            ("SHELL", "/bin/bash"),
            ("LANG", "en_US.UTF-8"),
            ("DEBUG", "1"),
            ("NODE_ENV", "production"),
            ("API_KEY", "secret-key-123"),
        ];

        let mut context = ExecutionContext::default();

        for (name, value) in test_cases {
            let env = Environment::builder()
                .id(Uuid::new_v4())
                .name(name)
                .value(value)
                .build();

            handler.apply_context(&env, &mut context).await.unwrap();
            assert_eq!(context.env.get(name), Some(&value.to_string()));
        }
    }
}
