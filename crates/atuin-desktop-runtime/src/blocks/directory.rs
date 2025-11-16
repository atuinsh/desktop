use crate::blocks::{Block, BlockBehavior, FromDocument};
use crate::client::LocalValueProvider;
use crate::context::{BlockContext, ContextResolver, DocumentCwd};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Directory {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub path: String,
}

#[async_trait]
impl BlockBehavior for Directory {
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::Directory(self)
    }

    async fn passive_context(
        &self,
        resolver: &ContextResolver,
        _block_local_value_provider: Option<&dyn LocalValueProvider>,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        let mut context = BlockContext::new();
        let resolved_path = resolver.resolve_template(&self.path)?;
        context.insert(DocumentCwd(resolved_path));
        Ok(Some(context))
    }
}

impl FromDocument for Directory {
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

        let path = props
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or("Missing path")?
            .to_string();

        Ok(Directory::builder().id(id).path(path).build())
    }
}

#[cfg(test)]
mod tests {
    use crate::context::ResolvedContext;

    use super::*;

    // Basic functionality tests
    #[tokio::test]
    async fn test_basic_directory_context() {
        let dir = Directory::builder()
            .id(Uuid::new_v4())
            .path("/tmp/test")
            .build();

        let context = ResolvedContext::from_block(&dir, None).await.unwrap();

        assert_eq!(context.cwd, "/tmp/test");
    }

    // Edge cases
    #[tokio::test]
    async fn test_empty_path() {
        let dir = Directory::builder().id(Uuid::new_v4()).path("").build();

        let context = dir
            .passive_context(&ContextResolver::default(), None)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(context.get::<DocumentCwd>().unwrap().0, "");
    }

    #[tokio::test]
    async fn test_relative_path() {
        let dir = Directory::builder()
            .id(Uuid::new_v4())
            .path("./relative/path")
            .build();

        let context = dir
            .passive_context(&ContextResolver::default(), None)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(context.get::<DocumentCwd>().unwrap().0, "./relative/path");
    }

    #[tokio::test]
    async fn test_path_with_spaces() {
        let dir = Directory::builder()
            .id(Uuid::new_v4())
            .path("/path with spaces/test")
            .build();

        let context = dir
            .passive_context(&ContextResolver::default(), None)
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
        let dir = Directory::builder()
            .id(Uuid::new_v4())
            .path("/path/with-special_chars.123/test")
            .build();

        let context = dir
            .passive_context(&ContextResolver::default(), None)
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
        let dir = Directory::builder()
            .id(Uuid::new_v4())
            .path("/path/with/unicode/测试/test")
            .build();

        let context = dir
            .passive_context(&ContextResolver::default(), None)
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
        let original = Directory::builder()
            .id(Uuid::new_v4())
            .path("/tmp/test")
            .build();

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: Directory = serde_json::from_str(&json).unwrap();

        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.path, deserialized.path);
    }

    #[tokio::test]
    async fn test_from_document_valid() {
        let id = Uuid::new_v4();
        let json_data = serde_json::json!({
            "id": id.to_string(),
            "props": {
                "path": "/tmp/test"
            },
            "type": "directory"
        });

        let dir = Directory::from_document(&json_data).unwrap();
        assert_eq!(dir.id, id);
        assert_eq!(dir.path, "/tmp/test");
    }

    #[tokio::test]
    async fn test_from_document_missing_id() {
        let json_data = serde_json::json!({
            "props": {
                "path": "/tmp/test"
            },
            "type": "directory"
        });

        let result = Directory::from_document(&json_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid or missing id"));
    }

    #[tokio::test]
    async fn test_from_document_missing_path() {
        let json_data = serde_json::json!({
            "id": Uuid::new_v4().to_string(),
            "props": {},
            "type": "directory"
        });

        let result = Directory::from_document(&json_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing path"));
    }

    #[tokio::test]
    async fn test_from_document_invalid_id() {
        let json_data = serde_json::json!({
            "id": "not-a-uuid",
            "props": {
                "path": "/tmp/test"
            },
            "type": "directory"
        });

        let result = Directory::from_document(&json_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid or missing id"));
    }

    #[tokio::test]
    async fn test_builder_pattern() {
        let id = Uuid::new_v4();
        let dir = Directory::builder().id(id).path("/test/path").build();

        assert_eq!(dir.id, id);
        assert_eq!(dir.path, "/test/path");
    }

    #[tokio::test]
    async fn test_builder_with_string_conversions() {
        let id = Uuid::new_v4();
        let dir = Directory::builder()
            .id(id)
            .path("/test/path".to_string()) // String instead of &str
            .build();

        assert_eq!(dir.id, id);
        assert_eq!(dir.path, "/test/path");
    }
}
