use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::runtime::blocks::document::block_context::ResolvedContext;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum DocumentBridgeMessage {
    BlockContextUpdate {
        #[serde(rename = "blockId")]
        block_id: Uuid,
        context: ResolvedContext,
    },
}
