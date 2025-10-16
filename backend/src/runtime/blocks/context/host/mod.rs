//! Host context block for switching between localhost and SSH connections
//!
//! The host block allows switching the execution context between:
//! - localhost (local execution)
//! - SSH connections (remote execution)
//!
//! ## Usage
//!
//! ```json
//! {
//!   "type": "host",
//!   "props": {
//!     "host": "localhost"  // or "user@remote.host" or ""
//!   }
//! }
//! ```
//!
//! ## Behavior
//!
//! - `"localhost"`, `"local"`, or `""` -> Sets context to local execution
//! - Any other value -> Sets context to SSH execution with that host
//!
//! ## Examples
//!
//! ```json
//! // Switch to localhost
//! {"type": "host", "props": {"host": "localhost"}}
//!
//! // Switch to SSH
//! {"type": "host", "props": {"host": "user@server.com"}}
//!
//! // Switch back to localhost (empty string)
//! {"type": "host", "props": {"host": ""}}
//! ```

use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::runtime::blocks::handler::{ContextProvider, ExecutionContext};
use async_trait::async_trait;

/// Host context block for switching between localhost and SSH connections
#[derive(Debug, Serialize, Deserialize, Clone, TypedBuilder)]
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

/// Handler for host context blocks
pub struct HostHandler;

#[async_trait]
impl ContextProvider for HostHandler {
    type Block = Host;

    fn block_type(&self) -> &'static str {
        "host"
    }

    async fn apply_context(
        &self,
        block: &Self::Block,
        context: &mut ExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Determine if this should be local or SSH execution
        let host = block.host.trim().to_lowercase();

        if host.is_empty() || host == "local" || host == "localhost" {
            // Switch to local execution
            context.ssh_host = None;
        } else {
            // Switch to SSH execution
            context.ssh_host = Some(host.to_string());
        }

        Ok(())
    }
}

impl Host {
    #[allow(dead_code)]
    /// Create a Host block from a document block
    pub fn from_document(
        block: &serde_json::Value,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
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

        Ok(Host::builder().id(Uuid::parse_str(id)?).host(host).build())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::blocks::handler::ExecutionContext;
    use serde_json::json;
    use std::collections::HashMap;

    fn create_test_context() -> ExecutionContext {
        ExecutionContext {
            runbook_id: Uuid::new_v4(),
            cwd: "/test/path".to_string(),
            env: HashMap::new(),
            variables: HashMap::new(),
            ssh_host: Some("existing@host.com".to_string()), // Start with SSH context
            document: Vec::new(),
            ssh_pool: None,
            output_storage: None,
            pty_store: None,
            event_bus: None,
        }
    }

    #[tokio::test]
    async fn test_block_type() {
        let handler = HostHandler;
        assert_eq!(handler.block_type(), "host");
    }

    #[tokio::test]
    async fn test_builder_pattern() {
        let host = Host::builder()
            .id(Uuid::new_v4())
            .host("test.example.com")
            .build();

        assert_eq!(host.host, "test.example.com");
    }

    #[tokio::test]
    async fn test_builder_with_string_conversions() {
        let host = Host::builder()
            .id(Uuid::new_v4())
            .host("user@host.com") // Test string conversion
            .build();

        assert_eq!(host.host, "user@host.com");
    }

    #[tokio::test]
    async fn test_switch_to_localhost() {
        let host = Host::builder().id(Uuid::new_v4()).host("localhost").build();

        let mut context = create_test_context();
        assert!(context.ssh_host.is_some()); // Start with SSH

        let handler = HostHandler;
        let result = handler.apply_context(&host, &mut context).await;

        assert!(result.is_ok());
        assert!(context.ssh_host.is_none()); // Should switch to local
    }

    #[tokio::test]
    async fn test_switch_to_empty_host() {
        let host = Host::builder().id(Uuid::new_v4()).host("").build();

        let mut context = create_test_context();
        assert!(context.ssh_host.is_some()); // Start with SSH

        let handler = HostHandler;
        let result = handler.apply_context(&host, &mut context).await;

        assert!(result.is_ok());
        assert!(context.ssh_host.is_none()); // Should switch to local
    }

    #[tokio::test]
    async fn test_switch_to_ssh_host() {
        let host = Host::builder()
            .id(Uuid::new_v4())
            .host("user@newhost.com")
            .build();

        let mut context = create_test_context();
        context.ssh_host = None; // Start with local

        let handler = HostHandler;
        let result = handler.apply_context(&host, &mut context).await;

        assert!(result.is_ok());
        assert_eq!(context.ssh_host, Some("user@newhost.com".to_string()));
    }

    #[tokio::test]
    async fn test_switch_between_ssh_hosts() {
        let host = Host::builder()
            .id(Uuid::new_v4())
            .host("user@different.com")
            .build();

        let mut context = create_test_context();
        assert_eq!(context.ssh_host, Some("existing@host.com".to_string()));

        let handler = HostHandler;
        let result = handler.apply_context(&host, &mut context).await;

        assert!(result.is_ok());
        assert_eq!(context.ssh_host, Some("user@different.com".to_string()));
    }

    #[tokio::test]
    async fn test_host_with_whitespace() {
        let host = Host::builder()
            .id(Uuid::new_v4())
            .host("  user@host.com  ")
            .build();

        let mut context = create_test_context();
        context.ssh_host = None;

        let handler = HostHandler;
        let result = handler.apply_context(&host, &mut context).await;

        assert!(result.is_ok());
        assert_eq!(context.ssh_host, Some("user@host.com".to_string())); // Trimmed
    }

    #[tokio::test]
    async fn test_localhost_variations() {
        let variations = vec!["localhost", "LOCALHOST", "foo.com", "1.2.3.4"];

        for host_str in variations {
            let host_block = Host::builder().id(Uuid::new_v4()).host(host_str).build();

            let mut context = create_test_context();
            assert!(context.ssh_host.is_some()); // Start with SSH

            let handler = HostHandler;
            let result = handler.apply_context(&host_block, &mut context).await;

            assert!(result.is_ok());
            if host_str.to_lowercase() == "localhost" {
                assert!(context.ssh_host.is_none(), "Failed for: {}", host_str);
            } else {
                assert!(context.ssh_host.is_some(), "Failed for: {}", host_str);
            }
        }
    }

    #[tokio::test]
    async fn test_host_context_preserves_other_fields() {
        let host = Host::builder().id(Uuid::new_v4()).host("localhost").build();

        let mut context = ExecutionContext {
            runbook_id: Uuid::new_v4(),
            cwd: "/test/path".to_string(),
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
        let original_cwd = context.cwd.clone();
        let original_env = context.env.clone();
        let original_variables = context.variables.clone();
        let original_document = context.document.clone();

        let handler = HostHandler;
        let result = handler.apply_context(&host, &mut context).await;

        assert!(result.is_ok());

        // SSH host should change
        assert!(context.ssh_host.is_none());

        // Other fields should be preserved
        assert_eq!(context.runbook_id, original_runbook_id);
        assert_eq!(context.cwd, original_cwd);
        assert_eq!(context.env, original_env);
        assert_eq!(context.variables, original_variables);
        assert_eq!(context.document, original_document);
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

            let mut context = create_test_context();
            let handler = HostHandler;
            let result = handler.apply_context(&host_block, &mut context).await;

            assert!(result.is_ok(), "Failed for host: {}", host_str);
            assert_eq!(
                context.ssh_host.as_deref(),
                expected_ssh_host,
                "Failed for host: {}",
                host_str
            );
        }
    }

    #[tokio::test]
    async fn test_multiple_host_switches() {
        let mut context = create_test_context();
        context.ssh_host = None; // Start local
        let handler = HostHandler;

        // Switch to SSH
        let host1 = Host::builder()
            .id(Uuid::new_v4())
            .host("user@host1.com")
            .build();
        handler.apply_context(&host1, &mut context).await.unwrap();
        assert_eq!(context.ssh_host, Some("user@host1.com".to_string()));

        // Switch to different SSH
        let host2 = Host::builder()
            .id(Uuid::new_v4())
            .host("user@host2.com")
            .build();
        handler.apply_context(&host2, &mut context).await.unwrap();
        assert_eq!(context.ssh_host, Some("user@host2.com".to_string()));

        // Switch back to local
        let localhost = Host::builder().id(Uuid::new_v4()).host("localhost").build();
        handler
            .apply_context(&localhost, &mut context)
            .await
            .unwrap();
        assert!(context.ssh_host.is_none());
    }
}
