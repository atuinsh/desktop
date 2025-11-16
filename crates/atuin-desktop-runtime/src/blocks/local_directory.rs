use crate::{
    blocks::{Block, BlockBehavior, FromDocument},
    client::LocalValueProvider,
    context::{BlockContext, ContextResolver, DocumentCwd},
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct LocalDirectory {
    #[builder(setter(into))]
    pub id: Uuid,
}

#[async_trait]
impl BlockBehavior for LocalDirectory {
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::LocalDirectory(self)
    }

    async fn passive_context(
        &self,
        resolver: &ContextResolver,
        block_local_value_provider: Option<&dyn LocalValueProvider>,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        let local_value = if let Some(block_local_value_provider) = block_local_value_provider {
            block_local_value_provider
                .get_block_local_value(self.id, "path")
                .await?
                .ok_or("Path not found for local directory")?
        } else {
            return Err("Block local value provider not found".into());
        };

        let mut context = BlockContext::new();
        let resolved_path = resolver.resolve_template(&local_value)?;
        context.insert(DocumentCwd(resolved_path));
        Ok(Some(context))
    }
}

impl FromDocument for LocalDirectory {
    fn from_document(block_data: &serde_json::Value) -> Result<Self, String> {
        let id = block_data
            .get("id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or("Invalid or missing id")?;

        Ok(LocalDirectory::builder().id(id).build())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{client::local::MemoryBlockLocalValueProvider, context::ResolvedContext};

    fn local_value_provider(path: String) -> impl LocalValueProvider {
        MemoryBlockLocalValueProvider::new(vec![("path".to_string(), path)])
    }

    // Basic functionality tests
    #[tokio::test]
    async fn test_basic_directory_context() {
        let dir = LocalDirectory::builder().id(Uuid::new_v4()).build();

        let context =
            ResolvedContext::from_block(&dir, Some(&local_value_provider("/tmp/test".to_string())))
                .await
                .unwrap();

        assert_eq!(context.cwd, "/tmp/test");
    }

    #[tokio::test]
    async fn test_relative_path() {
        let dir = LocalDirectory::builder().id(Uuid::new_v4()).build();

        let context = dir
            .passive_context(
                &ContextResolver::default(),
                Some(&local_value_provider("./relative/path".to_string())),
            )
            .await
            .unwrap()
            .unwrap();

        assert_eq!(context.get::<DocumentCwd>().unwrap().0, "./relative/path");
    }

    #[tokio::test]
    async fn test_path_with_spaces() {
        let dir = LocalDirectory::builder().id(Uuid::new_v4()).build();

        let context = dir
            .passive_context(
                &ContextResolver::default(),
                Some(&local_value_provider("/path with spaces/test".to_string())),
            )
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            context.get::<DocumentCwd>().unwrap().0,
            "/path with spaces/test"
        );
    }

    #[tokio::test]
    async fn test_path_with_special_chars() {
        let dir = LocalDirectory::builder().id(Uuid::new_v4()).build();

        let context = dir
            .passive_context(
                &ContextResolver::default(),
                Some(&local_value_provider(
                    "/path/with-special_chars.123/test".to_string(),
                )),
            )
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            context.get::<DocumentCwd>().unwrap().0,
            "/path/with-special_chars.123/test"
        );
    }

    #[tokio::test]
    async fn test_unicode_path() {
        let dir = LocalDirectory::builder().id(Uuid::new_v4()).build();

        let context = dir
            .passive_context(
                &ContextResolver::default(),
                Some(&local_value_provider(
                    "/path/with/unicode/测试/test".to_string(),
                )),
            )
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            context.get::<DocumentCwd>().unwrap().0,
            "/path/with/unicode/测试/test"
        );
    }

    // Serialization tests
    #[tokio::test]
    async fn test_json_serialization_roundtrip() {
        let original = LocalDirectory::builder().id(Uuid::new_v4()).build();

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: LocalDirectory = serde_json::from_str(&json).unwrap();

        assert_eq!(original.id, deserialized.id);
    }

    #[tokio::test]
    async fn test_from_document_valid() {
        let id = Uuid::new_v4();
        let json_data = serde_json::json!({
            "id": id.to_string(),
            "props": {},
            "type": "directory"
        });

        let dir = LocalDirectory::from_document(&json_data).unwrap();
        assert_eq!(dir.id, id);
    }

    #[tokio::test]
    async fn test_from_document_missing_id() {
        let json_data = serde_json::json!({
            "props": {},
            "type": "directory"
        });

        let result = LocalDirectory::from_document(&json_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid or missing id"));
    }

    #[tokio::test]
    async fn test_from_document_invalid_id() {
        let json_data = serde_json::json!({
            "id": "not-a-uuid",
            "props": {},
            "type": "directory"
        });

        let result = LocalDirectory::from_document(&json_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid or missing id"));
    }
}
