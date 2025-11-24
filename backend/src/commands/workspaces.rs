use std::path::{Path, PathBuf};

use serde_yaml::Value;
use tauri::{ipc::Channel, path::BaseDirectory, AppHandle, Manager, State};

use crate::{
    state::AtuinState,
    workspaces::{
        fs_ops::{WorkspaceConfig, WorkspaceDirInfo},
        manager::WorkspaceEvent,
        offline_runbook::OfflineRunbook,
        workspace::WorkspaceError,
    },
};

#[tauri::command]
pub async fn copy_welcome_workspace(app: AppHandle, id: String) -> Result<String, String> {
    let welcome_path = app
        .path()
        .resolve("resources/welcome", BaseDirectory::Resource)
        .map_err(|e| e.to_string())?;

    let documents_path = app
        .path()
        .document_dir()
        .or_else(|_| app.path().home_dir()) // possible for a linux system to have no document directory
        .or_else(|_| app.path().app_config_dir()) // can't imagine we'd ever have to fallback to here, but we know the dir works
        .map_err(|_| "Failed to locate a suitable directory for welcome workspace")?;

    let mut target_path = documents_path
        .join("Atuin Runbooks")
        .join("Welcome to Atuin");
    let mut suffix: Option<u32> = None;
    while target_path.exists() {
        suffix = Some(suffix.unwrap_or(0) + 1);
        target_path = target_path
            .parent()
            .unwrap()
            .join(format!("Welcome to Atuin {}", suffix.unwrap()));
    }

    copy_dir_all(&welcome_path, &target_path).map_err(|e| e.to_string())?;

    // Overwrite the ID in the copied `atuin.toml` file with the given ID
    let config_path = target_path.join("atuin.toml");
    let mut config = WorkspaceConfig::from_file(&config_path)
        .await
        .map_err(|e| e.to_string())?;
    config.workspace.id = id;
    let contents = toml::to_string(&config).map_err(|e| e.to_string())?;
    tokio::fs::write(config_path, contents)
        .await
        .map_err(|e| e.to_string())?;

    Ok(target_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn reset_workspaces(state: State<'_, AtuinState>) -> Result<(), WorkspaceError> {
    let mut manager = state.workspaces.lock().await;
    let manager = manager.as_mut().expect("Workspaces not found in state");
    manager.reset();
    Ok(())
}

#[tauri::command]
pub async fn watch_workspace(
    path: PathBuf,
    id: String,
    channel: Channel<WorkspaceEvent>,
    state: State<'_, AtuinState>,
) -> Result<(), WorkspaceError> {
    let workspaces_clone = state.workspaces.clone();
    let mut manager = state.workspaces.lock().await;
    let manager = manager.as_mut().expect("Workspaces not found in state");
    manager
        .watch_workspace(
            path,
            &id,
            move |event: WorkspaceEvent| match channel.send(event) {
                Ok(_) => (),
                Err(e) => {
                    println!("Error sending workspace event: {e:?}");
                }
            },
            workspaces_clone,
        )
        .await
}

#[tauri::command]
pub async fn unwatch_workspace(
    id: String,
    state: State<'_, AtuinState>,
) -> Result<(), WorkspaceError> {
    let mut manager = state.workspaces.lock().await;
    let manager = manager.as_mut().expect("Workspaces not found in state");
    manager.unwatch_workspace(&id).await
}

#[tauri::command]
pub async fn create_workspace(
    path: PathBuf,
    id: String,
    name: String,
    state: State<'_, AtuinState>,
) -> Result<(), WorkspaceError> {
    let mut manager = state.workspaces.lock().await;
    let manager = manager.as_mut().expect("Workspaces not found in state");
    manager.create_workspace(&path, &id, &name).await
}

#[tauri::command]
pub async fn rename_workspace(
    id: String,
    name: String,
    state: State<'_, AtuinState>,
) -> Result<(), WorkspaceError> {
    let mut manager = state.workspaces.lock().await;
    let manager = manager.as_mut().expect("Workspaces not found in state");
    manager.rename_workspace(&id, &name).await
}

#[tauri::command]
pub async fn read_dir(
    workspace_id: String,
    state: State<'_, AtuinState>,
) -> Result<WorkspaceDirInfo, WorkspaceError> {
    let mut manager = state.workspaces.lock().await;
    let manager = manager.as_mut().expect("Workspaces not found in state");
    manager.get_dir_info(&workspace_id).await
}

#[tauri::command]
pub async fn get_workspace_id_by_folder(
    folder: String,
    state: State<'_, AtuinState>,
) -> Result<String, WorkspaceError> {
    let mut manager = state.workspaces.lock().await;
    let manager = manager.as_mut().expect("Workspaces not found in state");
    let folder = PathBuf::from(folder);
    manager.get_workspace_id_by_folder(&folder).await
}

#[tauri::command]
pub async fn save_runbook(
    workspace_id: String,
    runbook_id: String,
    name: String,
    content: Value,
    state: State<'_, AtuinState>,
) -> Result<String, WorkspaceError> {
    let mut manager = state.workspaces.lock().await;
    let manager = manager.as_mut().expect("Workspaces not found in state");
    manager
        .save_runbook(&workspace_id, &runbook_id, &name, content)
        .await
}

#[tauri::command]
pub async fn delete_runbook(
    workspace_id: String,
    runbook_id: String,
    state: State<'_, AtuinState>,
) -> Result<(), WorkspaceError> {
    let mut manager = state.workspaces.lock().await;
    let manager = manager.as_mut().expect("Workspaces not found in state");
    manager.delete_runbook(&workspace_id, &runbook_id).await
}

#[tauri::command]
pub async fn create_folder(
    workspace_id: String,
    parent_path: Option<String>,
    name: String,
    state: State<'_, AtuinState>,
) -> Result<String, WorkspaceError> {
    let mut manager = state.workspaces.lock().await;
    let manager = manager.as_mut().expect("Workspaces not found in state");
    manager
        .create_folder(&workspace_id, parent_path.as_deref(), &name)
        .await
        .map(|path| path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn rename_folder(
    workspace_id: String,
    folder_id: String,
    new_name: String,
    state: State<'_, AtuinState>,
) -> Result<(), WorkspaceError> {
    let mut manager = state.workspaces.lock().await;
    let manager = manager.as_mut().expect("Workspaces not found in state");
    manager
        .rename_folder(&workspace_id, &folder_id, &new_name)
        .await
}

#[tauri::command]
pub async fn delete_folder(
    workspace_id: String,
    folder_id: String,
    state: State<'_, AtuinState>,
) -> Result<(), WorkspaceError> {
    let mut manager = state.workspaces.lock().await;
    let manager = manager.as_mut().expect("Workspace not found in state");
    manager.delete_folder(&workspace_id, &folder_id).await
}

#[tauri::command]
pub async fn move_items(
    workspace_id: String,
    item_ids: Vec<String>,
    new_parent: Option<String>,
    state: State<'_, AtuinState>,
) -> Result<(), WorkspaceError> {
    let mut manager = state.workspaces.lock().await;
    let manager = manager.as_mut().expect("Workspace not found in state");
    manager
        .move_items(&workspace_id, &item_ids, new_parent.as_deref())
        .await
}

#[tauri::command]
pub async fn create_runbook(
    workspace_id: String,
    parent_folder_id: Option<String>,
    name: String,
    content: Value,
    forked_from: Option<String>,
    state: State<'_, AtuinState>,
) -> Result<String, WorkspaceError> {
    let mut manager = state.workspaces.lock().await;
    let manager = manager.as_mut().expect("Workspace not found in state");
    manager
        .create_runbook(
            &workspace_id,
            parent_folder_id.as_deref(),
            &name,
            &content,
            forked_from.as_deref(),
        )
        .await
}

#[tauri::command]
pub async fn get_runbook(
    runbook_id: String,
    state: State<'_, AtuinState>,
) -> Result<OfflineRunbook, WorkspaceError> {
    let mut manager = state.workspaces.lock().await;
    let manager = manager.as_mut().expect("Workspace not found in state");
    manager.get_runbook(&runbook_id).await
}

#[tauri::command]
pub async fn move_items_between_workspaces(
    item_ids: Vec<String>,
    old_workspace_id: String,
    new_workspace_id: String,
    new_parent_folder_id: Option<String>,
    state: State<'_, AtuinState>,
) -> Result<(), WorkspaceError> {
    let mut manager = state.workspaces.lock().await;
    let manager = manager.as_mut().expect("Workspace not found in state");
    manager
        .move_items_between_workspaces(
            &item_ids,
            &old_workspace_id,
            &new_workspace_id,
            new_parent_folder_id.as_deref(),
        )
        .await
}

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    std::fs::create_dir_all(&dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}
