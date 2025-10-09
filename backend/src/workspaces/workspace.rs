use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use atuin_common::utils::uuid_v7;
use json_digest::digest_data;
use notify_debouncer_full::{notify::RecommendedWatcher, Debouncer, RecommendedCache};
use serde::Serialize;
use serde_yaml::Value;
use ts_rs::TS;

use crate::workspaces::{
    fs_ops::{find_unique_path, FsOpsHandle, WorkspaceDirInfo},
    hash_history::HashHistory,
    manager::OnEvent,
    offline_runbook::OfflineRunbook,
    state::WorkspaceState,
};

#[derive(thiserror::Error, TS, Debug, Clone, Serialize, PartialEq, Eq, Hash)]
#[serde(tag = "type", content = "data")]
#[ts(export, tag = "type", content = "data")]
pub enum WorkspaceError {
    #[error("Failed to process workspace at {path}: {message}")]
    WorkspaceReadError { path: PathBuf, message: String },
    #[error("Failed to create workspace {workspace_id}: {message}")]
    WorkspaceCreateError {
        workspace_id: String,
        message: String,
    },
    #[error("Workspace {workspace_id} not watched")]
    WorkspaceNotWatched { workspace_id: String },
    #[error("Workspace {workspace_id} already watched")]
    WorkspaceAlreadyWatched { workspace_id: String },
    #[error("Failed to save runbook {runbook_id}: {message}")]
    RunbookSaveError { runbook_id: String, message: String },
    #[error("Failed to delete runbook {runbook_id}: {message}")]
    RunbookDeleteError { runbook_id: String, message: String },
    #[error("Failed to rename workspace {workspace_id}: {message}")]
    WorkspaceRenameError {
        workspace_id: String,
        message: String,
    },
    #[error("Failed to watch workspace {workspace_id}: {message}")]
    WatchError {
        workspace_id: String,
        message: String,
    },
    #[error("Failed to create folder {name} in {workspace_id}: {message}")]
    FolderCreateError {
        workspace_id: String,
        name: String,
        message: String,
    },
    #[error("Failed to rename folder {folder_id}: {message}")]
    FolderRenameError {
        workspace_id: String,
        folder_id: String,
        message: String,
    },
    #[error("Failed to delete folder {folder_id}: {message}")]
    FolderDeleteError {
        workspace_id: String,
        folder_id: String,
        message: String,
    },
    #[error("Failed to move items: {message}")]
    ItemMoveError {
        workspace_id: String,
        item_ids: Vec<String>,
        new_parent: PathBuf,
        message: String,
    },

    #[error("{message}")]
    GenericWorkspaceError { message: String },
}

pub struct Workspace {
    pub id: String,
    pub state: Result<WorkspaceState, WorkspaceError>,
    pub histories: HashMap<String, HashHistory>,
    pub path: PathBuf,
    pub _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
    pub fs_ops: FsOpsHandle,
    pub on_event: Arc<dyn OnEvent>,
}

impl Workspace {
    pub async fn rename(&mut self, name: &str) -> Result<(), WorkspaceError> {
        let path = self.path.clone();
        self.fs_ops.rename_workspace(path, name).await.map_err(|e| {
            WorkspaceError::WorkspaceRenameError {
                workspace_id: self.id.clone(),
                message: e.to_string(),
            }
        })
    }

    pub async fn create_folder(
        &mut self,
        parent_path: Option<&Path>,
        name: &str,
    ) -> Result<PathBuf, WorkspaceError> {
        self.fs_ops
            .create_folder(&parent_path.unwrap_or(&self.path), &name)
            .await
            .map_err(|e| WorkspaceError::FolderCreateError {
                workspace_id: self.id.clone(),
                name: name.to_string(),
                message: e.to_string(),
            })
    }

    pub async fn rename_folder(
        &mut self,
        folder_id: &str,
        new_name: &str,
    ) -> Result<(), WorkspaceError> {
        let from = PathBuf::from(folder_id);
        if let Some(parent) = from.parent() {
            let to = parent.join(new_name);
            self.fs_ops.rename_folder(&from, &to).await.map_err(|e| {
                WorkspaceError::FolderRenameError {
                    workspace_id: self.id.clone(),
                    folder_id: folder_id.to_string(),
                    message: e.to_string(),
                }
            })
        } else {
            Err(WorkspaceError::FolderRenameError {
                workspace_id: self.id.clone(),
                folder_id: folder_id.to_string(),
                message: "Folder has no parent".to_string(),
            })
        }
    }

    pub async fn delete_folder(&mut self, folder_id: &str) -> Result<(), WorkspaceError> {
        self.fs_ops
            .trash_folder(&PathBuf::from(folder_id))
            .await
            .map_err(|e| WorkspaceError::FolderDeleteError {
                workspace_id: self.id.clone(),
                folder_id: folder_id.to_string(),
                message: e.to_string(),
            })
    }

    pub async fn move_items(
        &mut self,
        item_ids: &[String],
        new_parent: Option<&str>,
    ) -> Result<(), WorkspaceError> {
        if let Err(e) = &self.state {
            return Err(WorkspaceError::GenericWorkspaceError {
                message: format!("Bad workspace state: {e:?}"),
            });
        }

        let new_parent = new_parent.map(PathBuf::from).unwrap_or(self.path.clone());

        let top_items = self
            .state
            .as_ref()
            .unwrap()
            .calculate_toplevel_paths(item_ids);

        self.fs_ops
            .move_items(&top_items, &new_parent)
            .await
            .map_err(|e| WorkspaceError::ItemMoveError {
                workspace_id: self.id.clone(),
                item_ids: item_ids.to_vec(),
                new_parent,
                message: e.to_string(),
            })?;

        Ok(())
    }

    pub async fn create_runbook(
        &mut self,
        parent_folder_id: Option<&str>,
        name: &str,
        content: &Value,
    ) -> Result<String, WorkspaceError> {
        let id = uuid_v7();

        let parent_folder = parent_folder_id.map(PathBuf::from);

        self.save_runbook(&id.to_string(), name, content, parent_folder.as_ref())
            .await?;

        Ok(id.to_string())
    }

    /// Saves a runbook to the workspace if its hash has changed
    /// and returns the hash of the saved runbook.
    pub async fn save_runbook(
        &mut self,
        runbook_id: &str,
        name: &str,
        content: &Value,
        parent_folder: Option<impl AsRef<Path>>,
    ) -> Result<String, WorkspaceError> {
        if let Err(e) = &self.state {
            return Err(WorkspaceError::RunbookSaveError {
                runbook_id: runbook_id.to_string(),
                message: format!("Bad workspace state: {e:?}"),
            });
        }

        let current_path = parent_folder
            .map(|p| p.as_ref().join(runbook_id))
            .or_else(|| {
                self.state
                    .as_ref()
                    .ok()
                    .and_then(|s| s.runbooks.get(runbook_id).map(|r| r.path.clone()))
            });

        let new_filename = name_to_filename(name);
        let new_filename = format!("{new_filename}.atrb");

        let new_path = current_path
            .as_ref()
            .map(|p| p.with_file_name(new_filename.clone()))
            .unwrap_or_else(|| self.path.join(new_filename));

        let paths_match = current_path
            .as_ref()
            .map(|p| p == &new_path)
            .unwrap_or(false);

        let mut map = serde_yaml::Mapping::new();
        map.insert(
            Value::String("id".to_string()),
            Value::String(runbook_id.to_string()),
        );
        map.insert(
            Value::String("name".to_string()),
            Value::String(name.to_string()),
        );
        map.insert(
            Value::String("version".to_string()),
            Value::Number(1.into()),
        );
        map.insert(
            Value::String("content".to_string()),
            serde_yaml::to_value(content).unwrap(),
        );
        let map = Value::Mapping(map);

        // Create a canonical hash by converting to JSON first, then using json-digest
        // This ensures consistent hashing regardless of YAML serialization order
        let json_value: serde_json::Value =
            serde_yaml::from_value(map.clone()).map_err(|e| WorkspaceError::RunbookSaveError {
                runbook_id: runbook_id.to_string(),
                message: format!("Failed to convert YAML to JSON for hashing: {e}"),
            })?;

        let digest = digest_data(&json_value).map_err(|e| WorkspaceError::RunbookSaveError {
            runbook_id: runbook_id.to_string(),
            message: format!("Failed to hash runbook content: {e}"),
        })?;

        match Ok::<String, WorkspaceError>(digest.clone()) {
            Ok(digest) => {
                if let Some(history) = self.histories.get(runbook_id) {
                    // If the hash hasn't changed, we don't need to save the runbook,
                    // UNLESS the name has changed, in which case FsOps will handle the rename.
                    if history.latest() == Some(&digest) && paths_match {
                        return Ok(digest);
                    }
                }

                self.fs_ops
                    .save_runbook(runbook_id, current_path.as_ref(), &new_path, map.clone())
                    .await
                    .map_err(|e| WorkspaceError::RunbookSaveError {
                        runbook_id: runbook_id.to_string(),
                        message: e.to_string(),
                    })?;

                self.histories
                    .entry(runbook_id.to_string())
                    .or_insert_with(|| HashHistory::new(5))
                    .push(digest.clone());

                Ok(digest)
            }
            Err(e) => Err(WorkspaceError::RunbookSaveError {
                runbook_id: runbook_id.to_string(),
                message: e.to_string(),
            }),
        }
    }

    pub async fn delete_runbook(&mut self, runbook_id: &str) -> Result<(), WorkspaceError> {
        let runbook = self
            .state
            .as_ref()
            .ok()
            .and_then(|s| s.runbooks.get(runbook_id))
            .ok_or(WorkspaceError::GenericWorkspaceError {
                message: format!("Runbook {runbook_id} not found"),
            })?;

        self.fs_ops
            .delete_runbook(&runbook.path)
            .await
            .map_err(|e| WorkspaceError::RunbookDeleteError {
                runbook_id: runbook_id.to_string(),
                message: e.to_string(),
            })
    }

    pub async fn get_runbook(&mut self, id: &str) -> Result<OfflineRunbook, WorkspaceError> {
        let path = if let Ok(state) = &self.state {
            if let Some(runbook) = state.runbooks.get(id) {
                Ok(runbook.path.clone())
            } else {
                Err(WorkspaceError::GenericWorkspaceError {
                    message: format!("Runbook {id} not found"),
                })
            }
        } else {
            Err(WorkspaceError::GenericWorkspaceError {
                message: "Workspace state is not loaded".to_string(),
            })
        }?;

        let runbook = self
            .fs_ops
            .get_runbook(&path)
            .await
            .map(|file| OfflineRunbook::new(file, self.id.clone()))
            .map_err(|e| WorkspaceError::GenericWorkspaceError {
                message: e.to_string(),
            })?;

        self.histories
            .entry(id.to_string())
            .or_insert_with(|| HashHistory::new(5))
            .push(runbook.file.content_hash.clone());

        Ok(runbook)
    }

    pub async fn get_dir_info(&self) -> Result<WorkspaceDirInfo, WorkspaceError> {
        self.fs_ops
            .get_dir_info(self.path.clone())
            .await
            .map_err(|e| WorkspaceError::WorkspaceReadError {
                path: self.path.clone(),
                message: e.to_string(),
            })
    }

    pub async fn rescan(&self) -> Result<WorkspaceState, WorkspaceError> {
        WorkspaceState::new(&self.id, &self.path)
            .await
            .map_err(|e| WorkspaceError::WorkspaceReadError {
                path: self.path.clone(),
                message: e.to_string(),
            })
    }

    pub async fn move_into_workspace(
        &mut self,
        paths_to_move: &[impl AsRef<Path>],
        previous_root: impl AsRef<Path>,
        new_parent_folder_id: Option<PathBuf>,
    ) -> Result<(), WorkspaceError> {
        let mut moved_runbooks: Vec<(PathBuf, PathBuf)> = Vec::with_capacity(paths_to_move.len());

        let mut success = true;
        for path in paths_to_move {
            let filename = path.as_ref().file_name();
            if filename.is_none() {
                success = false;
                break;
            }

            let filename = filename.unwrap();

            let path_without_root = path.as_ref().strip_prefix(previous_root.as_ref());
            if path_without_root.is_err() {
                success = false;
                break;
            }

            let mut new_path = new_parent_folder_id
                .as_ref()
                .unwrap_or(&self.path)
                .join(filename);

            if new_path.exists() {
                let new_path_result = find_unique_path(&new_path);
                if new_path_result.is_err() {
                    success = false;
                    break;
                }
                new_path = new_path_result.unwrap();
            }

            let result = tokio::fs::rename(&path, &new_path).await;
            if result.is_err() {
                success = false;
                break;
            }

            moved_runbooks.push((new_path, path.as_ref().to_path_buf()));
        }

        if !success {
            for moved in moved_runbooks {
                let _ = tokio::fs::rename(moved.0, moved.1).await;
            }

            return Err(WorkspaceError::GenericWorkspaceError {
                message: "Failed to move items".to_string(),
            });
        }

        Ok(())
    }
}

fn name_to_filename(name: &str) -> String {
    name.chars()
        .flat_map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '_' || c == '-' {
                Some(c)
            } else {
                None
            }
        })
        .collect::<String>()
        .split(' ')
        .filter(|s| !s.is_empty())
        .collect::<Vec<&str>>()
        .join(" ")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_to_filename() {
        assert_eq!(name_to_filename(" Hello World! "), "Hello World");
        assert_eq!(
            name_to_filename("Wait, is this a file runbook!?"),
            "Wait is this a file runbook"
        );
    }
}
