use uuid::Uuid;

use crate::{run::pty::remove_pty, state::AtuinState};

#[tauri::command]
pub async fn runbook_kill_all_ptys<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AtuinState>,
    runbook: Uuid,
) -> Result<(), String> {
    let pty_store = state.pty_store();
    let ptys = pty_store
        .list_pty_for_runbook(runbook)
        .await
        .map_err(|e| e.to_string())?;

    for pty in ptys {
        remove_pty(app.clone(), pty.pid, pty_store.clone())
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}
