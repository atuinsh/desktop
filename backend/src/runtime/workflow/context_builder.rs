use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

use crate::runtime::blocks::context::{
    directory::{Directory, DirectoryHandler},
    environment::{Environment, EnvironmentHandler},
    host::{Host, HostHandler},
    local_var::{LocalVar, LocalVarHandler},
    ssh_connect::{SshConnect, SshConnectHandler},
    var::{Var, VarHandler},
};
use crate::runtime::blocks::handler::{ContextProvider, ExecutionContext};

#[derive(Debug, Clone)]
pub struct BlockInfo {
    pub id: String,
    pub block_type: String,
    pub props: HashMap<String, String>,
    #[allow(dead_code)] // May be useful for future hierarchical context features
    pub parent_id: Option<String>,
}

pub struct ContextBuilder;

impl ContextBuilder {
    /// Build execution context by walking up the document tree from the target block
    pub async fn build_context(
        block_id: &str,
        document: &[Value],
        runbook_id: &str,
    ) -> Result<ExecutionContext, Box<dyn std::error::Error>> {
        // First, flatten the document and build parent relationships
        let blocks = Self::flatten_document(document)?;

        // Find the target block index

        // Find all blocks that come before the target block in document order
        let target_index = blocks
            .iter()
            .position(|b| b.id == block_id)
            .ok_or_else(|| format!("Block {} not found in flattened document", block_id))?;

        let preceding_blocks = &blocks[..target_index];

        // Build context by applying each preceding block's contribution
        let mut context = ExecutionContext {
            runbook_id: Uuid::parse_str(runbook_id)?,
            cwd: std::env::current_dir()?.to_string_lossy().to_string(),
            env: HashMap::new(),
            variables: HashMap::new(),
            ssh_host: None,
            document: document.to_vec(),
            ssh_pool: None,       // Will be set by the caller if needed
            output_storage: None, // Will be set by the caller if needed
            pty_store: None,      // Will be set by the caller if needed
            event_bus: None,      // Will be set by the caller if needed
        };

        // Apply context modifications from preceding blocks (in document order)
        for block in preceding_blocks {
            Self::apply_block_context(block, &mut context).await?;
        }

        Ok(context)
    }

    /// Flatten the nested document structure into a flat list with parent relationships
    fn flatten_document(document: &[Value]) -> Result<Vec<BlockInfo>, Box<dyn std::error::Error>> {
        let mut blocks = Vec::new();
        Self::flatten_recursive(document, None, &mut blocks)?;
        Ok(blocks)
    }

    fn flatten_recursive(
        nodes: &[Value],
        parent_id: Option<String>,
        blocks: &mut Vec<BlockInfo>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for node in nodes {
            let id = node
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or("Block missing id")?
                .to_string();

            let block_type = node
                .get("type")
                .and_then(|v| v.as_str())
                .ok_or("Block missing type")?
                .to_string();

            let props = node
                .get("props")
                .and_then(|v| v.as_object())
                .map(|obj| {
                    obj.iter()
                        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                        .collect()
                })
                .unwrap_or_default();

            blocks.push(BlockInfo {
                id: id.clone(),
                block_type,
                props,
                parent_id: parent_id.clone(),
            });

            // Recursively process children
            if let Some(children) = node.get("children").and_then(|v| v.as_array()) {
                Self::flatten_recursive(children, Some(id), blocks)?;
            }
        }

        Ok(())
    }

    /// Apply a block's context modifications
    async fn apply_block_context(
        block: &BlockInfo,
        context: &mut ExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match block.block_type.as_str() {
            "directory" => {
                if let Some(path) = block.props.get("path") {
                    if !path.is_empty() {
                        let directory_block = Directory::builder()
                            .id(uuid::Uuid::parse_str(&block.id)?)
                            .path(path.clone())
                            .build();
                        DirectoryHandler
                            .apply_context(&directory_block, context)
                            .await
                            .map_err(|e| format!("Directory context error: {}", e))?;
                    }
                }
            }
            "env" => {
                if let (Some(name), Some(value)) =
                    (block.props.get("name"), block.props.get("value"))
                {
                    if !name.is_empty() {
                        let env_block = Environment::builder()
                            .id(uuid::Uuid::parse_str(&block.id)?)
                            .name(name.clone())
                            .value(value.clone())
                            .build();
                        EnvironmentHandler
                            .apply_context(&env_block, context)
                            .await
                            .map_err(|e| format!("Environment context error: {}", e))?;
                    }
                }
            }
            "ssh-connect" => {
                if let Some(user_host) = block.props.get("userHost") {
                    if !user_host.is_empty() {
                        let ssh_block = SshConnect::builder()
                            .id(uuid::Uuid::parse_str(&block.id)?)
                            .user_host(user_host.clone())
                            .build();
                        SshConnectHandler
                            .apply_context(&ssh_block, context)
                            .await
                            .map_err(|e| format!("SSH context error: {}", e))?;
                    }
                }
            }
            "host-select" => {
                if let Some(host) = block.props.get("host") {
                    let host_block = Host::builder()
                        .id(uuid::Uuid::parse_str(&block.id)?)
                        .host(host.clone())
                        .build();
                    HostHandler
                        .apply_context(&host_block, context)
                        .await
                        .map_err(|e| format!("Host context error: {}", e))?;
                }
            }
            "var" => {
                if let (Some(name), Some(value)) =
                    (block.props.get("name"), block.props.get("value"))
                {
                    if !name.is_empty() {
                        let var_block = Var::builder()
                            .id(uuid::Uuid::parse_str(&block.id)?)
                            .name(name.clone())
                            .value(value.clone())
                            .build();
                        VarHandler
                            .apply_context(&var_block, context)
                            .await
                            .map_err(|e| format!("Var context error: {}", e))?;
                    }
                }
            }
            "local-var" => {
                if let Some(name) = block.props.get("name") {
                    if !name.is_empty() {
                        let local_var_block = LocalVar::builder()
                            .id(uuid::Uuid::parse_str(&block.id)?)
                            .name(name.clone())
                            .build();
                        LocalVarHandler
                            .apply_context(&local_var_block, context)
                            .await
                            .map_err(|e| format!("Local var context error: {}", e))?;
                    }
                }
            }
            _ => {
                // Other block types don't affect context
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_context_builder() {
        let document = vec![json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "type": "directory",
            "props": { "path": "/tmp" },
            "children": [
                {
                    "id": "00000000-0000-0000-0000-000000000002",
                    "type": "env",
                    "props": { "name": "TEST_VAR", "value": "test_value" }
                },
                {
                    "id": "00000000-0000-0000-0000-000000000003",
                    "type": "script",
                    "props": { "code": "echo $TEST_VAR" }
                }
            ]
        })];

        let context = ContextBuilder::build_context(
            "00000000-0000-0000-0000-000000000003",
            &document,
            "00000000-0000-0000-0000-000000000000",
        )
        .await
        .unwrap();

        assert_eq!(context.cwd, "/tmp");
        assert_eq!(context.env.get("TEST_VAR"), Some(&"test_value".to_string()));
    }

    #[tokio::test]
    async fn test_context_builder_host_select() {
        let document = vec![json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "type": "host-select",
            "props": { "host": "local" },
            "children": [
                {
                    "id": "00000000-0000-0000-0000-000000000002",
                    "type": "host-select",
                    "props": { "host": "user@remote.com" }
                },
                {
                    "id": "00000000-0000-0000-0000-000000000003",
                    "type": "script",
                    "props": { "code": "echo 'test'" }
                }
            ]
        })];

        let context = ContextBuilder::build_context(
            "00000000-0000-0000-0000-000000000003",
            &document,
            "00000000-0000-0000-0000-000000000000",
        )
        .await
        .unwrap();

        // Should have SSH host set by the second host-select block
        assert_eq!(context.ssh_host, Some("user@remote.com".to_string()));
    }

    #[tokio::test]
    async fn test_context_builder_local_var() {
        let document = vec![json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "type": "local-var",
            "props": { "name": "secret_key" },
            "children": [
                {
                    "id": "00000000-0000-0000-0000-000000000002",
                    "type": "script",
                    "props": { "code": "echo $secret_key" }
                }
            ]
        })];

        let context = ContextBuilder::build_context(
            "00000000-0000-0000-0000-000000000002",
            &document,
            "00000000-0000-0000-0000-000000000000",
        )
        .await
        .unwrap();

        // Should add empty value for the local variable (since no stored value in test)
        assert_eq!(context.variables.get("secret_key"), Some(&String::new()));
    }

    #[tokio::test]
    async fn test_context_builder_local_var_empty_name() {
        let document = vec![json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "type": "local-var",
            "props": { "name": "" },
            "children": [
                {
                    "id": "00000000-0000-0000-0000-000000000002",
                    "type": "script",
                    "props": { "code": "echo test" }
                }
            ]
        })];

        let context = ContextBuilder::build_context(
            "00000000-0000-0000-0000-000000000002",
            &document,
            "00000000-0000-0000-0000-000000000000",
        )
        .await
        .unwrap();

        // Should not add anything for empty name
        assert!(context.variables.is_empty());
    }

    #[tokio::test]
    async fn test_context_builder_host_select_local() {
        let document = vec![json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "type": "host-select",
            "props": { "host": "local" },
            "children": [
                {
                    "id": "00000000-0000-0000-0000-000000000002",
                    "type": "script",
                    "props": { "code": "echo 'test'" }
                }
            ]
        })];

        let context = ContextBuilder::build_context(
            "00000000-0000-0000-0000-000000000002",
            &document,
            "00000000-0000-0000-0000-000000000000",
        )
        .await
        .unwrap();

        // Should have no SSH host (local execution)
        assert_eq!(context.ssh_host, None);
    }

    #[tokio::test]
    async fn test_context_builder_var() {
        let document = vec![json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "type": "var",
            "props": { "name": "MY_VAR", "value": "my_value" },
            "children": [
                {
                    "id": "00000000-0000-0000-0000-000000000002",
                    "type": "script",
                    "props": { "code": "echo $MY_VAR" }
                }
            ]
        })];

        let context = ContextBuilder::build_context(
            "00000000-0000-0000-0000-000000000002",
            &document,
            "00000000-0000-0000-0000-000000000000",
        )
        .await
        .unwrap();

        // Should have the variable set
        assert_eq!(
            context.variables.get("MY_VAR"),
            Some(&"my_value".to_string())
        );
    }

    #[tokio::test]
    async fn test_context_builder_var_empty_name() {
        let document = vec![json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "type": "var",
            "props": { "name": "", "value": "my_value" },
            "children": [
                {
                    "id": "00000000-0000-0000-0000-000000000002",
                    "type": "script",
                    "props": { "code": "echo test" }
                }
            ]
        })];

        let context = ContextBuilder::build_context(
            "00000000-0000-0000-0000-000000000002",
            &document,
            "00000000-0000-0000-0000-000000000000",
        )
        .await
        .unwrap();

        // Should not add anything for empty name
        assert!(context.variables.is_empty());
    }

    #[tokio::test]
    async fn test_context_builder_var_invalid_name() {
        let document = vec![json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "type": "var",
            "props": { "name": "INVALID NAME!", "value": "my_value" },
            "children": [
                {
                    "id": "00000000-0000-0000-0000-000000000002",
                    "type": "script",
                    "props": { "code": "echo test" }
                }
            ]
        })];

        let result = ContextBuilder::build_context(
            "00000000-0000-0000-0000-000000000002",
            &document,
            "00000000-0000-0000-0000-000000000000",
        )
        .await;

        // Should fail due to invalid variable name
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e
                .to_string()
                .contains("can only contain letters, numbers, and underscores"));
        }
    }
}
