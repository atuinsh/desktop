use minijinja::{
    value::{Enumerator, Object},
    Value,
};
use serde::Serialize;
use std::{collections::HashMap, sync::Arc};

use crate::context::BlockExecutionOutput;

#[derive(Debug)]
struct OutputWrapper(Arc<dyn BlockExecutionOutput>);

impl Object for OutputWrapper {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        self.0.get_template_value(key.as_str()?)
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        self.0.enumerate_template_keys()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PtyTemplateState {
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunbookTemplateState {
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceTemplateState {
    /// The root path of the workspace containing atuin.toml
    /// None for online workspaces (no concept of root)
    pub root: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AtuinTemplateState {
    pub runbook: RunbookTemplateState,
    pub workspace: WorkspaceTemplateState,
}

#[derive(Debug, Clone)]
pub struct BlockTemplateState {
    /// Name it something that's not just type lol
    pub block_type: String,
    pub content: String,
    pub props: HashMap<String, String>,
    pub output: Option<Arc<dyn BlockExecutionOutput>>,
}

impl Object for BlockTemplateState {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        tracing::debug!("BlockTemplateState::get_value: {key}", key = key.as_str()?);
        match key.as_str()? {
            "block_type" => Some(Value::from_serialize(&self.block_type)),
            "content" => Some(Value::from_serialize(&self.content)),
            "props" => Some(Value::from_serialize(&self.props)),
            "output" => self
                .output
                .as_ref()
                .map(|o| Value::from_object(OutputWrapper(o.clone()))),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VecBlockTemplateState(Vec<BlockTemplateState>);

impl Object for VecBlockTemplateState {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let n = key.as_usize()?;
        self.0.get(n).cloned().map(Value::from_object)
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Seq(self.0.len())
    }
}

impl FromIterator<BlockTemplateState> for VecBlockTemplateState {
    fn from_iter<T: IntoIterator<Item = BlockTemplateState>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[derive(Debug, Clone)]
pub struct HashMapBlockTemplateState(HashMap<String, BlockTemplateState>);

impl Object for HashMapBlockTemplateState {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let k = key.as_str()?;
        self.0.get(k).cloned().map(Value::from_object)
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Values(self.0.keys().map(|k| Value::from(k.clone())).collect())
    }
}

impl FromIterator<(String, BlockTemplateState)> for HashMapBlockTemplateState {
    fn from_iter<T: IntoIterator<Item = (String, BlockTemplateState)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[derive(Debug, Clone)]
pub struct DocumentTemplateState {
    pub first: BlockTemplateState,
    pub last: BlockTemplateState,
    pub content: VecBlockTemplateState,
    pub named: HashMapBlockTemplateState,

    /// The block previous to the current block
    /// This is, of course, contextual to what is presently executing - and
    /// only really makes sense if we're running a template from a Pty.
    /// We can use the pty map to lookup the metadata for the pty, and from there figure
    /// out the block ID
    pub previous: Option<BlockTemplateState>,
}

impl DocumentTemplateState {
    pub fn new(
        flattened_doc: &[serde_json::Value],
        active_block_id: Option<&str>,
        block_outputs: HashMap<String, Option<Arc<dyn BlockExecutionOutput>>>,
    ) -> Option<Self> {
        if flattened_doc.is_empty() {
            return None;
        }

        tracing::debug!("block_outputs: {block_outputs:?}",);

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

                let mut state = serialized_block_to_state(block);
                state.output = block_outputs
                    .get(block.get("id").unwrap().as_str().unwrap())
                    .cloned()
                    .flatten();
                name.map(|name| (name, state))
            })
            .collect::<HashMap<String, BlockTemplateState>>();

        let mut first = serialized_block_to_state(flattened_doc.first().unwrap());
        first.output = block_outputs
            .get(
                flattened_doc
                    .first()
                    .unwrap()
                    .get("id")
                    .unwrap()
                    .as_str()
                    .unwrap(),
            )
            .cloned()
            .flatten();
        let mut last = serialized_block_to_state(flattened_doc.last().unwrap());
        last.output = block_outputs
            .get(
                flattened_doc
                    .last()
                    .unwrap()
                    .get("id")
                    .unwrap()
                    .as_str()
                    .unwrap(),
            )
            .cloned()
            .flatten();
        let content = flattened_doc
            .iter()
            .map(|block| {
                let mut state = serialized_block_to_state(block);
                state.output = block_outputs
                    .get(block.get("id").unwrap().as_str().unwrap())
                    .cloned()
                    .flatten();
                state
            })
            .collect::<Vec<BlockTemplateState>>();

        let previous = if let Some(active_block_id) = active_block_id {
            flattened_doc
                .iter()
                .position(|block| block.get("id").unwrap().as_str().unwrap() == active_block_id)
                .and_then(|active_index| active_index.checked_sub(1))
                .and_then(|prev_index| flattened_doc.get(prev_index))
                .map(|prev_block| {
                    let mut state = serialized_block_to_state(prev_block);
                    let prev_block_id = prev_block.get("id").unwrap().as_str().unwrap();
                    state.output = block_outputs.get(prev_block_id).cloned().flatten();
                    state
                })
        } else {
            None
        };

        Some(Self {
            first,
            last,
            content: VecBlockTemplateState(content),
            named: HashMapBlockTemplateState(named),
            previous,
        })
    }
}

impl Object for DocumentTemplateState {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        tracing::debug!(
            "DocumentTemplateState::get_value: {key}",
            key = key.as_str()?
        );
        match key.as_str()? {
            "first" => Some(Value::from_object(self.first.clone())),
            "last" => Some(Value::from_object(self.last.clone())),
            "content" => Some(Value::from_object(self.content.clone())),
            "named" => Some(Value::from_object(self.named.clone())),
            "previous" => self.previous.clone().map(Value::from_object),
            _ => None,
        }
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Str(&["first", "last", "content", "named", "previous"])
    }
}

#[derive(Debug, Clone)]
pub struct TemplateState {
    // In the case where a document is empty, we have no document state.
    pub doc: Option<DocumentTemplateState>,
    pub var: HashMap<String, Value>,
    pub workspace: WorkspaceTemplateState,
}

impl Object for TemplateState {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        tracing::debug!("TemplateState::get_value: {key}", key = key.as_str()?);
        match key.as_str()? {
            "var" => Some(Value::make_object_map(
                self.clone(),
                |this| Box::new(this.var.keys().map(Value::from)),
                |this, key| this.var.get(key.as_str()?).cloned(),
            )),

            "doc" => self.doc.as_ref().map(|doc| Value::from_object(doc.clone())),

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
                output: None,
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
        output: None,
    }
}
