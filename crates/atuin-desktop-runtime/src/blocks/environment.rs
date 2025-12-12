use crate::blocks::{Block, BlockBehavior, FromDocument};
use crate::client::LocalValueProvider;
use crate::context::{BlockContext, ContextResolver, DocumentEnvVar};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub name: String,

    #[builder(setter(into))]
    pub value: String,
}

#[async_trait]
impl BlockBehavior for Environment {
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::Environment(self)
    }

    async fn passive_context(
        &self,
        resolver: &ContextResolver,
        _block_local_value_provider: Option<&dyn LocalValueProvider>,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        let mut context = BlockContext::new();
        if self.name.is_empty() {
            return Err("Environment variable name cannot be empty".into());
        }

        if self.name.contains('=') || self.name.contains('\0') {
            return Err("Environment variable name contains invalid characters".into());
        }

        let resolved_name = resolver.resolve_template(&self.name)?;
        let resolved_value = resolver.resolve_template(&self.value)?;

        context.insert(DocumentEnvVar(resolved_name, resolved_value));
        Ok(Some(context))
    }
}

impl FromDocument for Environment {
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

        let name = props
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or("Missing name")?
            .to_string();

        let value = props
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
    use crate::context::ResolvedContext;

    use super::*;

    // Basic functionality tests
    #[tokio::test]
    async fn test_basic_environment_context() {
        let env = Environment::builder()
            .id(Uuid::new_v4())
            .name("TEST_VAR")
            .value("test_value")
            .build();

        let context = ResolvedContext::from_block(&env, None).await.unwrap();

        assert_eq!(
            context.env_vars.get("TEST_VAR"),
            Some(&"test_value".to_string())
        );
    }

    // Edge cases
    #[tokio::test]
    async fn test_empty_value() {
        let env = Environment::builder()
            .id(Uuid::new_v4())
            .name("EMPTY_VAR")
            .value("")
            .build();

        let context = ResolvedContext::from_block(&env, None).await.unwrap();
        assert_eq!(context.env_vars.get("EMPTY_VAR"), Some(&"".to_string()));
    }

    #[tokio::test]
    async fn test_empty_name_fails() {
        let env = Environment::builder()
            .id(Uuid::new_v4())
            .name("")
            .value("test_value")
            .build();

        let context = ResolvedContext::from_block(&env, None).await;
        assert!(context.is_err());
    }

    #[tokio::test]
    async fn test_invalid_name_with_equals() {
        let env = Environment::builder()
            .id(Uuid::new_v4())
            .name("INVALID=NAME")
            .value("test_value")
            .build();

        let context = ResolvedContext::from_block(&env, None).await;
        assert!(context.is_err());
    }

    #[tokio::test]
    async fn test_invalid_name_with_null() {
        let env = Environment::builder()
            .id(Uuid::new_v4())
            .name("INVALID\0NAME")
            .value("test_value")
            .build();

        let context = ResolvedContext::from_block(&env, None).await;
        assert!(context.is_err());
    }

    #[tokio::test]
    async fn test_value_with_special_chars() {
        let env = Environment::builder()
            .id(Uuid::new_v4())
            .name("SPECIAL_VAR")
            .value("value with spaces and symbols: !@#$%^&*()")
            .build();

        let context = ResolvedContext::from_block(&env, None).await.unwrap();

        assert_eq!(
            context.env_vars.get("SPECIAL_VAR"),
            Some(&"value with spaces and symbols: !@#$%^&*()".to_string())
        );
    }

    #[tokio::test]
    async fn test_multiline_value() {
        let env = Environment::builder()
            .id(Uuid::new_v4())
            .name("MULTILINE_VAR")
            .value("line1\nline2\nline3")
            .build();

        let context = ResolvedContext::from_block(&env, None).await.unwrap();
        assert_eq!(
            context.env_vars.get("MULTILINE_VAR"),
            Some(&"line1\nline2\nline3".to_string())
        );
    }

    #[tokio::test]
    async fn test_unicode_value() {
        let env = Environment::builder()
            .id(Uuid::new_v4())
            .name("UNICODE_VAR")
            .value("测试值 🚀 émojis")
            .build();

        let context = ResolvedContext::from_block(&env, None).await.unwrap();
        assert_eq!(
            context.env_vars.get("UNICODE_VAR"),
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
            "props": {
                "name": "TEST_VAR",
                "value": "test_value"
            },
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
            "props": {
                "name": "TEST_VAR"
            },
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
            "props": {
                "value": "test_value"
            },
            "type": "env"
        });

        let result = Environment::from_document(&json_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing name"));
    }

    #[tokio::test]
    async fn test_common_environment_variable_patterns() {
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

        for (name, value) in test_cases {
            let env = Environment::builder()
                .id(Uuid::new_v4())
                .name(name)
                .value(value)
                .build();

            let context = ResolvedContext::from_block(&env, None).await.unwrap();
            assert_eq!(context.env_vars.get(name), Some(&value.to_string()));
        }
    }
}
