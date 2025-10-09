use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use json_digest;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use tokio::sync::{
    mpsc::{channel, error::SendError, Receiver, Sender},
    oneshot,
};
use trash::TrashContext;
use ts_rs::TS;

use crate::{
    run_async_command,
    workspaces::offline_runbook::{OfflineRunbookFile, OfflineRunbookFileInternal},
};

use crate::workspaces::manager::{create_ignore_matcher, should_ignore_path};

#[derive(thiserror::Error, Debug)]
pub enum FsOpsError {
    #[error("Failed to send instruction to filesystem operations actor")]
    SendError,
    #[error("File missing: {0}")]
    FileMissingError(String),
    #[error("IO Operation failed: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to serialize workspace config: {0}")]
    WorkspaceSerializeError(#[from] toml::ser::Error),
    #[error("Failed to serialize runbook: {0}")]
    RunbookSerializeError(#[from] serde_yaml::Error),
    #[error("Failed to deserialize workspace config: {0}")]
    DeserializeError(#[from] toml::de::Error),
    #[error("Failed to trash folder: {0}")]
    TrashError(#[from] trash::Error),
    #[error("Failed to digest runbook content: {0}")]
    DigestError(String),
}

// Manual impl for `SendError` since using the `#[from]` attribute
// causes any `Result` that may contain a `FsOpsError` to be rather large
// due to the size of the `FsOpsInstruction` enum
impl From<SendError<FsOpsInstruction>> for FsOpsError {
    fn from(_error: SendError<FsOpsInstruction>) -> Self {
        FsOpsError::SendError
    }
}

pub type Reply<T> = oneshot::Sender<Result<T, FsOpsError>>;

pub enum FsOpsInstruction {
    RenameWorkspace(PathBuf, String, Reply<()>),
    GetDirectoryInformation(PathBuf, Reply<WorkspaceDirInfo>),
    SaveRunbook(String, Option<PathBuf>, PathBuf, Value, Reply<()>),
    DeleteRunbook(PathBuf, Reply<()>),
    GetRunbook(PathBuf, Reply<OfflineRunbookFile>),
    CreateFolder(PathBuf, PathBuf, Reply<PathBuf>),
    RenameFolder(PathBuf, PathBuf, Reply<()>),
    TrashFolder(PathBuf, Reply<()>),
    MoveItems(Vec<PathBuf>, PathBuf, Reply<()>),
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceDirInfo {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub contents: Vec<DirEntry>,
}

#[derive(TS, Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[ts(export)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub path: PathBuf,
    #[ts(type = "{ secs_since_epoch: number, nanos_since_epoch: number } | null")]
    pub lastmod: Option<SystemTime>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    // Nested so that there are no top-level keys in the config file
    pub workspace: WorkspaceConfigDetails,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceConfigDetails {
    pub id: String,
    pub name: String,
}

impl WorkspaceConfig {
    pub fn new(id: String, name: String) -> Self {
        Self {
            workspace: WorkspaceConfigDetails { id, name },
        }
    }

    pub async fn from_file(path: impl AsRef<Path>) -> Result<Self, FsOpsError> {
        let config_text = tokio::fs::read_to_string(path).await?;
        let config: WorkspaceConfig = toml::from_str(&config_text)?;
        Ok(config)
    }
}

#[derive(Clone)]
pub struct FsOpsHandle {
    tx: Sender<FsOpsInstruction>,
}

impl FsOpsHandle {
    pub fn new() -> Self {
        let (tx, rx) = channel(16);

        tauri::async_runtime::spawn(async move {
            let mut actor = FsOps::new();
            actor.run(rx).await;
        });

        Self { tx }
    }

    pub async fn get_dir_info(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<WorkspaceDirInfo, FsOpsError> {
        let (sender, receiver) = oneshot::channel();
        self.tx
            .send(FsOpsInstruction::GetDirectoryInformation(
                path.as_ref().to_path_buf(),
                sender,
            ))
            .await?;
        receiver.await.unwrap()
    }

    pub async fn rename_workspace(
        &self,
        path: impl AsRef<Path>,
        name: &str,
    ) -> Result<(), FsOpsError> {
        let (sender, receiver) = oneshot::channel();
        self.tx
            .send(FsOpsInstruction::RenameWorkspace(
                path.as_ref().to_path_buf(),
                name.to_string(),
                sender,
            ))
            .await?;
        receiver.await.unwrap()
    }

    pub async fn save_runbook(
        &self,
        runbook_id: &str,
        old_path: Option<impl AsRef<Path>>,
        new_path: impl AsRef<Path>,
        content: Value,
    ) -> Result<(), FsOpsError> {
        let (sender, receiver) = oneshot::channel();

        self.tx
            .send(FsOpsInstruction::SaveRunbook(
                runbook_id.to_string(),
                old_path.map(|p| p.as_ref().to_path_buf()),
                new_path.as_ref().to_path_buf(),
                content,
                sender,
            ))
            .await?;
        receiver.await.unwrap()
    }

    pub async fn delete_runbook(&self, path: impl AsRef<Path>) -> Result<(), FsOpsError> {
        let (sender, receiver) = oneshot::channel();
        self.tx
            .send(FsOpsInstruction::DeleteRunbook(
                path.as_ref().to_path_buf(),
                sender,
            ))
            .await?;
        receiver.await.unwrap()
    }

    pub async fn get_runbook(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<OfflineRunbookFile, FsOpsError> {
        let (sender, receiver) = oneshot::channel();
        self.tx
            .send(FsOpsInstruction::GetRunbook(
                path.as_ref().to_path_buf(),
                sender,
            ))
            .await?;
        receiver.await.unwrap()
    }

    pub async fn create_folder(
        &self,
        parent_path: impl AsRef<Path>,
        name: impl AsRef<Path>,
    ) -> Result<PathBuf, FsOpsError> {
        let (sender, receiver) = oneshot::channel();

        self.tx
            .send(FsOpsInstruction::CreateFolder(
                parent_path.as_ref().to_path_buf(),
                name.as_ref().to_path_buf(),
                sender,
            ))
            .await?;
        receiver.await.unwrap()
    }

    pub async fn rename_folder(
        &self,
        from: impl AsRef<Path>,
        to: impl AsRef<Path>,
    ) -> Result<(), FsOpsError> {
        let (sender, receiver) = oneshot::channel();

        self.tx
            .send(FsOpsInstruction::RenameFolder(
                from.as_ref().to_path_buf(),
                to.as_ref().to_path_buf(),
                sender,
            ))
            .await?;
        receiver.await.unwrap()
    }

    pub async fn trash_folder(&self, path: impl AsRef<Path>) -> Result<(), FsOpsError> {
        let (sender, receiver) = oneshot::channel();
        self.tx
            .send(FsOpsInstruction::TrashFolder(
                path.as_ref().to_path_buf(),
                sender,
            ))
            .await?;
        receiver.await.unwrap()
    }

    pub async fn move_items(&self, items: &[PathBuf], new_parent: &Path) -> Result<(), FsOpsError> {
        let (sender, receiver) = oneshot::channel();

        self.tx
            .send(FsOpsInstruction::MoveItems(
                items.to_vec(),
                new_parent.to_path_buf(),
                sender,
            ))
            .await?;
        receiver.await.unwrap()
    }

    pub async fn shutdown(&self) -> Result<(), FsOpsError> {
        self.tx.send(FsOpsInstruction::Shutdown).await?;
        Ok(())
    }
}

impl Drop for FsOpsHandle {
    fn drop(&mut self) {
        run_async_command(async {
            let _ = self.shutdown().await;
        });
    }
}

pub struct FsOps {}

impl FsOps {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn create_workspace(
        path: impl AsRef<Path>,
        id: &str,
        name: &str,
    ) -> Result<(), FsOpsError> {
        let config = WorkspaceConfig::new(id.to_string(), name.to_string());
        let workspace_path = path.as_ref().join("atuin.toml");
        let contents = toml::to_string(&config)?;
        tokio::fs::write(workspace_path, contents).await?;

        Ok(())
    }

    pub async fn get_workspace_config_by_folder(
        folder: impl AsRef<Path>,
    ) -> Result<WorkspaceConfig, FsOpsError> {
        let config_path = folder.as_ref().join("atuin.toml");
        if !config_path.exists() {
            return Err(FsOpsError::FileMissingError("atuin.toml".to_string()));
        }
        let config = WorkspaceConfig::from_file(config_path).await?;
        Ok(config)
    }

    pub async fn run(&mut self, mut rx: Receiver<FsOpsInstruction>) {
        while let Some(instruction) = rx.recv().await {
            match instruction {
                FsOpsInstruction::GetDirectoryInformation(path, reply_to) => {
                    let info = self.get_directory_information(&path).await;
                    let _ = reply_to.send(info);
                }
                FsOpsInstruction::RenameWorkspace(path, name, reply_to) => {
                    let result = self.rename_workspace(&path, &name).await;
                    let _ = reply_to.send(result);
                }
                FsOpsInstruction::SaveRunbook(
                    runbook_id,
                    old_path,
                    new_path,
                    content,
                    reply_to,
                ) => {
                    let result = self
                        .save_runbook(&runbook_id, old_path.as_ref(), &new_path, content)
                        .await;
                    let _ = reply_to.send(result);
                }
                FsOpsInstruction::DeleteRunbook(path, reply_to) => {
                    let result = self.trash_path(&path).await;
                    let _ = reply_to.send(result);
                }
                FsOpsInstruction::GetRunbook(path, reply_to) => {
                    let result = self.get_runbook(&path).await;
                    let _ = reply_to.send(result);
                }
                FsOpsInstruction::CreateFolder(parent_path, name, reply_to) => {
                    let result = self.create_folder(&parent_path, &name).await;
                    let _ = reply_to.send(result);
                }
                FsOpsInstruction::RenameFolder(from, to, reply_to) => {
                    let result = self.rename_folder(&from, &to).await;
                    let _ = reply_to.send(result);
                }
                FsOpsInstruction::TrashFolder(path, reply_to) => {
                    let result = self.trash_path(&path).await;
                    let _ = reply_to.send(result);
                }
                FsOpsInstruction::MoveItems(items, new_parent, reply_to) => {
                    let result = self.move_items(&items, &new_parent).await;
                    let _ = reply_to.send(result);
                }
                FsOpsInstruction::Shutdown => {
                    break;
                }
            }
        }
    }

    async fn get_directory_information(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<WorkspaceDirInfo, FsOpsError> {
        let config_path = path.as_ref().join("atuin.toml");
        if !config_path.exists() {
            return Err(FsOpsError::FileMissingError("atuin.toml".to_string()));
        }

        let mut contents = read_dir_recursive(path.as_ref()).await?;
        contents.retain(|entry| entry.name.ends_with(".atrb"));
        let config_text = tokio::fs::read_to_string(config_path).await?;
        let config: WorkspaceConfig = toml::from_str(&config_text)?;
        Ok(WorkspaceDirInfo {
            id: config.workspace.id,
            name: config.workspace.name,
            path: path.as_ref().to_path_buf(),
            contents,
        })
    }

    async fn rename_workspace(
        &mut self,
        path: impl AsRef<Path>,
        name: &str,
    ) -> Result<(), FsOpsError> {
        let config_path = path.as_ref().join("atuin.toml");
        if !config_path.exists() {
            return Err(FsOpsError::FileMissingError("atuin.toml".to_string()));
        }

        let config_text = tokio::fs::read_to_string(config_path).await?;
        let mut config: WorkspaceConfig = toml::from_str(&config_text)?;
        config.workspace.name = name.to_string();
        let contents = toml::to_string(&config)?;
        tokio::fs::write(path.as_ref().join("atuin.toml"), contents).await?;
        Ok(())
    }

    // TODO: there's a weird bug where sometimes the file will add a suffix even though its name isn't actually changing,
    // like it's detected that its own file is a name conflict
    async fn save_runbook(
        &mut self,
        runbook_id: &str,
        old_path: Option<impl AsRef<Path>>,
        new_path: impl AsRef<Path>,
        content: Value,
    ) -> Result<(), FsOpsError> {
        let mut needs_unique_path = false;
        if new_path.as_ref().exists() {
            // First, check to see if the file has the same ID as the runbook we're saving
            // Read the existing YAML file and check the ID
            let yaml_text = tokio::fs::read_to_string(new_path.as_ref()).await?;
            let value = serde_yaml::from_str::<Value>(&yaml_text)?;
            if let Some(id) = value.get("id").and_then(|v| v.as_str()) {
                if id != runbook_id {
                    needs_unique_path = true;
                }
            } else {
                // If there's no ID field, we can savely create a new file with that ID
                needs_unique_path = true;
            }
        }

        let new_path = if needs_unique_path {
            find_unique_path(new_path.as_ref())?
        } else {
            new_path.as_ref().to_path_buf()
        };

        if let Some(old_path) = old_path {
            if old_path.as_ref() != new_path && old_path.as_ref().exists() {
                tokio::fs::rename(old_path.as_ref(), &new_path).await?;
            }
        }

        let yaml_text = serde_yaml::to_string(&content)?;
        tokio::fs::write(new_path, yaml_text).await?;

        Ok(())
    }

    async fn get_runbook(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<OfflineRunbookFile, FsOpsError> {
        let yaml_text = tokio::fs::read_to_string(path.as_ref()).await?;
        let yaml_value: Value = serde_yaml::from_str(&yaml_text)?;

        // Convert to JSON for canonical hashing to match save_runbook behavior
        let json_value: serde_json::Value =
            serde_yaml::from_value(yaml_value.clone()).map_err(|_| {
                FsOpsError::DigestError("Failed to convert YAML to JSON for hashing".to_string())
            })?;

        let content_hash = json_digest::digest_data(&json_value)
            .map_err(|e| FsOpsError::DigestError(format!("Failed to hash content: {e}")))?;

        let internal: OfflineRunbookFileInternal = serde_yaml::from_value(yaml_value)?;
        let metadata = tokio::fs::metadata(path.as_ref()).await?;
        let created = metadata.created().ok();
        let updated = metadata.modified().ok();
        let runbook = OfflineRunbookFile::new(internal, content_hash, created, updated);
        Ok(runbook)
    }

    async fn create_folder(
        &mut self,
        parent_path: impl AsRef<Path>,
        name: impl AsRef<Path>,
    ) -> Result<PathBuf, FsOpsError> {
        if !parent_path.as_ref().exists() {
            return Err(FsOpsError::FileMissingError(
                parent_path.as_ref().to_string_lossy().to_string(),
            ));
        }

        let mut suffix: Option<u32> = None;
        let mut target = parent_path.as_ref().join(name.as_ref());

        while target.exists() {
            suffix = Some(suffix.unwrap_or(0) + 1);
            target = parent_path.as_ref().join(format!(
                "{} {}",
                name.as_ref().to_string_lossy(),
                suffix.unwrap()
            ));
        }

        tokio::fs::create_dir(&target).await?;

        Ok(target)
    }

    async fn rename_folder(
        &mut self,
        from: impl AsRef<Path>,
        to: impl AsRef<Path>,
    ) -> Result<(), FsOpsError> {
        tokio::fs::rename(from.as_ref(), to.as_ref()).await?;
        Ok(())
    }

    async fn trash_path(&mut self, path: &PathBuf) -> Result<(), FsOpsError> {
        if !path.exists() {
            return Err(FsOpsError::FileMissingError(
                path.to_string_lossy().to_string(),
            ));
        }

        #[allow(unused_mut)]
        let mut trash_ctx = TrashContext::default();

        // By default, `trash` uses `DeleteMethod::Finder` on macOS, which
        // requires extra permissions and produces the Finder "delete" sound.
        // `NsFileManager` is faster, requires no extra permissions, and
        // doesn't produce a sound. The only downside is that it doesn't add
        // the "Put Back" option to the trash item on some systems.
        //
        // See https://github.com/Byron/trash-rs/blob/b80f7edb1e3db64ae029b02a26d77c11986d9f11/src/macos/mod.rs#L12-L43
        #[cfg(target_os = "macos")]
        {
            use trash::macos::{DeleteMethod, TrashContextExtMacos};
            trash_ctx.set_delete_method(DeleteMethod::NsFileManager);
        }

        trash_ctx.delete(path)?;
        Ok(())
    }

    async fn move_items(&mut self, items: &[PathBuf], new_parent: &Path) -> Result<(), FsOpsError> {
        for item in items {
            let new_path = new_parent.join(item.file_name().unwrap());
            let new_path = find_unique_path(&new_path)?;
            if item.exists() {
                tokio::fs::rename(item, &new_path).await?;
            }
        }
        Ok(())
    }
}

// Using an iterative approach here as recursing fails due to recursive Box::pin issues
pub async fn read_dir_recursive(path: impl AsRef<Path>) -> Result<Vec<DirEntry>, FsOpsError> {
    let workspace_root = path.as_ref();
    let mut gitignore = create_ignore_matcher(workspace_root).ok();

    let mut contents = Vec::new();
    let mut stack = vec![workspace_root.to_path_buf()];

    while let Some(current_path) = stack.pop() {
        let mut dir = tokio::fs::read_dir(&current_path).await?;
        while let Some(entry) = dir.next_entry().await? {
            let mut path = entry.path();
            if path.is_symlink() {
                path = path.read_link()?;
            }

            let containing_folder = path.parent().unwrap_or(workspace_root);
            if !gitignore
                .as_ref()
                .map(|g| g.path() == containing_folder)
                .unwrap_or(false)
            {
                gitignore = create_ignore_matcher(containing_folder).ok();
            }

            // Use our shared ignore logic
            if should_ignore_path(&path, workspace_root, gitignore.as_ref()) {
                continue;
            }

            let attrs = entry.metadata().await?;
            let lastmod = attrs.modified().ok();

            if path.is_dir() {
                contents.push(DirEntry {
                    name: path.file_name().unwrap().to_string_lossy().to_string(),
                    is_dir: true,
                    path: path.clone(),
                    lastmod,
                });
                stack.push(path);
            } else if path.is_file() {
                contents.push(DirEntry {
                    name: path.file_name().unwrap().to_string_lossy().to_string(),
                    is_dir: false,
                    path,
                    lastmod,
                });
            }
        }
    }

    Ok(contents)
}

pub fn find_unique_path(path: impl AsRef<Path>) -> Result<PathBuf, FsOpsError> {
    let stem = path
        .as_ref()
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or(FsOpsError::FileMissingError(
            "path is not a file".to_string(),
        ))?;
    let extension = path
        .as_ref()
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .map(|s| format!(".{s}"))
        .unwrap_or_default();
    let parent = path.as_ref().parent().ok_or(FsOpsError::FileMissingError(
        "path has no parent".to_string(),
    ))?;

    let mut suffix: Option<u32> = None;
    let mut target = path.as_ref().to_path_buf();

    while target.exists() {
        suffix = Some(suffix.unwrap_or(0) + 1);
        target = parent.join(format!(
            "{stem}{}{extension}",
            suffix.map_or(String::new(), |s| format!("-{s}"))
        ));
    }

    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_read_dir_recursive() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        let base = temp.path();

        // Create nested directory structure
        let dir1 = base.join("dir1");
        let dir2 = dir1.join("dir2");
        let dir3 = dir1.join("dir3");
        fs::create_dir_all(&dir2)?;
        fs::create_dir_all(&dir3)?;

        // Create some files
        fs::write(dir1.join("file1.txt"), "content1")?;
        fs::write(dir2.join("file2.txt"), "content2")?;
        fs::write(dir3.join("file3.txt"), "content3")?;
        fs::write(base.join("root.txt"), "root")?;

        let entries = read_dir_recursive(base).await?;

        // Verify we found all directories and files
        assert_eq!(entries.len(), 7); // 3 dirs + 4 files

        // Check for directories
        let dirs: Vec<_> = entries.iter().filter(|e| e.is_dir).collect();
        assert_eq!(dirs.len(), 3);
        assert!(dirs.iter().any(|d| d.path == dir1));
        assert!(dirs.iter().any(|d| d.path == dir2));
        assert!(dirs.iter().any(|d| d.path == dir3));

        // Check for files
        let files: Vec<_> = entries.iter().filter(|e| !e.is_dir).collect();
        assert_eq!(files.len(), 4);
        assert!(files.iter().any(|f| f.path == base.join("root.txt")));
        assert!(files.iter().any(|f| f.path == dir1.join("file1.txt")));
        assert!(files.iter().any(|f| f.path == dir2.join("file2.txt")));
        assert!(files.iter().any(|f| f.path == dir3.join("file3.txt")));

        Ok(())
    }

    #[tokio::test]
    async fn test_find_unique_path() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        let base = temp.path();

        // Test when file doesn't exist - should return same path
        let test_path = base.join("test.txt");
        let unique_path = find_unique_path(&test_path)?;
        assert_eq!(unique_path, test_path);

        // Create the file and test again - should return incremented path
        fs::write(&test_path, "content")?;
        let unique_path = find_unique_path(&test_path)?;
        assert_eq!(unique_path, base.join("test-1.txt"));

        // Create multiple files and verify incrementing behavior
        fs::write(base.join("test-1.txt"), "content")?;
        fs::write(base.join("test-2.txt"), "content")?;
        let unique_path = find_unique_path(&test_path)?;
        assert_eq!(unique_path, base.join("test-3.txt"));

        // Test with file that has no extension
        let no_ext = base.join("noext");
        fs::write(&no_ext, "content")?;
        let unique_path = find_unique_path(&no_ext)?;
        assert_eq!(unique_path, base.join("noext-1"));

        Ok(())
    }
}
