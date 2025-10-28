use crate::runtime::blocks::{
    document::{
        actor::BlockLocalValueProvider,
        block_context::{BlockContext, ContextResolver, DocumentVar},
    },
    Block, BlockBehavior, FromDocument,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct LocalVar {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub name: String,

    #[builder(setter(into))]
    pub value: String,
}

impl FromDocument for LocalVar {
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

        Ok(LocalVar::builder().id(id).name(name).value(value).build())
    }
}

#[async_trait]
impl BlockBehavior for LocalVar {
    fn into_block(self) -> Block {
        Block::LocalVar(self)
    }

    // TODO: get this from KV storage
    async fn passive_context(
        &self,
        resolver: &ContextResolver,
        block_local_value_provider: Option<&Box<dyn BlockLocalValueProvider>>,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        // Validate name
        if self.name.is_empty() {
            return Err("Local variable name cannot be empty".into());
        }

        if !self.name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err("Variable names can only contain letters, numbers, and underscores".into());
        }

        // Resolve value
        let mut context = BlockContext::new();
        let resolved_value = resolver.resolve_template(&self.value)?;
        context.insert(DocumentVar(self.name.clone(), resolved_value));
        Ok(Some(context))
    }
}

#[cfg(test)]
mod tests {
    use crate::runtime::blocks::document::block_context::ResolvedContext;

    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_local_var_handler_empty_name() {
        let local_var = LocalVar::builder()
            .id(Uuid::new_v4())
            .name("")
            .value("")
            .build();

        let context = ResolvedContext::from_block(&local_var, None).await;
        assert!(context.is_err());
    }

    #[tokio::test]
    async fn test_local_var_handler_with_name() {
        let local_var = LocalVar::builder()
            .id(Uuid::new_v4())
            .name("test_var")
            .value("test_value")
            .build();

        let context = ResolvedContext::from_block(&local_var, None).await.unwrap();

        assert_eq!(
            context.variables.get("test_var"),
            Some(&"test_value".to_string())
        );
    }

    #[tokio::test]
    async fn test_local_var_serialization() {
        let local_var = LocalVar::builder()
            .id(Uuid::new_v4())
            .name("test_var")
            .value("test_value")
            .build();

        // Test serialization roundtrip
        let json = serde_json::to_string(&local_var).unwrap();
        let deserialized: LocalVar = serde_json::from_str(&json).unwrap();

        assert_eq!(local_var.name, deserialized.name);
        assert_eq!(local_var.id, deserialized.id);
    }

    #[tokio::test]
    async fn test_local_var_valid_name_pattern() {
        // Test valid variable names (same pattern as frontend validation)
        let valid_names = vec!["test_var", "TEST123", "var_name_123", "a", "A"];

        for name in valid_names {
            let local_var = LocalVar::builder()
                .id(Uuid::new_v4())
                .name(name)
                .value("test_value")
                .build();

            let context = ResolvedContext::from_block(&local_var, None).await.unwrap();
            assert_eq!(context.variables.get(name), Some(&"test_value".to_string()));
        }
    }
}
