use minijinja::{
    value::{Enumerator, Object},
    Environment, Value,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};

use crate::state::AtuinState;

/// Flatten a document to include nested blocks (like those in ToggleHeading children)
/// This creates a linear execution order regardless of UI nesting structure
pub fn flatten_document(doc: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut flattened = Vec::new();
    for block in doc {
        flattened.push(block.clone());
        if let Some(children) = block.get("children").and_then(|c| c.as_array()) {
            if !children.is_empty() {
                flattened.extend(flatten_document(children));
            }
        }
    }
    flattened
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyTemplateState {
    pub id: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunbookTemplateState {
    pub id: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceTemplateState {
    /// The root path of the workspace containing atuin.toml
    /// Empty string for online workspaces (no concept of root)
    pub root: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtuinTemplateState {
    pub runbook: RunbookTemplateState,
    pub workspace: WorkspaceTemplateState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockState {
    /// Name it something that's not just type lol
    pub block_type: String,
    pub content: String,
    pub props: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentTemplateState {
    pub first: BlockState,
    pub last: BlockState,
    pub content: Vec<BlockState>,
    pub named: HashMap<String, BlockState>,

    /// The block previous to the current block
    /// This is, of course, contextual to what is presently executing - and
    /// only really makes sense if we're running a template from a Pty.
    /// We can use the pty map to lookup the metadata for the pty, and from there figure
    /// out the block ID
    pub previous: Option<BlockState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateState {
    // In the case where a document is empty, we have no document state.
    pub doc: Option<DocumentTemplateState>,
    pub var: HashMap<String, Value>,
    pub workspace: WorkspaceTemplateState,
}

impl Object for TemplateState {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        match key.as_str()? {
            "var" => Some(Value::make_object_map(
                self.clone(),
                |this| Box::new(this.var.keys().map(Value::from)),
                |this, key| this.var.get(key.as_str()?).cloned(),
            )),

            "doc" => self
                .doc
                .as_ref()
                .map(|doc| Value::from_serialize(doc.clone())),

            "workspace" => Some(Value::from_serialize(&self.workspace)),

            _ => None,
        }
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Str(&["var", "doc", "workspace"])
    }
}

pub fn serialized_block_to_state(block: serde_json::Value) -> BlockState {
    let block = match block.as_object() {
        Some(obj) => obj,
        None => {
            // Return a default block state for non-object values
            return BlockState {
                block_type: "unknown".to_string(),
                content: String::new(),
                props: HashMap::new(),
            };
        }
    };

    let block_type = block
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("unknown");

    // Content is tricky. It's actually an array, that might look something like this
    // ```
    // [
    //  {"styles":{},"text":"BIG ","type":"text"},
    //  {"styles":{"textColor":"blue"},"text":"BLOCK","type":"text"},
    //  {"styles":{},"text":" OF TEXT","type":"text"}
    // ]
    // ```
    // It might be fun to someday turn that into some ascii-coloured text in the terminal,
    // but for now we should just flatten it into a single string

    // 1. Get the content
    let content = block.get("content");

    // 2. Flatten into a single string. Ensure the type of each element is "text", ignore what
    //    isn't for now. If it's none, we can just ignore it
    let content = if let Some(content) = content {
        // Ensure it actually is an array
        match content {
            serde_json::Value::String(val) => val.clone(),
            serde_json::Value::Number(val) => val.to_string(),

            serde_json::Value::Array(val) => {
                let content = val.iter().filter_map(|v| {
                    let v = v.as_object()?;
                    if v.get("type")?.as_str()? == "text" {
                        Some(v.get("text")?.as_str()?.to_string())
                    } else {
                        None
                    }
                });
                content.collect::<Vec<String>>().join("")
            }

            _ => String::from(""),
        }
    } else {
        String::from("")
    };

    let props: HashMap<String, String> = block
        .get("props")
        .and_then(|p| p.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Now for some block-specific stuff

    // 1. For the editor block, the "code" prop contains the contents of the editor. It would be
    //    better if the template system exposed that as "content", even though it isn't stored like
    //    that in the internal schema
    let content: String = match block_type {
        "editor" => props.get("code").cloned().unwrap_or(String::from("")),

        "run" => props.get("code").cloned().unwrap_or(String::from("")),
        _ => content,
    };

    BlockState {
        block_type: block_type.to_string(),
        props,
        content: content.to_string(),
    }
}

#[tauri::command]
pub async fn template_str(
    source: String,
    block_id: String,
    runbook: String,
    state: tauri::State<'_, AtuinState>,
    doc: Vec<serde_json::Value>,
    workspace_root: Option<String>,
) -> Result<String, String> {
    // Determine workspace root - try parameter first, then try to get from workspace manager
    let workspace_root = if let Some(root) = workspace_root {
        root
    } else {
        // Try to get workspace root from the runbook's workspace
        if let Some(workspace_manager) = state.workspaces.lock().await.as_ref() {
            workspace_manager
                .workspace_root(&runbook)
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_default()
        } else {
            String::new()
        }
    };

    // Fetch the variables from the state
    let mut output_vars = state.runbook_output_variables.read().await.clone();

    let flattened_doc = flatten_document(&doc);

    // We might also have var blocks in the document, which we need to add to the output vars
    let var_blocks = flattened_doc
        .iter()
        .filter(|block| block.get("type").unwrap().as_str().unwrap() == "var")
        .collect::<Vec<_>>();
    for block in var_blocks {
        let props = match block.get("props") {
            Some(props) => props,
            None => continue,
        };

        let name = match props.get("name").and_then(|n| n.as_str()) {
            Some(name) => name,
            None => continue,
        };

        // Only add the var if it doesn't already exist, as it might have been set by something else
        if let Some(vars) = output_vars.get(&runbook) {
            if vars.contains_key(name) {
                continue;
            }
        }

        let value = match props.get("value").and_then(|v| v.as_str()) {
            Some(value) => value,
            None => continue,
        };

        output_vars
            .entry(runbook.clone())
            .or_insert_with(HashMap::new)
            .insert(name.to_string(), value.to_string());
    }

    let mut env = Environment::new();
    env.set_trim_blocks(true);

    // Add custom filter for shell escaping
    env.add_filter("shellquote", |value: String| -> String {
        // Use POSIX shell single-quote escaping:
        // wrap in single quotes and escape any single quotes as '\''
        format!("'{}'", value.replace('\'', "'\\''"))
    });

    // Iterate through the flattened doc, and find the block previous to the current one
    // If the previous block is an empty paragraph, skip it. Its content array will have 0
    // length
    let previous = flattened_doc.iter().enumerate().find_map(|(i, block)| {
        let block = block.as_object().unwrap();
        if block.get("id").unwrap().as_str().unwrap() == block_id {
            if i == 0 {
                None
            } else {
                // Continue iterating backwards until we find a block that isn't an empty
                // paragraph
                let mut i = i - 1;
                loop {
                    let block = flattened_doc.get(i).unwrap().as_object().unwrap();
                    if block.get("type").unwrap().as_str().unwrap() == "paragraph"
                        && block.get("content").unwrap().as_array().unwrap().is_empty()
                    {
                        if i == 0 {
                            return None;
                        } else {
                            i -= 1;
                        }
                    } else {
                        return Some(flattened_doc.get(i).unwrap().clone());
                    }
                }
            }
        } else {
            None
        }
    });

    let named = flattened_doc
        .iter()
        .filter_map(|block| {
            let name = block
                .get("props")
                .unwrap()
                .as_object()
                .unwrap()
                .iter()
                .find_map(|(k, v)| {
                    if k == "name" {
                        Some(v.as_str().unwrap().to_string())
                    } else {
                        None
                    }
                });

            name.map(|name| (name, serialized_block_to_state(block.clone())))
        })
        .collect();

    let previous = previous.map(serialized_block_to_state);
    let var = output_vars
        .get(&runbook)
        .map_or(HashMap::new(), |v| v.clone());
    // convert into map of string -> value
    let var = var
        .iter()
        .map(|(k, v)| (k.to_string(), Value::from(v.to_string())))
        .collect::<HashMap<String, Value>>();

    let doc_state = if !doc.is_empty() {
        Some(DocumentTemplateState {
            previous,
            first: serialized_block_to_state(doc.first().unwrap().clone()),
            last: serialized_block_to_state(doc.last().unwrap().clone()),
            content: doc.into_iter().map(serialized_block_to_state).collect(),
            named,
        })
    } else {
        None
    };

    let template_state = TemplateState {
        doc: doc_state,
        var,
        workspace: WorkspaceTemplateState {
            root: workspace_root,
        },
    };

    let source = env
        .render_str(source.as_str(), template_state)
        .map_err(|e| e.to_string())?;

    Ok(source)
}

/// Template a string with the given variables and document context
/// This is a simplified version of template_str that doesn't require Tauri state
pub fn template_with_context(
    source: &str,
    variables: &HashMap<String, String>,
    document: &[serde_json::Value],
    block_id: Option<&str>,
) -> Result<String, String> {
    // If no variables and empty document, and source has no template syntax, return original source
    if variables.is_empty() && document.is_empty() && !source.contains("{{") {
        return Ok(source.to_string());
    }

    let flattened_doc = flatten_document(document);

    // Convert variables to Minijinja Values
    let var: HashMap<String, Value> = variables
        .iter()
        .map(|(k, v)| (k.clone(), Value::from(v.clone())))
        .collect();

    // Build document template state if we have a document
    let doc_state = if !document.is_empty() {
        // Find the previous block if we have a block_id
        let previous = if let Some(bid) = block_id {
            flattened_doc.iter().enumerate().find_map(|(i, block)| {
                let block_obj = block.as_object()?;
                if block_obj.get("id")?.as_str()? == bid {
                    if i == 0 {
                        None
                    } else {
                        // Find the previous non-empty paragraph block
                        let mut idx = i - 1;
                        loop {
                            let prev_block = flattened_doc.get(idx)?.as_object()?;
                            if prev_block.get("type")?.as_str()? == "paragraph"
                                && prev_block.get("content")?.as_array()?.is_empty()
                            {
                                if idx == 0 {
                                    return None;
                                } else {
                                    idx -= 1;
                                }
                            } else {
                                return Some(flattened_doc.get(idx)?.clone());
                            }
                        }
                    }
                } else {
                    None
                }
            })
        } else {
            None
        };

        // Build named blocks map
        let named = flattened_doc
            .iter()
            .filter_map(|block| {
                let name = block
                    .get("props")?
                    .as_object()?
                    .get("name")?
                    .as_str()?
                    .to_string();
                Some((name, serialized_block_to_state(block.clone())))
            })
            .collect();

        Some(DocumentTemplateState {
            first: serialized_block_to_state(document.first().ok_or("Document is empty")?.clone()),
            last: serialized_block_to_state(document.last().ok_or("Document is empty")?.clone()),
            content: flattened_doc
                .iter()
                .map(|b| serialized_block_to_state(b.clone()))
                .collect(),
            named,
            previous: Some(serialized_block_to_state(
                previous.unwrap_or_else(|| serde_json::Value::Null),
            )),
        })
    } else {
        None
    };

    let template_state = TemplateState {
        doc: doc_state,
        var,
    };

    let mut env = Environment::new();
    env.set_trim_blocks(true);

    let rendered = env
        .render_str(source, template_state)
        .map_err(|e| e.to_string())?;

    Ok(rendered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_with_context_basic() {
        let mut variables = HashMap::new();
        variables.insert("name".to_string(), "World".to_string());

        let result = template_with_context("Hello {{ var.name }}!", &variables, &[], None).unwrap();

        assert_eq!(result, "Hello World!");
    }

    #[test]
    fn test_template_with_context_multiple_vars() {
        let mut variables = HashMap::new();
        variables.insert("first".to_string(), "Hello".to_string());
        variables.insert("second".to_string(), "World".to_string());

        let result =
            template_with_context("{{ var.first }} {{ var.second }}!", &variables, &[], None)
                .unwrap();

        assert_eq!(result, "Hello World!");
    }

    #[test]
    fn test_template_with_missing_var() {
        let variables = HashMap::new();

        let result = template_with_context(
            "Hello {{ var.missing | default('Default') }}!",
            &variables,
            &[],
            None,
        )
        .unwrap();

        assert_eq!(result, "Hello Default!");
    }

    #[test]
    fn test_template_with_document_context() {
        let mut variables = HashMap::new();
        variables.insert("test_var".to_string(), "test_value".to_string());

        let doc = vec![serde_json::json!({
            "id": "block1",
            "type": "paragraph",
            "props": { "name": "first_block" },
            "content": [{"type": "text", "text": "First block content"}]
        })];

        let result = template_with_context(
            "Variable: {{ var.test_var }}",
            &variables,
            &doc,
            Some("block2"),
        )
        .unwrap();

        assert_eq!(result, "Variable: test_value");
    }

    #[test]
    fn test_shellquote_filter() {
        use minijinja::Environment;

        let mut env = Environment::new();
        env.add_filter("shellquote", |value: String| -> String {
            format!("'{}'", value.replace('\'', "'\\''"))
        });

        // Test simple string
        let result = env.render_str(
            "{{ text | shellquote }}",
            minijinja::context! { text => "hello" },
        );
        assert_eq!(result.unwrap(), "'hello'");

        // Test string with single quotes
        let result = env.render_str(
            "{{ text | shellquote }}",
            minijinja::context! { text => "it's working" },
        );
        assert_eq!(result.unwrap(), "'it'\\''s working'");

        // Test string with double quotes
        let result = env.render_str(
            "{{ text | shellquote }}",
            minijinja::context! { text => "say \"hello\"" },
        );
        assert_eq!(result.unwrap(), "'say \"hello\"'");

        // Test string with special shell characters
        let result = env.render_str(
            "{{ text | shellquote }}",
            minijinja::context! { text => "$PATH and `whoami`" },
        );
        assert_eq!(result.unwrap(), "'$PATH and `whoami`'");

        // Test empty string
        let result = env.render_str(
            "{{ text | shellquote }}",
            minijinja::context! { text => "" },
        );
        assert_eq!(result.unwrap(), "''");
    }

    #[test]
    fn test_workspace_root_template() {
        use super::{Environment, TemplateState, WorkspaceTemplateState};
        use std::collections::HashMap;

        let workspace_state = WorkspaceTemplateState {
            root: String::from("/Users/test/workspace"),
        };

        let template_state = TemplateState {
            doc: None,
            var: HashMap::new(),
            workspace: workspace_state,
        };

        let env = Environment::new();
        let result = env
            .render_str("{{ workspace.root }}/src", template_state)
            .unwrap();
        assert_eq!(result, "/Users/test/workspace/src");

        // Test empty workspace root (online workspace)
        let workspace_state = WorkspaceTemplateState {
            root: String::from(""),
        };

        let template_state = TemplateState {
            doc: None,
            var: HashMap::new(),
            workspace: workspace_state,
        };

        let result = env
            .render_str("{{ workspace.root }}", template_state)
            .unwrap();
        assert_eq!(result, "");
    }
}
