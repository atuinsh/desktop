use minijinja::{Environment, Value};
use std::collections::HashMap;

use atuin_desktop_templates::{
    flatten_document, serialized_block_to_state, DocumentTemplateState, TemplateState,
    WorkspaceTemplateState,
};

use crate::state::AtuinState;

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
                        return Some(flattened_doc.get(i).unwrap());
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

            name.map(|name| (name, serialized_block_to_state(block)))
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
            first: serialized_block_to_state(doc.first().unwrap()),
            last: serialized_block_to_state(doc.last().unwrap()),
            content: doc.iter().map(serialized_block_to_state).collect(),
            named,
        })
    } else {
        None
    };

    let template_state = TemplateState {
        doc: doc_state,
        var,
        workspace: WorkspaceTemplateState {
            root: Some(workspace_root),
        },
    };

    let source = env
        .render_str(source.as_str(), template_state)
        .map_err(|e| e.to_string())?;

    Ok(source)
}

/// Template a string with the given variables and document context
/// This is a simplified version of template_str that doesn't require Tauri state
#[allow(dead_code)]
pub fn template_with_context(
    source: &str,
    variables: &HashMap<String, String>,
    document: &[serde_json::Value],
    block_id: Option<&str>,
    workspace_root: Option<String>,
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
                Some((name, serialized_block_to_state(block)))
            })
            .collect();

        Some(DocumentTemplateState {
            first: serialized_block_to_state(document.first().ok_or("Document is empty")?),
            last: serialized_block_to_state(document.last().ok_or("Document is empty")?),
            content: flattened_doc
                .iter()
                .map(serialized_block_to_state)
                .collect(),
            named,
            previous: Some(serialized_block_to_state(
                &previous.unwrap_or_else(|| serde_json::Value::Null),
            )),
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

        let result =
            template_with_context("Hello {{ var.name }}!", &variables, &[], None, None).unwrap();

        assert_eq!(result, "Hello World!");
    }

    #[test]
    fn test_template_with_context_multiple_vars() {
        let mut variables = HashMap::new();
        variables.insert("first".to_string(), "Hello".to_string());
        variables.insert("second".to_string(), "World".to_string());

        let result = template_with_context(
            "{{ var.first }} {{ var.second }}!",
            &variables,
            &[],
            None,
            None,
        )
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
            None,
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
            root: Some(String::from("/Users/test/workspace")),
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
            root: Some(String::from("")),
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
