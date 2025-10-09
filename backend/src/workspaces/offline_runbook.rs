use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(TS, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "RustOfflineRunbook")]
pub struct OfflineRunbook {
    #[serde(flatten)]
    pub file: OfflineRunbookFile,
    pub workspace_id: String,
}

#[derive(TS, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "RustOfflineRunbookFile")]
pub struct OfflineRunbookFile {
    pub content_hash: String,
    #[ts(type = "{ secs_since_epoch: number, nanos_since_epoch: number } | null")]
    pub created: Option<SystemTime>,
    #[ts(type = "{ secs_since_epoch: number, nanos_since_epoch: number } | null")]
    pub updated: Option<SystemTime>,
    #[serde(flatten)]
    pub internal: OfflineRunbookFileInternal,
}

impl OfflineRunbookFile {
    pub fn new(
        internal: OfflineRunbookFileInternal,
        content_hash: String,
        created: Option<SystemTime>,
        updated: Option<SystemTime>,
    ) -> Self {
        Self {
            content_hash,
            created,
            updated,
            internal,
        }
    }
}

/// This is the struct that gets deserialized from the .atrb file
#[derive(TS, Serialize, Deserialize)]
pub struct OfflineRunbookFileInternal {
    pub id: String,
    pub name: String,
    pub version: u64,
    #[ts(type = "any[]")]
    pub content: Value,
}

impl OfflineRunbook {
    pub fn new(file: OfflineRunbookFile, workspace_id: String) -> Self {
        Self { file, workspace_id }
    }
}
