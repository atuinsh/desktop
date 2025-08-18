use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

use crate::runtime::blocks::handler::ExecutionContext;

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
            Self::apply_block_context(block, &mut context)?;
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
    fn apply_block_context(
        block: &BlockInfo,
        context: &mut ExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match block.block_type.as_str() {
            "directory" => {
                if let Some(path) = block.props.get("path") {
                    if !path.is_empty() {
                        context.cwd = path.clone();
                    }
                }
            }
            "env" => {
                if let (Some(name), Some(value)) =
                    (block.props.get("name"), block.props.get("value"))
                {
                    if !name.is_empty() {
                        context.env.insert(name.clone(), value.clone());
                    }
                }
            }
            "ssh-connect" => {
                if let Some(user_host) = block.props.get("userHost") {
                    if !user_host.is_empty() {
                        context.ssh_host = Some(user_host.clone());
                    }
                }
            }
            "host-select" => {
                if let Some(host) = block.props.get("host") {
                    let host = host.trim();
                    if host.is_empty() || host == "local" || host == "localhost" {
                        context.ssh_host = None; // Switch to local execution
                    } else {
                        context.ssh_host = Some(host.to_string()); // Switch to SSH execution
                    }
                }
            }
            "var" => {
                if let (Some(name), Some(value)) =
                    (block.props.get("name"), block.props.get("value"))
                {
                    if !name.is_empty() {
                        context.variables.insert(name.clone(), value.clone());
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
            "id": "root",
            "type": "directory",
            "props": { "path": "/tmp" },
            "children": [
                {
                    "id": "env1",
                    "type": "env",
                    "props": { "name": "TEST_VAR", "value": "test_value" }
                },
                {
                    "id": "script1",
                    "type": "script",
                    "props": { "code": "echo $TEST_VAR" }
                }
            ]
        })];

        let context = ContextBuilder::build_context(
            "script1",
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
            "id": "root",
            "type": "host-select",
            "props": { "host": "local" },
            "children": [
                {
                    "id": "host2",
                    "type": "host-select",
                    "props": { "host": "user@remote.com" }
                },
                {
                    "id": "script1",
                    "type": "script",
                    "props": { "code": "echo 'test'" }
                }
            ]
        })];

        let context = ContextBuilder::build_context(
            "script1",
            &document,
            "00000000-0000-0000-0000-000000000000",
        )
        .await
        .unwrap();

        // Should have SSH host set by the second host-select block
        assert_eq!(context.ssh_host, Some("user@remote.com".to_string()));
    }

    #[tokio::test]
    async fn test_context_builder_host_select_local() {
        let document = vec![json!({
            "id": "host1",
            "type": "host-select",
            "props": { "host": "local" },
            "children": [
                {
                    "id": "script1",
                    "type": "script",
                    "props": { "code": "echo 'test'" }
                }
            ]
        })];

        let context = ContextBuilder::build_context(
            "script1",
            &document,
            "00000000-0000-0000-0000-000000000000",
        )
        .await
        .unwrap();

        // Should have no SSH host (local execution)
        assert_eq!(context.ssh_host, None);
    }
}
