use minijinja::{
    value::{Enumerator, Object},
    Value,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};

/// Flatten a document to include nested blocks (like those in ToggleHeading children)
/// This creates a linear execution order regardless of UI nesting structure
pub fn flatten_document(doc: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut flattened = Vec::with_capacity(doc.len());
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyTemplateState {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunbookTemplateState {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceTemplateState {
    /// The root path of the workspace containing atuin.toml
    /// None for online workspaces (no concept of root)
    pub root: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtuinTemplateState {
    pub runbook: RunbookTemplateState,
    pub workspace: WorkspaceTemplateState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockTemplateState {
    /// Name it something that's not just type lol
    pub block_type: String,
    pub content: String,
    pub props: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentTemplateState {
    pub first: BlockTemplateState,
    pub last: BlockTemplateState,
    pub content: Vec<BlockTemplateState>,
    pub named: HashMap<String, BlockTemplateState>,

    /// The block previous to the current block
    /// This is, of course, contextual to what is presently executing - and
    /// only really makes sense if we're running a template from a Pty.
    /// We can use the pty map to lookup the metadata for the pty, and from there figure
    /// out the block ID
    pub previous: Option<BlockTemplateState>,
}

impl DocumentTemplateState {
    pub fn new(doc: &[serde_json::Value], active_block_id: Option<&str>) -> Option<Self> {
        if doc.is_empty() {
            return None;
        }

        let flattened_doc = flatten_document(doc);

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

        let first = serialized_block_to_state(flattened_doc.first().unwrap());
        let last = serialized_block_to_state(flattened_doc.last().unwrap());
        let content = flattened_doc
            .iter()
            .map(serialized_block_to_state)
            .collect();

        let previous = if let Some(active_block_id) = active_block_id {
            flattened_doc
                .iter()
                .position(|block| block.get("id").unwrap().as_str().unwrap() == active_block_id)
                .and_then(|active_index| flattened_doc.get(active_index - 1))
                .map(serialized_block_to_state)
        } else {
            None
        };

        Some(Self {
            first,
            last,
            content,
            named,
            previous,
        })
    }
}

impl Object for DocumentTemplateState {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        match key.as_str()? {
            "first" => Some(Value::from_serialize(&self.first)),
            "last" => Some(Value::from_serialize(&self.last)),
            "content" => Some(Value::from_serialize(&self.content)),
            "named" => Some(Value::from_serialize(&self.named)),
            "previous" => self.previous.clone().map(|p| Value::from_serialize(&p)),
            _ => None,
        }
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Str(&["first", "last", "content", "named", "previous"])
    }
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

pub fn serialized_block_to_state(block: &serde_json::Value) -> BlockTemplateState {
    let block = match block.as_object() {
        Some(obj) => obj,
        None => {
            // Return a default block state for non-object values
            return BlockTemplateState {
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

    BlockTemplateState {
        block_type: block_type.to_string(),
        props,
        content: content.to_string(),
    }
}
