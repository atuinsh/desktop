use crate::{
    blocks::{Block, BlockBehavior, FromDocument},
    client::LocalValueProvider,
    context::{BlockContext, ContextResolver, DocumentSshHost},
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct SshConnect {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub user_host: String,
}

impl FromDocument for SshConnect {
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

        let user_host = props
            .get("userHost")
            .or_else(|| props.get("user_host")) // Support both camelCase and snake_case
            .and_then(|v| v.as_str())
            .ok_or("Missing userHost")?
            .to_string();

        Ok(SshConnect::builder().id(id).user_host(user_host).build())
    }
}

impl SshConnect {
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

#[async_trait]
impl BlockBehavior for SshConnect {
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::SshConnect(self)
    }

    async fn passive_context(
        &self,
        resolver: &ContextResolver,
        _block_local_value_provider: Option<&dyn LocalValueProvider>,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        let mut context = BlockContext::new();
        let resolved_user_host = resolver.resolve_template(&self.user_host)?;

        // Basic validation of user_host format
        if resolved_user_host.is_empty() {
            return Err("SSH user_host cannot be empty".into());
        }

        // Basic format validation (should contain @ or be just hostname)
        if !resolved_user_host.contains('@') && resolved_user_host.contains(' ') {
            return Err("Invalid SSH user_host format".into());
        }

        context.insert(DocumentSshHost(Some(resolved_user_host)));
        Ok(Some(context))
    }
}

#[cfg(test)]
mod tests {
    use crate::context::ResolvedContext;

    use super::*;

    // Basic functionality tests
    #[tokio::test]
    async fn test_basic_ssh_context() {
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("user@host.com")
            .build();

        let context = ResolvedContext::from_block(&ssh, None).await.unwrap();

        assert_eq!(context.ssh_host, Some("user@host.com".to_string()));
    }

    // Edge cases
    #[tokio::test]
    async fn test_empty_user_host_fails() {
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("")
            .build();

        let context = ResolvedContext::from_block(&ssh, None).await;

        assert!(context.is_err());
    }

    #[tokio::test]
    async fn test_invalid_format_with_spaces() {
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("invalid host name")
            .build();

        let context = ResolvedContext::from_block(&ssh, None).await;
        assert!(context.is_err());
    }

    #[tokio::test]
    async fn test_hostname_only() {
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("hostname.com")
            .build();

        let context = ResolvedContext::from_block(&ssh, None).await.unwrap();
        assert_eq!(context.ssh_host, Some("hostname.com".to_string()));
    }

    #[tokio::test]
    async fn test_ip_address() {
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("192.168.1.100")
            .build();

        let context = ResolvedContext::from_block(&ssh, None).await.unwrap();
        assert_eq!(context.ssh_host, Some("192.168.1.100".to_string()));
    }

    #[tokio::test]
    async fn test_user_with_ip() {
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("root@192.168.1.100")
            .build();

        let context = ResolvedContext::from_block(&ssh, None).await.unwrap();
        assert_eq!(context.ssh_host, Some("root@192.168.1.100".to_string()));
    }

    #[tokio::test]
    async fn test_with_port() {
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("user@host.com:2222")
            .build();

        let context = ResolvedContext::from_block(&ssh, None).await.unwrap();

        assert_eq!(context.ssh_host, Some("user@host.com:2222".to_string()));
    }

    #[tokio::test]
    async fn test_hostname_with_port() {
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("host.com:2222")
            .build();

        let context = ResolvedContext::from_block(&ssh, None).await.unwrap();
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
            "props": {
                "userHost": "user@host.com"
            },
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
            "props": {
                "user_host": "user@host.com"
            },
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
            "props": {},
            "type": "ssh-connect"
        });

        let result = SshConnect::from_document(&json_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing userHost"));
    }

    #[tokio::test]
    async fn test_common_ssh_patterns() {
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

            let context = ResolvedContext::from_block(&ssh, None).await.unwrap();
            assert_eq!(context.ssh_host, Some(user_host.to_string()));
        }
    }
}
