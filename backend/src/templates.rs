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
pub struct AtuinTemplateState {
    pub runbook: RunbookTemplateState,
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

            _ => None,
        }
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Str(&["var", "doc"])
    }
}

fn serialized_block_to_state(block: serde_json::Value) -> BlockState {
    let block = block.as_object().unwrap();
    let block_type = block.get("type").unwrap().as_str().unwrap();

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
        .unwrap()
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
        .collect();

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
) -> Result<String, String> {
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
    };

    let source = env
        .render_str(source.as_str(), template_state)
        .map_err(|e| e.to_string())?;

    Ok(source)
}

#[tauri::command]
pub async fn get_dependent_variables(source: String) -> Result<Vec<String>, String> {
    let mut env = Environment::new();
    env.set_trim_blocks(true);

    let template = env
        .template_from_str(source.as_str())
        .map_err(|e| e.to_string())?;
    let dependent = template
        .undeclared_variables(true)
        .into_iter()
        .collect::<Vec<String>>();

    Ok(dependent)
}
