use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

use super::FromDocument;

#[derive(Debug, Serialize, Deserialize, Clone, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Clickhouse {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub name: String,

    #[builder(setter(into))]
    pub query: String,

    #[builder(setter(into))]
    pub uri: String,

    #[builder(default = 0)]
    pub auto_refresh: i32,
}

impl FromDocument for Clickhouse {
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

        let clickhouse = Clickhouse::builder()
            .id(id)
            .name(
                props
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("ClickHouse Query")
                    .to_string(),
            )
            .query(
                props
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            )
            .uri(
                props
                    .get("uri")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            )
            .auto_refresh(
                props
                    .get("autoRefresh")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as i32,
            )
            .build();

        Ok(clickhouse)
    }
}
