use crate::runtime::blocks::handler::{ContextProvider, ExecutionContext};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct SshConnect {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub user_host: String,
}

pub struct SshConnectHandler;

#[async_trait]
impl ContextProvider for SshConnectHandler {
    type Block = SshConnect;

    fn block_type(&self) -> &'static str {
        "ssh-connect"
    }

    async fn apply_context(
        &self,
        block: &SshConnect,
        context: &mut ExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Basic validation of user_host format
        if block.user_host.is_empty() {
            return Err("SSH user_host cannot be empty".into());
        }

        // Basic format validation (should contain @ or be just hostname)
        if !block.user_host.contains('@') && block.user_host.contains(' ') {
            return Err("Invalid SSH user_host format".into());
        }

        context.ssh_host = Some(block.user_host.clone());
        Ok(())
    }
}

impl SshConnect {
    #[allow(dead_code)] // Used for JSON parsing but not currently called
    pub fn from_document(block_data: &serde_json::Value) -> Result<Self, String> {
        let id = block_data
            .get("id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or("Invalid or missing id")?;

        let user_host = block_data
            .get("userHost")
            .or_else(|| block_data.get("user_host")) // Support both camelCase and snake_case
            .and_then(|v| v.as_str())
            .ok_or("Missing userHost")?
            .to_string();

        Ok(SshConnect::builder().id(id).user_host(user_host).build())
    }

    /// Parse the user_host string into components
    #[allow(dead_code)] // Utility method for future use
    pub fn parse_user_host(&self) -> (Option<String>, String, Option<u16>) {
        let parts: Vec<&str> = self.user_host.split('@').collect();

        if parts.len() == 2 {
            let user = Some(parts[0].to_string());
            let host_port = parts[1];

            // Check for port
            if let Some(colon_pos) = host_port.rfind(':') {
                let host = host_port[..colon_pos].to_string();
                let port_str = &host_port[colon_pos + 1..];
                let port = port_str.parse::<u16>().ok();
                (user, host, port)
            } else {
                (user, host_port.to_string(), None)
            }
        } else {
            // No user specified, just host (and possibly port)
            if let Some(colon_pos) = self.user_host.rfind(':') {
                let host = self.user_host[..colon_pos].to_string();
                let port_str = &self.user_host[colon_pos + 1..];
                let port = port_str.parse::<u16>().ok();
                (None, host, port)
            } else {
                (None, self.user_host.clone(), None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // Basic functionality tests
    #[tokio::test]
    async fn test_basic_ssh_context() {
        let handler = SshConnectHandler;
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("user@host.com")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&ssh, &mut context).await.unwrap();

        assert_eq!(context.ssh_host, Some("user@host.com".to_string()));
    }

    #[tokio::test]
    async fn test_block_type() {
        let handler = SshConnectHandler;
        assert_eq!(handler.block_type(), "ssh-connect");
    }

    // Edge cases
    #[tokio::test]
    async fn test_empty_user_host_fails() {
        let handler = SshConnectHandler;
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("")
            .build();

        let mut context = ExecutionContext::default();
        let result = handler.apply_context(&ssh, &mut context).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("SSH user_host cannot be empty"));
    }

    #[tokio::test]
    async fn test_invalid_format_with_spaces() {
        let handler = SshConnectHandler;
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("invalid host name")
            .build();

        let mut context = ExecutionContext::default();
        let result = handler.apply_context(&ssh, &mut context).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid SSH user_host format"));
    }

    #[tokio::test]
    async fn test_hostname_only() {
        let handler = SshConnectHandler;
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("hostname.com")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&ssh, &mut context).await.unwrap();

        assert_eq!(context.ssh_host, Some("hostname.com".to_string()));
    }

    #[tokio::test]
    async fn test_ip_address() {
        let handler = SshConnectHandler;
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("192.168.1.100")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&ssh, &mut context).await.unwrap();

        assert_eq!(context.ssh_host, Some("192.168.1.100".to_string()));
    }

    #[tokio::test]
    async fn test_user_with_ip() {
        let handler = SshConnectHandler;
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("root@192.168.1.100")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&ssh, &mut context).await.unwrap();

        assert_eq!(context.ssh_host, Some("root@192.168.1.100".to_string()));
    }

    #[tokio::test]
    async fn test_with_port() {
        let handler = SshConnectHandler;
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("user@host.com:2222")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&ssh, &mut context).await.unwrap();

        assert_eq!(context.ssh_host, Some("user@host.com:2222".to_string()));
    }

    #[tokio::test]
    async fn test_hostname_with_port() {
        let handler = SshConnectHandler;
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("host.com:2222")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&ssh, &mut context).await.unwrap();

        assert_eq!(context.ssh_host, Some("host.com:2222".to_string()));
    }

    // Parsing tests
    #[tokio::test]
    async fn test_parse_user_host_full() {
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("user@host.com:2222")
            .build();

        let (user, host, port) = ssh.parse_user_host();
        assert_eq!(user, Some("user".to_string()));
        assert_eq!(host, "host.com");
        assert_eq!(port, Some(2222));
    }

    #[tokio::test]
    async fn test_parse_user_host_no_port() {
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("user@host.com")
            .build();

        let (user, host, port) = ssh.parse_user_host();
        assert_eq!(user, Some("user".to_string()));
        assert_eq!(host, "host.com");
        assert_eq!(port, None);
    }

    #[tokio::test]
    async fn test_parse_host_only() {
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("host.com")
            .build();

        let (user, host, port) = ssh.parse_user_host();
        assert_eq!(user, None);
        assert_eq!(host, "host.com");
        assert_eq!(port, None);
    }

    #[tokio::test]
    async fn test_parse_host_with_port() {
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("host.com:2222")
            .build();

        let (user, host, port) = ssh.parse_user_host();
        assert_eq!(user, None);
        assert_eq!(host, "host.com");
        assert_eq!(port, Some(2222));
    }

    #[tokio::test]
    async fn test_parse_invalid_port() {
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("host.com:invalid")
            .build();

        let (user, host, port) = ssh.parse_user_host();
        assert_eq!(user, None);
        assert_eq!(host, "host.com");
        assert_eq!(port, None); // Invalid port should be None
    }

    // Serialization tests
    #[tokio::test]
    async fn test_json_serialization_roundtrip() {
        let original = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("user@host.com")
            .build();

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: SshConnect = serde_json::from_str(&json).unwrap();

        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.user_host, deserialized.user_host);
    }

    #[tokio::test]
    async fn test_from_document_valid_camel_case() {
        let id = Uuid::new_v4();
        let json_data = serde_json::json!({
            "id": id.to_string(),
            "userHost": "user@host.com",
            "type": "ssh-connect"
        });

        let ssh = SshConnect::from_document(&json_data).unwrap();
        assert_eq!(ssh.id, id);
        assert_eq!(ssh.user_host, "user@host.com");
    }

    #[tokio::test]
    async fn test_from_document_valid_snake_case() {
        let id = Uuid::new_v4();
        let json_data = serde_json::json!({
            "id": id.to_string(),
            "user_host": "user@host.com",
            "type": "ssh-connect"
        });

        let ssh = SshConnect::from_document(&json_data).unwrap();
        assert_eq!(ssh.id, id);
        assert_eq!(ssh.user_host, "user@host.com");
    }

    #[tokio::test]
    async fn test_from_document_missing_user_host() {
        let json_data = serde_json::json!({
            "id": Uuid::new_v4().to_string(),
            "type": "ssh-connect"
        });

        let result = SshConnect::from_document(&json_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing userHost"));
    }

    // Integration tests
    #[tokio::test]
    async fn test_multiple_ssh_contexts_override() {
        let handler = SshConnectHandler;

        let ssh1 = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("user1@host1.com")
            .build();

        let ssh2 = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("user2@host2.com")
            .build();

        let mut context = ExecutionContext::default();

        // Apply first SSH connection
        handler.apply_context(&ssh1, &mut context).await.unwrap();
        assert_eq!(context.ssh_host, Some("user1@host1.com".to_string()));

        // Apply second SSH connection (should override)
        handler.apply_context(&ssh2, &mut context).await.unwrap();
        assert_eq!(context.ssh_host, Some("user2@host2.com".to_string()));
    }

    #[tokio::test]
    async fn test_ssh_context_preserves_other_fields() {
        let handler = SshConnectHandler;
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("user@host.com")
            .build();

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
            ssh_host: None,
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

        handler.apply_context(&ssh, &mut context).await.unwrap();

        // SSH host should be updated
        assert_eq!(context.ssh_host, Some("user@host.com".to_string()));

        // Other fields should be preserved
        assert_eq!(context.runbook_id, original_runbook_id);
        assert_eq!(context.cwd, original_cwd);
        assert_eq!(context.env, original_env);
        assert_eq!(context.variables, original_variables);
        assert_eq!(context.document, original_document);
    }

    #[tokio::test]
    async fn test_common_ssh_patterns() {
        let handler = SshConnectHandler;
        let test_cases = vec![
            "root@server.com",
            "deploy@192.168.1.100",
            "user@host.example.com:2222",
            "admin@10.0.0.1:22",
            "ubuntu@ec2-instance.amazonaws.com",
            "git@github.com",
            "localhost",
            "127.0.0.1",
        ];

        for user_host in test_cases {
            let ssh = SshConnect::builder()
                .id(Uuid::new_v4())
                .user_host(user_host)
                .build();

            let mut context = ExecutionContext::default();
            handler.apply_context(&ssh, &mut context).await.unwrap();
            assert_eq!(context.ssh_host, Some(user_host.to_string()));
        }
    }
}
