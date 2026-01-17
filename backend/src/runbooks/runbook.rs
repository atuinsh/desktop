// Represent a runbook as raw data

use eyre::Result;
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::{run::pty::remove_pty, state::AtuinState};

pub const CURRENT_RUNBOOK_VERSION: u32 = 0;

/// Export a runbook. Pass in the content, and the location to save the file
/// Why do this in Rust?
///
/// 1. Tauri has a bunch of filesystem limitations to not allow blanket writes to JS.
/// This totally makes sense, but for this specific case we should bypass that for writing Runbook content.
/// We also allow generic shell command execution in our frontend, so this isn't a big deal
///
/// 2. While we're not doing a tonne of validation right now, I'd like to ensure that this can be properly parsed into
/// our Runbook struct before we save it to disk.
#[tauri::command]
pub fn export_atrb(json: String, file_path: String) -> Result<(), String> {
    let mut runbook = Runbook::from_json(json).map_err(|e| e.to_string())?;
    let json = serde_json::to_string(&runbook).map_err(|e| e.to_string())?;

    // Ensure the current file format is specified
    runbook.version = CURRENT_RUNBOOK_VERSION;

    std::fs::write(file_path, json.as_bytes()).map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn delete_runbook_cleanup<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, AtuinState>,
    runbook: Uuid,
) -> Result<(), String> {
    // Cleanup all PTYs first
    // Seeing as we do not (yet) store runbook data grouped by runbook, we have to
    // iterate all of them and check the metadata. Boo.

    let ptys_to_remove: Vec<_> = {
        let ptys = state
            .pty_store()
            .list_pty_for_runbook(runbook)
            .await
            .map_err(|e| e.to_string())?;
        ptys.iter().map(|pty| pty.pid).collect()
    };

    for pty_id in ptys_to_remove {
        remove_pty(app.clone(), pty_id, state.pty_store()).await?;
    }

    Ok(())
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Runbook {
    #[serde(default)]
    pub version: u32,

    pub id: Uuid,
    pub content: String,

    // If we're parsing a runbook from a markdown file, it might not have all of this metadata.
    // When exporting, we should try and ensure there's the correct front matter. But this doesn't
    // necessarily exist in all cases.
    // Users should actually be able to import a plain old markdown file and get _something_ useful
    pub name: Option<String>,
    pub created: Option<u64>,
    pub updated: Option<u64>,
}

impl Runbook {
    pub fn from_json(json: String) -> Result<Runbook> {
        let runbook: Runbook = serde_json::from_str(json.as_str())?;
        Ok(runbook)
    }
}
