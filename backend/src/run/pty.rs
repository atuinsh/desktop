use crate::state::AtuinState;
use atuin_desktop_runtime::pty::PtyStoreHandle;
use eyre::Result;
use tauri::Manager;

async fn update_badge_count(app: &tauri::AppHandle, store: PtyStoreHandle) -> Result<()> {
    let len = store.len().await?;
    let len = if len == 0 { None } else { Some(len as i64) };

    app.webview_windows()
        .values()
        .next()
        .expect("no window found")
        .set_badge_count(len)?;

    Ok(())
}

#[tauri::command]
pub(crate) async fn pty_write(
    pid: uuid::Uuid,
    data: String,
    state: tauri::State<'_, AtuinState>,
) -> Result<(), String> {
    let bytes = data.as_bytes().to_vec();
    state
        .pty_store()
        .write_pty(pid, bytes.into())
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub(crate) async fn pty_resize(
    pid: uuid::Uuid,
    rows: u16,
    cols: u16,
    state: tauri::State<'_, AtuinState>,
) -> Result<(), String> {
    state
        .pty_store()
        .resize_pty(pid, rows, cols)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub(crate) async fn remove_pty(
    app: tauri::AppHandle,
    pid: uuid::Uuid,
    store: PtyStoreHandle,
) -> Result<(), String> {
    store.remove_pty(pid).await.map_err(|e| e.to_string())?;

    update_badge_count(&app, store.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}
