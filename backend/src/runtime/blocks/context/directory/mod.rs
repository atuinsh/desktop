use crate::runtime::blocks::handler::{ContextProvider, ExecutionContext};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Directory {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub path: String,
}

pub struct DirectoryHandler;

#[async_trait]
impl ContextProvider for DirectoryHandler {
    type Block = Directory;

    fn block_type(&self) -> &'static str {
        "directory"
    }

    async fn apply_context(
        &self,
        block: &Directory,
        context: &mut ExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        context.cwd = block.path.clone();
        Ok(())
    }
}

impl Directory {
    #[allow(dead_code)] // Used for JSON parsing but not currently called
    pub fn from_document(block_data: &serde_json::Value) -> Result<Self, String> {
        let id = block_data
            .get("id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or("Invalid or missing id")?;

        let path = block_data
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or("Missing path")?
            .to_string();

        Ok(Directory::builder().id(id).path(path).build())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // Basic functionality tests
    #[tokio::test]
    async fn test_basic_directory_context() {
        let handler = DirectoryHandler;
        let dir = Directory::builder()
            .id(Uuid::new_v4())
            .path("/tmp/test")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&dir, &mut context).await.unwrap();

        assert_eq!(context.cwd, "/tmp/test");
    }

    #[tokio::test]
    async fn test_block_type() {
        let handler = DirectoryHandler;
        assert_eq!(handler.block_type(), "directory");
    }

    // Edge cases
    #[tokio::test]
    async fn test_empty_path() {
        let handler = DirectoryHandler;
        let dir = Directory::builder().id(Uuid::new_v4()).path("").build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&dir, &mut context).await.unwrap();

        assert_eq!(context.cwd, "");
    }

    #[tokio::test]
    async fn test_relative_path() {
        let handler = DirectoryHandler;
        let dir = Directory::builder()
            .id(Uuid::new_v4())
            .path("./relative/path")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&dir, &mut context).await.unwrap();

        assert_eq!(context.cwd, "./relative/path");
    }

    #[tokio::test]
    async fn test_path_with_spaces() {
        let handler = DirectoryHandler;
        let dir = Directory::builder()
            .id(Uuid::new_v4())
            .path("/path with spaces/test")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&dir, &mut context).await.unwrap();

        assert_eq!(context.cwd, "/path with spaces/test");
    }

    #[tokio::test]
    async fn test_path_with_special_chars() {
        let handler = DirectoryHandler;
        let dir = Directory::builder()
            .id(Uuid::new_v4())
            .path("/path/with-special_chars.123/test")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&dir, &mut context).await.unwrap();

        assert_eq!(context.cwd, "/path/with-special_chars.123/test");
    }

    #[tokio::test]
    async fn test_unicode_path() {
        let handler = DirectoryHandler;
        let dir = Directory::builder()
            .id(Uuid::new_v4())
            .path("/path/with/unicode/测试/test")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&dir, &mut context).await.unwrap();

        assert_eq!(context.cwd, "/path/with/unicode/测试/test");
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
            "path": "/tmp/test",
            "type": "directory"
        });

        let dir = Directory::from_document(&json_data).unwrap();
        assert_eq!(dir.id, id);
        assert_eq!(dir.path, "/tmp/test");
    }

    #[tokio::test]
    async fn test_from_document_missing_id() {
        let json_data = serde_json::json!({
            "path": "/tmp/test",
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
            "path": "/tmp/test",
            "type": "directory"
        });

        let result = Directory::from_document(&json_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid or missing id"));
    }

    // Integration tests
    #[tokio::test]
    async fn test_multiple_directory_contexts() {
        let handler = DirectoryHandler;

        let dir1 = Directory::builder()
            .id(Uuid::new_v4())
            .path("/first/path")
            .build();

        let dir2 = Directory::builder()
            .id(Uuid::new_v4())
            .path("/second/path")
            .build();

        let mut context = ExecutionContext::default();

        // Apply first directory
        handler.apply_context(&dir1, &mut context).await.unwrap();
        assert_eq!(context.cwd, "/first/path");

        // Apply second directory (should override)
        handler.apply_context(&dir2, &mut context).await.unwrap();
        assert_eq!(context.cwd, "/second/path");
    }

    #[tokio::test]
    async fn test_directory_context_preserves_other_fields() {
        let handler = DirectoryHandler;
        let dir = Directory::builder()
            .id(Uuid::new_v4())
            .path("/tmp/test")
            .build();

        let mut context = ExecutionContext {
            runbook_id: Uuid::new_v4(),
            cwd: "/original/path".to_string(),
            env: {
                let mut env = HashMap::new();
                env.insert("TEST_VAR".to_string(), "test_value".to_string());
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
        let original_env = context.env.clone();
        let original_variables = context.variables.clone();
        let original_ssh_host = context.ssh_host.clone();
        let original_document = context.document.clone();

        handler.apply_context(&dir, &mut context).await.unwrap();

        // Directory should be updated
        assert_eq!(context.cwd, "/tmp/test");

        // Other fields should be preserved
        assert_eq!(context.runbook_id, original_runbook_id);
        assert_eq!(context.env, original_env);
        assert_eq!(context.variables, original_variables);
        assert_eq!(context.ssh_host, original_ssh_host);
        assert_eq!(context.document, original_document);
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
