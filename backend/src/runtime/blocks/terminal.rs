use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

use super::FromDocument;

#[derive(Debug, Serialize, Deserialize, Clone, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Terminal {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub name: String,

    #[builder(setter(into))]
    pub code: String,

    #[builder(default = true)]
    pub output_visible: bool,
}

impl FromDocument for Terminal {
    fn from_document(block_data: &serde_json::Value) -> Result<Self, String> {
        let block_id = block_data
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or("Block has no id")?;

        let props = block_data
            .get("props")
            .and_then(|p| p.as_object())
            .ok_or("Block has no props")?;

        let id = Uuid::parse_str(block_id).map_err(|e| e.to_string())?;

        let terminal = Terminal::builder()
            .id(id)
            .name(
                props
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Terminal")
                    .to_string(),
            )
            .code(
                props
                    .get("code")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            )
            .output_visible(
                props
                    .get("outputVisible")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
            )
            .build();

        Ok(terminal)
    }
}
