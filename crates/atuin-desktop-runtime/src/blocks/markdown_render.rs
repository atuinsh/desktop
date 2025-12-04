use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::blocks::{Block, BlockBehavior, FromDocument};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownRender {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into), default = String::new())]
    pub variable_name: String,
}

impl FromDocument for MarkdownRender {
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

        let variable_name = props
            .get("variableName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Ok(MarkdownRender::builder()
            .id(id)
            .variable_name(variable_name)
            .build())
    }
}

impl BlockBehavior for MarkdownRender {
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::MarkdownRender(self)
    }
}
