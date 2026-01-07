use crate::{
    blocks::{Block, BlockBehavior, FromDocument},
    client::LocalValueProvider,
    context::{
        BlockContext, ContextResolver, DocumentSshConfig, DocumentSshHost, SshIdentityKeyConfig,
    },
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

    /// The user@host:port shorthand (used when settings are not configured)
    #[builder(setter(into))]
    pub user_host: String,

    /// Optional user override (when set, disables userHost input)
    #[builder(default, setter(strip_option))]
    pub user: Option<String>,

    /// Optional hostname override (when set, disables userHost input)
    #[builder(default, setter(strip_option))]
    pub hostname: Option<String>,

    /// Optional port override
    #[builder(default, setter(strip_option))]
    pub port: Option<u16>,
    // Note: identity_key is stored in block local storage (per-user, not synced)
    // and is retrieved via LocalValueProvider in passive_context
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
            .or_else(|| props.get("user_host"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let user = props
            .get("user")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let hostname = props
            .get("hostname")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let port = match props.get("port") {
            Some(v) => match v.as_u64() {
                Some(0) => None, // 0 means "not set"
                Some(p) if p <= 65535 => Some(p as u16),
                Some(p) => return Err(format!("Invalid SSH port: {} (must be 1-65535)", p)),
                None => return Err("Invalid SSH port: expected a number".into()),
            },
            None => None,
        };

        Ok(SshConnect {
            id,
            user_host,
            user,
            hostname,
            port,
        })
    }
}

impl SshConnect {
    /// Parse identity key configuration from local storage value
    fn parse_identity_key_from_local(value: &str) -> Option<SshIdentityKeyConfig> {
        // The value is JSON-encoded: {"mode": "...", "value": "..."}
        let parsed: serde_json::Value = serde_json::from_str(value).ok()?;

        let mode = parsed.get("mode").and_then(|v| v.as_str())?;
        let key_value = parsed.get("value").and_then(|v| v.as_str()).unwrap_or("");

        match mode {
            "none" | "" => Some(SshIdentityKeyConfig::None),
            "paste" => {
                if key_value.is_empty() {
                    None
                } else {
                    Some(SshIdentityKeyConfig::Paste {
                        content: key_value.to_string(),
                    })
                }
            }
            "path" => {
                if key_value.is_empty() {
                    None
                } else {
                    Some(SshIdentityKeyConfig::Path {
                        path: key_value.to_string(),
                    })
                }
            }
            _ => None,
        }
    }

    /// Check if explicit settings are configured (user or hostname set)
    pub fn has_explicit_config(&self) -> bool {
        self.user.is_some() || self.hostname.is_some()
    }

    /// Get the effective user@host:port string for display
    pub fn effective_user_host(&self) -> String {
        if self.has_explicit_config() {
            let mut result = String::new();
            if let Some(ref user) = self.user {
                result.push_str(user);
                result.push('@');
            }
            if let Some(ref hostname) = self.hostname {
                result.push_str(hostname);
            }
            if let Some(port) = self.port {
                result.push(':');
                result.push_str(&port.to_string());
            }
            result
        } else {
            self.user_host.clone()
        }
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
        block_local_value_provider: Option<&dyn LocalValueProvider>,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        let mut context = BlockContext::new();

        let resolved_user_host = resolver.resolve_template(&self.user_host)?;
        let resolved_user = match &self.user {
            Some(u) => Some(resolver.resolve_template(u)?),
            None => None,
        };
        let resolved_hostname = match &self.hostname {
            Some(h) => Some(resolver.resolve_template(h)?),
            None => None,
        };

        let has_user = resolved_user.as_ref().is_some_and(|u| !u.is_empty());
        let has_hostname = resolved_hostname.as_ref().is_some_and(|h| !h.is_empty());
        if has_user != has_hostname {
            return Err(
                "SSH settings require both user and hostname to be set (or neither)".into(),
            );
        }

        let effective_user_host = if has_user && has_hostname {
            let mut result = String::new();
            if let Some(ref user) = resolved_user {
                result.push_str(user);
                result.push('@');
            }
            if let Some(ref hostname) = resolved_hostname {
                result.push_str(hostname);
            }
            if let Some(port) = self.port {
                result.push(':');
                result.push_str(&port.to_string());
            }
            result
        } else {
            resolved_user_host.clone()
        };

        if effective_user_host.is_empty() && resolved_hostname.is_none() {
            return Err("SSH connection requires either userHost or hostname in settings".into());
        }

        if !effective_user_host.is_empty()
            && !effective_user_host.contains('@')
            && effective_user_host.contains(' ')
        {
            return Err("Invalid SSH user_host format".into());
        }

        let identity_key = if let Some(provider) = block_local_value_provider {
            match provider.get_block_local_value(self.id, "identityKey").await {
                Ok(Some(value)) => {
                    tracing::debug!("Block {} read identityKey from KV: {}", self.id, value);
                    Self::parse_identity_key_from_local(&value)
                }
                Ok(None) => {
                    tracing::debug!("Block {} has no identityKey in KV", self.id);
                    None
                }
                Err(e) => {
                    tracing::warn!("Failed to get identity key from local storage: {}", e);
                    None
                }
            }
        } else {
            tracing::debug!("Block {} has no block_local_value_provider", self.id);
            None
        };
        tracing::debug!(
            "Block {} resolved identity_key to: {:?}",
            self.id,
            identity_key
        );

        // Backwards compatibility with older blocks that only check DocumentSshHost
        context.insert(DocumentSshHost(Some(effective_user_host.clone())));

        context.insert(DocumentSshConfig {
            user_host: effective_user_host,
            user: resolved_user,
            hostname: resolved_hostname,
            port: self.port,
            identity_key,
        });

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
    async fn test_from_document_empty_props_succeeds() {
        // from_document now succeeds with empty userHost (hostname might be in settings)
        // Validation happens in passive_context
        let json_data = serde_json::json!({
            "id": Uuid::new_v4().to_string(),
            "props": {},
            "type": "ssh-connect"
        });

        let result = SshConnect::from_document(&json_data);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().user_host, "");
    }

    #[tokio::test]
    async fn test_empty_props_context_fails() {
        // When neither userHost nor hostname is set, passive_context should fail
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("")
            .build();

        let context = ResolvedContext::from_block(&ssh, None).await;
        assert!(context.is_err());
    }

    #[tokio::test]
    async fn test_explicit_config_requires_both_user_and_hostname() {
        // Setting only user without hostname should fail
        let ssh_user_only = SshConnect {
            id: Uuid::new_v4(),
            user_host: "".to_string(),
            user: Some("root".to_string()),
            hostname: None,
            port: None,
        };
        let result = ResolvedContext::from_block(&ssh_user_only, None).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("both user and hostname"));

        // Setting only hostname without user should fail
        let ssh_hostname_only = SshConnect {
            id: Uuid::new_v4(),
            user_host: "".to_string(),
            user: None,
            hostname: Some("example.com".to_string()),
            port: None,
        };
        let result = ResolvedContext::from_block(&ssh_hostname_only, None).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("both user and hostname"));

        // Setting both user and hostname should succeed
        let ssh_both = SshConnect {
            id: Uuid::new_v4(),
            user_host: "".to_string(),
            user: Some("root".to_string()),
            hostname: Some("example.com".to_string()),
            port: Some(22),
        };
        let result = ResolvedContext::from_block(&ssh_both, None).await;
        assert!(result.is_ok());
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

    #[test]
    fn test_parse_identity_key_from_local_none() {
        let json = r#"{"mode": "none", "value": ""}"#;
        let result = SshConnect::parse_identity_key_from_local(json);
        assert_eq!(result, Some(SshIdentityKeyConfig::None));

        // Empty mode string also means None
        let json = r#"{"mode": "", "value": ""}"#;
        let result = SshConnect::parse_identity_key_from_local(json);
        assert_eq!(result, Some(SshIdentityKeyConfig::None));
    }

    #[test]
    fn test_parse_identity_key_from_local_paste() {
        let json = r#"{"mode": "paste", "value": "-----BEGIN OPENSSH PRIVATE KEY-----\ntest\n-----END OPENSSH PRIVATE KEY-----"}"#;
        let result = SshConnect::parse_identity_key_from_local(json);
        assert!(matches!(result, Some(SshIdentityKeyConfig::Paste { .. })));
        if let Some(SshIdentityKeyConfig::Paste { content }) = result {
            assert!(content.contains("BEGIN OPENSSH PRIVATE KEY"));
        }

        // Empty value returns None (invalid config)
        let json = r#"{"mode": "paste", "value": ""}"#;
        let result = SshConnect::parse_identity_key_from_local(json);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_identity_key_from_local_path() {
        let json = r#"{"mode": "path", "value": "/home/user/.ssh/id_ed25519"}"#;
        let result = SshConnect::parse_identity_key_from_local(json);
        assert!(matches!(result, Some(SshIdentityKeyConfig::Path { .. })));
        if let Some(SshIdentityKeyConfig::Path { path }) = result {
            assert_eq!(path, "/home/user/.ssh/id_ed25519");
        }

        // Empty value returns None (invalid config)
        let json = r#"{"mode": "path", "value": ""}"#;
        let result = SshConnect::parse_identity_key_from_local(json);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_identity_key_from_local_invalid() {
        // Invalid JSON
        let result = SshConnect::parse_identity_key_from_local("not json");
        assert!(result.is_none());

        // Missing mode field
        let result = SshConnect::parse_identity_key_from_local(r#"{"value": "test"}"#);
        assert!(result.is_none());

        // Unknown mode
        let result =
            SshConnect::parse_identity_key_from_local(r#"{"mode": "unknown", "value": "test"}"#);
        assert!(result.is_none());
    }

    #[test]
    fn test_from_document_invalid_port() {
        let json_data = serde_json::json!({
            "id": Uuid::new_v4().to_string(),
            "props": {
                "userHost": "user@host.com",
                "port": 99999
            },
            "type": "ssh-connect"
        });

        let result = SshConnect::from_document(&json_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid SSH port"));
    }

    #[test]
    fn test_has_explicit_config() {
        let ssh_none = SshConnect {
            id: Uuid::new_v4(),
            user_host: "user@host".to_string(),
            user: None,
            hostname: None,
            port: None,
        };
        assert!(!ssh_none.has_explicit_config());

        let ssh_user = SshConnect {
            id: Uuid::new_v4(),
            user_host: "".to_string(),
            user: Some("root".to_string()),
            hostname: None,
            port: None,
        };
        assert!(ssh_user.has_explicit_config());

        let ssh_hostname = SshConnect {
            id: Uuid::new_v4(),
            user_host: "".to_string(),
            user: None,
            hostname: Some("example.com".to_string()),
            port: None,
        };
        assert!(ssh_hostname.has_explicit_config());

        let ssh_both = SshConnect {
            id: Uuid::new_v4(),
            user_host: "".to_string(),
            user: Some("root".to_string()),
            hostname: Some("example.com".to_string()),
            port: Some(22),
        };
        assert!(ssh_both.has_explicit_config());
    }

    #[test]
    fn test_effective_user_host() {
        // No explicit config - returns user_host
        let ssh = SshConnect {
            id: Uuid::new_v4(),
            user_host: "user@host.com".to_string(),
            user: None,
            hostname: None,
            port: None,
        };
        assert_eq!(ssh.effective_user_host(), "user@host.com");

        // Explicit config - builds from parts
        let ssh = SshConnect {
            id: Uuid::new_v4(),
            user_host: "".to_string(),
            user: Some("root".to_string()),
            hostname: Some("example.com".to_string()),
            port: None,
        };
        assert_eq!(ssh.effective_user_host(), "root@example.com");

        // With port
        let ssh = SshConnect {
            id: Uuid::new_v4(),
            user_host: "".to_string(),
            user: Some("admin".to_string()),
            hostname: Some("server.io".to_string()),
            port: Some(2222),
        };
        assert_eq!(ssh.effective_user_host(), "admin@server.io:2222");
    }
}
