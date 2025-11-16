use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::blocks::{Block, BlockBehavior};
use crate::client::LocalValueProvider;
use crate::context::{BlockContext, ContextResolver, DocumentSshHost};
use async_trait::async_trait;

/// Host context block for switching between localhost and SSH connections
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Host {
    /// Unique identifier for this block
    #[builder(setter(into))]
    pub id: Uuid,

    /// Host to switch to
    /// - "localhost", "local", "", or None -> local execution  
    /// - Any other value -> SSH execution to that host
    #[builder(setter(into))]
    pub host: String,
}

impl Host {
    #[allow(dead_code)]
    /// Create a Host block from a document block
    pub fn from_document(block: &serde_json::Value) -> Result<Self, String> {
        let id = block
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid id field")?;

        let props = block
            .get("props")
            .and_then(|v| v.as_object())
            .ok_or("Missing or invalid props field")?;

        let host = props
            .get("host")
            .and_then(|v| v.as_str())
            .unwrap_or("localhost") // Default to localhost if not specified
            .to_string();

        Ok(Host::builder()
            .id(Uuid::parse_str(id).map_err(|e| e.to_string())?)
            .host(host)
            .build())
    }
}

#[async_trait]
impl BlockBehavior for Host {
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::Host(self)
    }

    async fn passive_context(
        &self,
        resolver: &ContextResolver,
        _block_local_value_provider: Option<&dyn LocalValueProvider>,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        let mut context = BlockContext::new();
        let host = self.host.trim().to_lowercase();

        if host.is_empty() || host == "local" || host == "localhost" {
            context.insert(DocumentSshHost(None));
        } else {
            let resolved_host = resolver.resolve_template(&host)?;
            context.insert(DocumentSshHost(Some(resolved_host)));
        }

        Ok(Some(context))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ResolvedContext;
    use serde_json::json;

    #[tokio::test]
    async fn test_switch_to_localhost() {
        let host = Host::builder().id(Uuid::new_v4()).host("localhost").build();

        let context = ResolvedContext::from_block(&host, None).await.unwrap();

        assert!(context.ssh_host.is_none());
    }

    #[tokio::test]
    async fn test_switch_to_empty_host() {
        let host = Host::builder().id(Uuid::new_v4()).host("").build();

        let context = ResolvedContext::from_block(&host, None).await.unwrap();
        assert!(context.ssh_host.is_none());
    }

    #[tokio::test]
    async fn test_switch_to_ssh_host() {
        let host = Host::builder()
            .id(Uuid::new_v4())
            .host("user@newhost.com")
            .build();

        let context = ResolvedContext::from_block(&host, None).await.unwrap();
        assert!(context.ssh_host.is_some());
    }

    #[tokio::test]
    async fn test_host_with_whitespace() {
        let host = Host::builder()
            .id(Uuid::new_v4())
            .host("  user@host.com  ")
            .build();

        let context = ResolvedContext::from_block(&host, None).await.unwrap();
        assert_eq!(context.ssh_host, Some("user@host.com".to_string()));
    }

    #[tokio::test]
    async fn test_localhost_variations() {
        let variations = vec![
            ("localhost", None),
            ("LOCALHOST", None),
            ("foo.com", Some("foo.com")),
            ("1.2.3.4", Some("1.2.3.4")),
        ];

        for (host_str, expected_ssh_host) in variations {
            let host_block = Host::builder().id(Uuid::new_v4()).host(host_str).build();

            let context = ResolvedContext::from_block(&host_block, None)
                .await
                .unwrap();
            assert_eq!(context.ssh_host, expected_ssh_host.map(|s| s.to_string()));
        }
    }

    #[tokio::test]
    async fn test_from_document_valid() {
        let block = json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "type": "host",
            "props": {
                "host": "user@example.com"
            }
        });

        let host = Host::from_document(&block).unwrap();
        assert_eq!(host.host, "user@example.com");
        assert_eq!(host.id.to_string(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[tokio::test]
    async fn test_from_document_missing_host_defaults_localhost() {
        let block = json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "type": "host",
            "props": {}
        });

        let host = Host::from_document(&block).unwrap();
        assert_eq!(host.host, "localhost");
    }

    #[tokio::test]
    async fn test_from_document_missing_id() {
        let block = json!({
            "type": "host",
            "props": {
                "host": "test.com"
            }
        });

        let result = Host::from_document(&block);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Missing or invalid id field"));
    }

    #[tokio::test]
    async fn test_from_document_missing_props() {
        let block = json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "type": "host"
        });

        let result = Host::from_document(&block);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Missing or invalid props field"));
    }

    #[tokio::test]
    async fn test_json_serialization_roundtrip() {
        let original = Host::builder()
            .id(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap())
            .host("test@example.com")
            .build();

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: Host = serde_json::from_str(&json).unwrap();

        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.host, deserialized.host);
    }

    #[tokio::test]
    async fn test_common_host_patterns() {
        let test_cases = vec![
            ("localhost", None),
            ("", None),
            ("user@host.com", Some("user@host.com")),
            ("host.com", Some("host.com")),
            ("user@192.168.1.100", Some("user@192.168.1.100")),
            ("192.168.1.100", Some("192.168.1.100")),
            ("user@host.com:22", Some("user@host.com:22")),
        ];

        for (host_str, expected_ssh_host) in test_cases {
            let host_block = Host::builder().id(Uuid::new_v4()).host(host_str).build();

            let context = ResolvedContext::from_block(&host_block, None)
                .await
                .unwrap();
            assert_eq!(context.ssh_host, expected_ssh_host.map(|s| s.to_string()));
        }
    }
}
