use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde::Serialize;
use serde_yaml::Value as YamlValue;
use tokio::fs::File;
use ts_rs::TS;

use crate::workspaces::fs_ops::{read_dir_recursive, DirEntry, WorkspaceConfig};

#[cfg(test)]
mod test_canonical_hash;

#[derive(thiserror::Error, Debug)]
pub enum WorkspaceStateError {
    #[error("Failed to read directory: {0}")]
    DirReadError(PathBuf),
    #[error("Failed to read directory: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to parse workspace config: {0}")]
    TomlError(#[from] toml::de::Error),
    #[error("Failed to read workspace info: {0}")]
    WorkspaceReadError(#[from] crate::workspaces::fs_ops::FsOpsError),
    #[error("Invalid workspace manifest: expected ID {0}, found ID {1}")]
    InvalidWorkspaceManifest(String, String),
    #[error("Invalid ATRB file: {0}")]
    InvalidAtrbFile(PathBuf),
    #[error("Multiple files with duplicate runbook IDs: {0} and {1}")]
    DuplicateRunbook(PathBuf, PathBuf),
}

#[derive(TS, Debug, Clone, Serialize, PartialEq, Eq, Hash)]
#[ts(export)]
pub struct WorkspaceState {
    pub id: String,
    pub name: String,
    pub root: PathBuf,
    pub entries: Vec<DirEntry>,
    pub runbooks: BTreeMap<String, WorkspaceRunbook>,
}

impl WorkspaceState {
    pub async fn new(id: &str, root: impl AsRef<Path>) -> Result<Self, WorkspaceStateError> {
        let config = WorkspaceConfig::from_file(root.as_ref().join("atuin.toml")).await?;
        if config.workspace.id != id {
            return Err(WorkspaceStateError::InvalidWorkspaceManifest(
                id.to_string(),
                config.workspace.id,
            ));
        }

        let entries = read_dir_recursive(root.as_ref()).await?;
        let runbooks = get_workspace_runbooks(&entries).await?;

        Ok(Self {
            id: id.to_string(),
            name: config.workspace.name,
            root: root.as_ref().to_path_buf(),
            entries,
            runbooks,
        })
    }

    /// Calculates the top-level paths from a list of item IDs.
    pub fn calculate_toplevel_paths(&self, item_ids: &[String]) -> Vec<PathBuf> {
        let mut result = Vec::new();

        for item_id in item_ids {
            // Check if this item is a child of any other item in the list
            let is_child = item_ids.iter().any(|other_id| {
                if other_id == item_id {
                    return false;
                }

                if let Some(ref current_path) = self.get_path_for_item(item_id) {
                    if let Some(ref other_path) = self.get_path_for_item(other_id) {
                        // Check if current_path is a child of other_path
                        return current_path.starts_with(other_path) && current_path != other_path;
                    }
                }

                false
            });

            // Only add if it's not a child of any other item
            if !is_child {
                result.push(self.get_path_for_item(item_id).unwrap());
            }
        }

        result
    }

    fn get_path_for_item(&self, item_id: &str) -> Option<PathBuf> {
        if let Some(runbook) = self.runbooks.get(item_id) {
            return Some(runbook.path.clone());
        }

        let path_buf = PathBuf::from(item_id);
        if self.entries.iter().any(|entry| entry.path == path_buf) {
            Some(path_buf)
        } else {
            None
        }
    }
}

#[derive(TS, Debug, Clone, Serialize, PartialEq, Eq, Hash)]
#[ts(export)]
pub struct WorkspaceRunbook {
    pub id: String,
    pub name: String,
    #[ts(type = "number")]
    pub version: u64,
    pub path: PathBuf,
    #[ts(type = "{ secs_since_epoch: number, nanos_since_epoch: number } | null")]
    pub lastmod: Option<SystemTime>,
}

impl WorkspaceRunbook {
    pub fn new(
        id: &str,
        name: &str,
        version: u64,
        path: impl AsRef<Path>,
        lastmod: Option<SystemTime>,
    ) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            version,
            path: path.as_ref().to_path_buf(),
            lastmod,
        }
    }

    pub async fn from_file(path: impl AsRef<Path>) -> Result<Self, WorkspaceStateError> {
        let file = File::open(&path).await?;
        let stats = File::metadata(&file).await?;
        drop(file);

        let yaml_content = tokio::fs::read_to_string(&path).await?;
        let yaml_value: YamlValue = serde_yaml::from_str(&yaml_content)
            .map_err(|_| WorkspaceStateError::InvalidAtrbFile(path.as_ref().to_path_buf()))?;

        let id = yaml_value
            .get("id")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string())
            .ok_or_else(|| WorkspaceStateError::InvalidAtrbFile(path.as_ref().to_path_buf()))?;
        let name = yaml_value
            .get("name")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string())
            .ok_or_else(|| WorkspaceStateError::InvalidAtrbFile(path.as_ref().to_path_buf()))?;
        let version = yaml_value
            .get("version")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| WorkspaceStateError::InvalidAtrbFile(path.as_ref().to_path_buf()))?;

        Ok(Self::new(
            &id,
            &name,
            version,
            path.as_ref(),
            stats.modified().ok(),
        ))
    }
}

async fn get_workspace_runbooks(
    dir_entries: &[DirEntry],
) -> Result<BTreeMap<String, WorkspaceRunbook>, WorkspaceStateError> {
    let mut runbooks: BTreeMap<String, WorkspaceRunbook> = BTreeMap::new();
    for entry in dir_entries {
        let name = entry
            .path
            .file_name()
            .ok_or_else(|| WorkspaceStateError::DirReadError(entry.path.clone()))?
            .to_string_lossy()
            .to_string();

        if name.ends_with(".atrb") {
            let runbook = WorkspaceRunbook::from_file(entry.path.clone()).await?;
            if runbooks.contains_key(&runbook.id) {
                return Err(WorkspaceStateError::DuplicateRunbook(
                    entry.path.clone(),
                    runbooks.get(&runbook.id).unwrap().path.clone(),
                ));
            }

            runbooks.insert(runbook.id.clone(), runbook);
        }
    }
    Ok(runbooks)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use crate::workspaces::fs_ops::WorkspaceConfigDetails;

    use super::*;

    async fn setup() -> TempDir {
        let root = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(&root).await.unwrap();

        let config = toml::to_string(&WorkspaceConfig {
            workspace: WorkspaceConfigDetails {
                id: "test".to_string(),
                name: "Test Workspace".to_string(),
            },
        })
        .unwrap();

        let config_path = root.path().join("atuin.toml");
        tokio::fs::write(&config_path, config).await.unwrap();

        let rb1_yaml = r#"
id: rb1
name: Runbook 1
version: 1
"#;

        let rb1_path = root.path().join("rb1.atrb");
        tokio::fs::write(&rb1_path, rb1_yaml).await.unwrap();

        let rb2_yaml = r#"
id: rb2
name: Runbook 2
version: 2
"#;

        let rb2_path = root.path().join("rb2.atrb");
        tokio::fs::write(&rb2_path, rb2_yaml).await.unwrap();

        root
    }

    #[tokio::test]
    async fn test_workspace_state() {
        let root = setup().await;
        let state = WorkspaceState::new("test", root.path()).await.unwrap();

        assert_eq!(state.id, "test");

        assert_eq!(state.runbooks.len(), 2);

        let rb1 = state.runbooks.get("rb1").unwrap();
        assert_eq!(rb1.id, "rb1");
        assert_eq!(rb1.name, "Runbook 1");
        assert_eq!(rb1.version, 1);

        let rb2 = state.runbooks.get("rb2").unwrap();
        assert_eq!(rb2.id, "rb2");
        assert_eq!(rb2.name, "Runbook 2");
        assert_eq!(rb2.version, 2);
    }
}
