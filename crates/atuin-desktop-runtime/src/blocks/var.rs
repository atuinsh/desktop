use crate::blocks::{Block, BlockBehavior, FromDocument};
use crate::client::LocalValueProvider;
use crate::context::{BlockContext, ContextResolver, DocumentVar};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Var {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub name: String,

    #[builder(setter(into))]
    pub value: String,
}

impl FromDocument for Var {
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

        Ok(Var::builder().id(id).name(name).value(value).build())
    }
}

#[async_trait]
impl BlockBehavior for Var {
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::Var(self)
    }

    async fn passive_context(
        &self,
        resolver: &ContextResolver,
        _block_local_value_provider: Option<&dyn LocalValueProvider>,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        let mut context = BlockContext::new();
        let resolved_name = resolver.resolve_template(&self.name)?;

        // Validate name
        if resolved_name.is_empty() {
            return Err("Variable name cannot be empty".into());
        }

        if !resolved_name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_')
        {
            return Err("Variable names can only contain letters, numbers, and underscores".into());
        }

        // Resolve template in value if it contains template markers
        let resolved_value = resolver.resolve_template(&self.value)?;

        context.insert(DocumentVar::new(
            resolved_name,
            resolved_value,
            self.value.clone(),
        ));
        Ok(Some(context))
    }
}

#[cfg(test)]
mod tests {
    use crate::context::ResolvedContext;

    use super::*;

    // Basic functionality tests
    #[tokio::test]
    async fn test_basic_var_context() {
        let var = Var::builder()
            .id(Uuid::new_v4())
            .name("TEST_VAR")
            .value("test_value")
            .build();

        let context = ResolvedContext::from_block(&var, None).await.unwrap();
        assert_eq!(
            context.variables.get("TEST_VAR"),
            Some(&"test_value".to_string())
        );
    }

    // Edge cases
    #[tokio::test]
    async fn test_empty_value() {
        let var = Var::builder()
            .id(Uuid::new_v4())
            .name("EMPTY_VAR")
            .value("")
            .build();

        let context = ResolvedContext::from_block(&var, None).await.unwrap();
        assert_eq!(context.variables.get("EMPTY_VAR"), Some(&"".to_string()));
    }

    #[tokio::test]
    async fn test_empty_name_fails() {
        let var = Var::builder()
            .id(Uuid::new_v4())
            .name("")
            .value("test_value")
            .build();

        let context = ResolvedContext::from_block(&var, None).await;
        assert!(context.is_err());
    }

    #[tokio::test]
    async fn test_invalid_name_with_spaces() {
        let var = Var::builder()
            .id(Uuid::new_v4())
            .name("INVALID NAME")
            .value("test_value")
            .build();

        let context = ResolvedContext::from_block(&var, None).await;
        assert!(context.is_err());
    }

    #[tokio::test]
    async fn test_invalid_name_with_special_chars() {
        let var = Var::builder()
            .id(Uuid::new_v4())
            .name("INVALID-NAME!")
            .value("test_value")
            .build();

        let context = ResolvedContext::from_block(&var, None).await;
        assert!(context.is_err());
    }

    #[tokio::test]
    async fn test_valid_name_with_underscores() {
        let var = Var::builder()
            .id(Uuid::new_v4())
            .name("VALID_VAR_NAME_123")
            .value("test_value")
            .build();

        let context = ResolvedContext::from_block(&var, None).await.unwrap();
        assert_eq!(
            context.variables.get("VALID_VAR_NAME_123"),
            Some(&"test_value".to_string())
        );
    }

    #[tokio::test]
    async fn test_value_with_special_chars() {
        let var = Var::builder()
            .id(Uuid::new_v4())
            .name("SPECIAL_VAR")
            .value("value with spaces and symbols: !@#$%^&*()")
            .build();

        let context = ResolvedContext::from_block(&var, None).await.unwrap();
        assert_eq!(
            context.variables.get("SPECIAL_VAR"),
            Some(&"value with spaces and symbols: !@#$%^&*()".to_string())
        );
    }

    #[tokio::test]
    async fn test_multiline_value() {
        let var = Var::builder()
            .id(Uuid::new_v4())
            .name("MULTILINE_VAR")
            .value("line1\nline2\nline3")
            .build();

        let context = ResolvedContext::from_block(&var, None).await.unwrap();
        assert_eq!(
            context.variables.get("MULTILINE_VAR"),
            Some(&"line1\nline2\nline3".to_string())
        );
    }

    #[tokio::test]
    async fn test_unicode_value() {
        let var = Var::builder()
            .id(Uuid::new_v4())
            .name("UNICODE_VAR")
            .value("测试值 🚀 émojis")
            .build();

        let context = ResolvedContext::from_block(&var, None).await.unwrap();
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
            "props": {
                "name": "TEST_VAR",
                "value": "test_value"
            },
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
            "props": {
                "name": "TEST_VAR"
            },
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
            "props": {
                "value": "test_value"
            },
            "type": "var"
        });

        let result = Var::from_document(&json_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing name"));
    }
}
