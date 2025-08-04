use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

use crate::runtime::blocks::handler::ExecutionContext;

#[derive(Debug, Clone)]
pub struct BlockInfo {
    pub id: String,
    pub block_type: String,
    pub props: HashMap<String, String>,
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
        
        // Find the target block
        let target_block = blocks
            .iter()
            .find(|b| b.id == block_id)
            .ok_or_else(|| format!("Block {} not found in document", block_id))?;
        
        // Collect all ancestor blocks
        let ancestors = Self::collect_ancestors(&target_block.id, &blocks);
        
        // Build context by applying each ancestor's contribution
        let mut context = ExecutionContext {
            runbook_id: Uuid::parse_str(runbook_id)?,
            cwd: std::env::current_dir()?.to_string_lossy().to_string(),
            env: HashMap::new(),
            variables: HashMap::new(),
            ssh_host: None,
            document: document.to_vec(),
        };
        
        // Apply context modifications from ancestors (in order from root to target)
        for ancestor in ancestors.iter().rev() {
            Self::apply_block_context(ancestor, &mut context)?;
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
    
    /// Collect all ancestors of a block (including the block itself)
    fn collect_ancestors(block_id: &str, blocks: &[BlockInfo]) -> Vec<BlockInfo> {
        let mut ancestors = Vec::new();
        let mut current_id = Some(block_id.to_string());
        
        while let Some(id) = current_id {
            if let Some(block) = blocks.iter().find(|b| b.id == id) {
                ancestors.push(block.clone());
                current_id = block.parent_id.clone();
            } else {
                break;
            }
        }
        
        ancestors
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
                if let (Some(name), Some(value)) = (block.props.get("name"), block.props.get("value")) {
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
            "var" => {
                if let (Some(name), Some(value)) = (block.props.get("name"), block.props.get("value")) {
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
        let document = vec![
            json!({
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
            })
        ];
        
        let context = ContextBuilder::build_context("script1", &document, "00000000-0000-0000-0000-000000000000")
            .await
            .unwrap();
        
        assert_eq!(context.cwd, "/tmp");
        assert_eq!(context.env.get("TEST_VAR"), Some(&"test_value".to_string()));
    }
}