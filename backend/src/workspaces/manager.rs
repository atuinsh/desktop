use notify_debouncer_full::{
    new_debouncer,
    notify::{event::ModifyKind, EventKind, RecursiveMode},
    DebounceEventResult, DebouncedEvent,
};
use serde::Serialize;
use serde_yaml::Value;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use ts_rs::TS;

use crate::{
    run_async_command,
    workspaces::{
        fs_ops::{FsOps, FsOpsHandle, WorkspaceDirInfo},
        offline_runbook::OfflineRunbook,
        state::WorkspaceState,
        workspace::{Workspace, WorkspaceError},
    },
};

use ignore::{gitignore::GitignoreBuilder, WalkBuilder};

// Shared function to create a gitignore matcher with consistent ignore settings
pub fn create_ignore_matcher(
    root_path: &Path,
) -> Result<ignore::gitignore::Gitignore, ignore::Error> {
    let mut builder = GitignoreBuilder::new(root_path);

    // Add gitignore files from the workspace
    let gitignore_path = root_path.join(".gitignore");
    if gitignore_path.exists() {
        builder.add(&gitignore_path);
    }

    builder.build()
}

// Check if a path should be ignored based on our rules
pub fn should_ignore_path(
    path: &Path,
    workspace_root: &Path,
    gitignore: Option<&ignore::gitignore::Gitignore>,
) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Always ignore common directories
    if matches!(
        name,
        "node_modules" | "target" | "dist" | "build" | "__pycache__" | "venv" | "env" | ".env"
    ) {
        return true;
    }

    // Always ignore hidden files/directories (except we might want some)
    if name.starts_with('.') && !matches!(name, ".atrb" | ".atuin") {
        return true;
    }

    // Check gitignore rules if we have them
    if let Some(gitignore) = gitignore {
        let relative_path = path.strip_prefix(workspace_root).unwrap_or(path);
        let matched = gitignore.matched(relative_path, path.is_dir());
        if matched.is_ignore() {
            return true;
        }
    }

    false
}

// Shared function to create WalkBuilder with consistent ignore settings (for file watching only)
pub fn create_ignore_walker<P: AsRef<Path>>(root_path: P) -> WalkBuilder {
    let root_path = root_path.as_ref();
    let mut builder = WalkBuilder::new(root_path);
    builder
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(true)
        .ignore(true)
        .follow_links(false);
    builder
}

pub trait OnEvent: Send + Sync + Fn(WorkspaceEvent) {}
impl<F: Send + Sync + Fn(WorkspaceEvent)> OnEvent for F {}

pub struct WorkspaceManager {
    workspaces: HashMap<String, Workspace>,
}

#[derive(TS, Debug, Serialize, Eq, Hash, PartialEq, Clone)]
#[serde(tag = "type", content = "data")]
#[ts(tag = "type", content = "data", export)]
pub enum WorkspaceEvent {
    State(WorkspaceState),
    Error(WorkspaceError),
    RunbookChanged(String),
    RunbookDeleted(String),
}

impl From<Result<WorkspaceState, WorkspaceError>> for WorkspaceEvent {
    fn from(state: Result<WorkspaceState, WorkspaceError>) -> Self {
        match state {
            Ok(state) => WorkspaceEvent::State(state),
            Err(err) => WorkspaceEvent::Error(err),
        }
    }
}

impl WorkspaceManager {
    pub fn new() -> Self {
        Self {
            workspaces: HashMap::new(),
        }
    }

    pub fn reset(&mut self) {
        // Drop will clean up debouncer and fs_ops
        self.workspaces.clear();
    }

    pub async fn watch_workspace(
        &mut self,
        path: impl AsRef<Path>,
        id: &str,
        on_event: impl OnEvent + 'static,
        // hack: we need to pass the manager to the callback, so we pass in
        // a clone of the manager Arc from the Tauri state
        manager_clone: Arc<tokio::sync::Mutex<Option<WorkspaceManager>>>,
    ) -> Result<(), WorkspaceError> {
        if self.workspaces.contains_key(id) {
            return Err(WorkspaceError::WorkspaceAlreadyWatched {
                workspace_id: id.to_string(),
            });
        }

        let on_event = Arc::new(on_event);

        let id_clone = id.to_string();
        let mut debouncer = new_debouncer(
            // 75ms is fast enough to make the UI feel responsive, but not too fast that
            // we fail to debounce or combine events
            Duration::from_millis(75),
            None,
            move |events_result: DebounceEventResult| {
                let manager_clone = manager_clone.clone();
                let id_clone = id_clone.clone();

                // the debouncer runs on a separate vanilla thread
                run_async_command(async move {
                    if let Some(manager) = manager_clone.lock().await.as_mut() {
                        match events_result {
                            Ok(events) => {
                                manager.handle_file_events(&id_clone, events).await;
                            }
                            Err(errors) => {
                                manager.handle_file_errors(&id_clone, errors).await;
                            }
                        }
                    }
                });
            },
        )
        .map_err(|e| WorkspaceError::WatchError {
            workspace_id: id.to_string(),
            message: e.to_string(),
        })?;

        let fs_ops = FsOpsHandle::new();
        let state =
            WorkspaceState::new(id, &path)
                .await
                .map_err(|e| WorkspaceError::WorkspaceReadError {
                    path: path.as_ref().to_path_buf(),
                    message: e.to_string(),
                });

        on_event(state.clone().into());

        if let Ok(state) = &state {
            for runbook_id in state.runbooks.keys() {
                on_event(WorkspaceEvent::RunbookChanged(runbook_id.clone()));
            }
        }

        // Use selective watching with ignore patterns instead of recursive watching
        self.setup_selective_watching(&mut debouncer, path.as_ref(), id)?;

        let ws = Workspace {
            id: id.to_string(),
            state,
            histories: HashMap::new(),
            path: path.as_ref().to_path_buf(),
            _debouncer: debouncer,
            fs_ops,
            on_event,
        };

        self.workspaces.insert(id.to_string(), ws);

        Ok(())
    }

    pub async fn unwatch_workspace(&mut self, id: &str) -> Result<(), WorkspaceError> {
        // Drop will clean up debouncer and fs_ops
        self.workspaces.remove(id);
        Ok(())
    }

    pub async fn create_workspace(
        &mut self,
        path: impl AsRef<Path>,
        id: &str,
        name: &str,
    ) -> Result<(), WorkspaceError> {
        FsOps::create_workspace(path, id, name).await.map_err(|e| {
            WorkspaceError::WorkspaceCreateError {
                workspace_id: id.to_string(),
                message: e.to_string(),
            }
        })
    }

    pub async fn rename_workspace(&mut self, id: &str, name: &str) -> Result<(), WorkspaceError> {
        let workspace = self.get_workspace_mut(id)?;
        workspace.rename(name).await
    }

    pub async fn create_runbook(
        &mut self,
        workspace_id: &str,
        parent_folder_id: Option<&str>,
        name: &str,
        content: &Value,
        forked_from: Option<&str>,
    ) -> Result<String, WorkspaceError> {
        let workspace = self.get_workspace_mut(workspace_id)?;
        workspace
            .create_runbook(parent_folder_id, name, content, forked_from)
            .await
    }

    pub async fn save_runbook(
        &mut self,
        workspace_id: &str,
        runbook_id: &str,
        name: &str,
        content: Value,
    ) -> Result<String, WorkspaceError> {
        let workspace = self.get_workspace_mut(workspace_id)?;

        // Preserve existing forked_from value when saving
        let forked_from = workspace
            .state
            .as_ref()
            .ok()
            .and_then(|s| s.runbooks.get(runbook_id))
            .and_then(|r| r.forked_from.clone());

        workspace
            .save_runbook(
                runbook_id,
                name,
                &content,
                None::<&Path>,
                forked_from.as_deref(),
            )
            .await
    }

    pub async fn delete_runbook(
        &mut self,
        workspace_id: &str,
        runbook_id: &str,
    ) -> Result<(), WorkspaceError> {
        let workspace = self.get_workspace_mut(workspace_id)?;
        workspace.delete_runbook(runbook_id).await
    }

    pub async fn get_runbook(
        &mut self,
        runbook_id: &str,
    ) -> Result<OfflineRunbook, WorkspaceError> {
        let workspace = self.workspaces.values_mut().find(|w| {
            w.state
                .as_ref()
                .map(|s| s.runbooks.contains_key(runbook_id))
                .unwrap_or(false)
        });

        if let Some(workspace) = workspace {
            workspace.get_runbook(runbook_id).await
        } else {
            Err(WorkspaceError::GenericWorkspaceError {
                message: format!("Runbook {runbook_id} not found"),
            })
        }
    }

    pub async fn create_folder(
        &mut self,
        workspace_id: &str,
        parent_path: Option<&str>,
        name: &str,
    ) -> Result<PathBuf, WorkspaceError> {
        let workspace = self.get_workspace_mut(workspace_id)?;
        workspace
            .create_folder(parent_path.map(Path::new), name)
            .await
    }

    pub async fn rename_folder(
        &mut self,
        workspace_id: &str,
        folder_id: &str,
        new_name: &str,
    ) -> Result<(), WorkspaceError> {
        let workspace = self.get_workspace_mut(workspace_id)?;
        workspace.rename_folder(folder_id, new_name).await
    }

    pub async fn delete_folder(
        &mut self,
        workspace_id: &str,
        folder_id: &str,
    ) -> Result<(), WorkspaceError> {
        let workspace = self.get_workspace_mut(workspace_id)?;
        workspace.delete_folder(folder_id).await
    }

    pub async fn move_items(
        &mut self,
        workspace_id: &str,
        item_ids: &[String],
        new_parent: Option<&str>,
    ) -> Result<(), WorkspaceError> {
        let workspace = self.get_workspace_mut(workspace_id)?;
        workspace.move_items(item_ids, new_parent).await
    }

    pub async fn move_items_between_workspaces(
        &mut self,
        item_ids: &[String],
        old_workspace_id: &str,
        new_workspace_id: &str,
        new_parent_folder_id: Option<&str>,
    ) -> Result<(), WorkspaceError> {
        let (old_root, paths_to_move) = {
            let old_workspace = self.get_workspace(old_workspace_id)?;

            if old_workspace.state.is_err() {
                return Err(WorkspaceError::GenericWorkspaceError {
                    message: "Old workspace is in an error state".to_string(),
                });
            }

            let old_root = old_workspace.path.clone();
            let paths_to_move = old_workspace
                .state
                .as_ref()
                .unwrap()
                .calculate_toplevel_paths(item_ids);

            (old_root, paths_to_move)
        };

        let new_workspace = self.get_workspace_mut(new_workspace_id)?;
        new_workspace
            .move_into_workspace(
                &paths_to_move,
                &old_root,
                new_parent_folder_id.map(PathBuf::from),
            )
            .await
    }

    pub async fn get_dir_info(
        &mut self,
        workspace_id: &str,
    ) -> Result<WorkspaceDirInfo, WorkspaceError> {
        let workspace = self.get_workspace_mut(workspace_id)?;
        workspace.get_dir_info().await
    }

    pub async fn get_workspace_id_by_folder(
        &mut self,
        folder: &Path,
    ) -> Result<String, WorkspaceError> {
        let config = FsOps::get_workspace_config_by_folder(folder)
            .await
            .map_err(|e| WorkspaceError::WorkspaceReadError {
                path: folder.to_path_buf(),
                message: e.to_string(),
            })?;
        Ok(config.workspace.id)
    }

    async fn rescan_workspace(&self, workspace_id: &str) -> Result<WorkspaceState, WorkspaceError> {
        if let Some(workspace) = self.workspaces.get(workspace_id) {
            workspace.rescan().await
        } else {
            Err(WorkspaceError::WorkspaceNotWatched {
                workspace_id: workspace_id.to_string(),
            })
        }
    }

    async fn handle_file_events(&mut self, workspace_id: &str, events: Vec<DebouncedEvent>) {
        if !self.workspaces.contains_key(workspace_id) {
            return;
        }

        log::debug!(
            "Handling {} file events for workspace {}",
            events.len(),
            workspace_id
        );
        let mut full_rescan = false;

        if self
            .workspaces
            .get(workspace_id)
            .map(|w| w.state.is_err())
            .unwrap_or(false)
        {
            full_rescan = true;
        }

        let mut known_events = HashSet::new();

        for event in events {
            // break early if the workspace is in an error state
            if full_rescan {
                break;
            }

            // returns true if notify detects some events may have been missed
            if event.need_rescan() {
                full_rescan = true;
                break;
            }

            let has_relevant_paths = event.paths.iter().any(|path| {
                if let Some(workspace) = self.workspaces.get(workspace_id) {
                    let gitignore = create_ignore_matcher(&workspace.path).ok();
                    !should_ignore_path(path, &workspace.path, gitignore.as_ref())
                        && is_relevant_file(path)
                } else {
                    false
                }
            });

            if !has_relevant_paths {
                log::debug!("Skipping irrelevant paths: {:?}", event.paths);
                continue;
            }

            log::debug!(
                "Processing relevant event: {:?} for paths: {:?}",
                event.event.kind,
                event.paths
            );

            match event.event.kind {
                EventKind::Access(_) => {}
                EventKind::Modify(kind) => {
                    if matches!(kind, ModifyKind::Name(_)) {
                        full_rescan = true;
                        break;
                    }

                    if let Some(path) = event.paths.first() {
                        // n.b. this is a hack to handle the case where a file is deleted
                        // and it gets reported as a pair of modification events
                        // (why does this happen??)
                        if !path.exists() {
                            full_rescan = true;
                            break;
                        }

                        if let Some(workspace) = self.workspaces.get_mut(workspace_id) {
                            if workspace.path.join("atuin.toml") == *path {
                                full_rescan = true;
                                break;
                            }

                            let matching_runbook = workspace
                                .state
                                .as_ref()
                                .ok()
                                .and_then(|s| s.runbooks.values().find(|r| r.path == *path));

                            if let Some(runbook) = matching_runbook {
                                known_events
                                    .insert(WorkspaceEvent::RunbookChanged(runbook.id.clone()));
                            }
                        }
                    }
                }
                EventKind::Create(_) | EventKind::Remove(_) => {
                    // Handle directory creation/deletion
                    if let Some(path) = event.paths.first() {
                        match event.event.kind {
                            EventKind::Create(_) if path.is_dir() => {
                                if let Some(workspace) = self.workspaces.get_mut(workspace_id) {
                                    let parent_path = path.parent().unwrap_or(&workspace.path);
                                    let gitignore = create_ignore_matcher(parent_path).ok();
                                    if !should_ignore_path(
                                        path,
                                        &workspace.path,
                                        gitignore.as_ref(),
                                    ) {
                                        log::debug!(
                                            "Adding watcher for new directory: {}",
                                            path.display()
                                        );
                                        if let Err(e) = workspace
                                            ._debouncer
                                            .watch(path, RecursiveMode::NonRecursive)
                                        {
                                            log::warn!(
                                                "Failed to watch new directory {}: {}",
                                                path.display(),
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                            EventKind::Remove(_) => {
                                // Remove watcher for deleted directory/file
                                if let Some(workspace_mut) = self.workspaces.get_mut(workspace_id) {
                                    log::debug!(
                                        "Removing watcher for deleted path: {}",
                                        path.display()
                                    );
                                    if let Err(e) = workspace_mut._debouncer.unwatch(path) {
                                        log::debug!("Failed to unwatch deleted path {} (this is normal): {}", path.display(), e);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    full_rescan = true;
                    break;
                }
                _ => {
                    // TODO: this could probably be smarter/more granular in the future,
                    // but there are enough edge cases that for now we just do a full rescan
                    full_rescan = true;
                    break;
                }
            }
        }

        if full_rescan {
            self.rescan_and_notify(workspace_id, Some(known_events))
                .await;
        } else {
            self.notify(workspace_id, known_events);
        }
    }

    async fn handle_file_errors(
        &mut self,
        workspace_id: &str,
        _errors: Vec<notify_debouncer_full::notify::Error>,
    ) {
        if !self.workspaces.contains_key(workspace_id) {
            return;
        }

        self.rescan_and_notify(workspace_id, None).await;
    }

    async fn rescan_and_notify(
        &mut self,
        workspace_id: &str,
        known_events: Option<HashSet<WorkspaceEvent>>,
    ) {
        let mut known_events = known_events.unwrap_or_default();

        match self.rescan_workspace(workspace_id).await {
            Ok(updated) => {
                if let Some(workspace) = self.workspaces.get_mut(workspace_id) {
                    if let Ok(state) = &workspace.state {
                        // since we're doing a full rescan, use runbook file metadata to figure out which changed
                        for (id, runbook) in updated.runbooks.iter() {
                            if let Some(workspace_runbook) = state.runbooks.get(id) {
                                if workspace_runbook.lastmod != runbook.lastmod {
                                    known_events.insert(WorkspaceEvent::RunbookChanged(id.clone()));
                                }
                            } else {
                                // runbook is new to the workspace
                                known_events.insert(WorkspaceEvent::RunbookChanged(id.clone()));
                            }
                        }

                        for (id, _runbook) in state.runbooks.iter() {
                            if !updated.runbooks.contains_key(id) {
                                known_events.insert(WorkspaceEvent::RunbookDeleted(id.clone()));
                            }
                        }
                    } else {
                        // If the workspace was in an error state previously,
                        // we should notify about every runbook that exists in the new state
                        for (id, _runbook) in updated.runbooks.iter() {
                            known_events.insert(WorkspaceEvent::RunbookChanged(id.clone()));
                        }
                    }

                    let mut watched_dirs = HashSet::new();
                    for entry in updated.entries.iter() {
                        if !entry.path.is_dir() {
                            continue;
                        }

                        let parent_path = entry.path.parent().unwrap_or(&workspace.path);
                        if watched_dirs.contains(&parent_path) {
                            continue;
                        }

                        let gitignore = create_ignore_matcher(parent_path).ok();
                        if !should_ignore_path(&entry.path, &workspace.path, gitignore.as_ref()) {
                            log::debug!(
                                "Adding watcher for directory found during rescan: {}",
                                entry.path.display()
                            );
                            if let Err(e) = workspace
                                ._debouncer
                                .watch(parent_path, RecursiveMode::NonRecursive)
                            {
                                log::warn!(
                                    "Failed to watch new directory {}: {}",
                                    parent_path.display(),
                                    e
                                );
                            } else {
                                watched_dirs.insert(parent_path);
                            }
                        }
                    }

                    workspace.state = Ok(updated);
                    (workspace.on_event)(workspace.state.clone().into());
                }
            }
            Err(e) => {
                if let Some(workspace) = self.workspaces.get_mut(workspace_id) {
                    workspace.state = Err(e);

                    (workspace.on_event)(workspace.state.clone().into());
                }
            }
        }

        self.notify(workspace_id, known_events);
    }

    fn notify(&self, workspace_id: &str, events: HashSet<WorkspaceEvent>) {
        if let Some(workspace) = self.workspaces.get(workspace_id) {
            for event in events {
                (workspace.on_event)(event);
            }
        }
    }

    pub async fn shutdown(&mut self) {
        self.reset();
    }

    /// Find the workspace root path for a given runbook ID
    /// Returns None if the runbook is not found in any workspace
    pub fn workspace_root(&self, runbook_id: &str) -> Option<PathBuf> {
        for workspace in self.workspaces.values() {
            if let Ok(workspace_state) = &workspace.state {
                if workspace_state.runbooks.contains_key(runbook_id) {
                    return Some(workspace_state.root.clone());
                }
            }
        }
        None
    }

    fn get_workspace(&mut self, id: &str) -> Result<&Workspace, WorkspaceError> {
        self.workspaces
            .get(id)
            .ok_or(WorkspaceError::WorkspaceNotWatched {
                workspace_id: id.to_string(),
            })
    }

    fn get_workspace_mut(&mut self, id: &str) -> Result<&mut Workspace, WorkspaceError> {
        self.workspaces
            .get_mut(id)
            .ok_or(WorkspaceError::WorkspaceNotWatched {
                workspace_id: id.to_string(),
            })
    }

    fn setup_selective_watching(
        &self,
        debouncer: &mut notify_debouncer_full::Debouncer<
            notify_debouncer_full::notify::RecommendedWatcher,
            notify_debouncer_full::RecommendedCache,
        >,
        root_path: &Path,
        workspace_id: &str,
    ) -> Result<(), WorkspaceError> {
        log::debug!(
            "Setting up selective watching for workspace {} at {}",
            workspace_id,
            root_path.display()
        );

        // TODO: This is a single threaded walker. Investigate perf bonus of a parallel walker
        let walker = create_ignore_walker(root_path).build();

        let mut watched_count = 0;
        for entry in walker.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                log::debug!("Watching directory: {}", path.display());
                debouncer
                    .watch(path, RecursiveMode::NonRecursive)
                    .map_err(|e| WorkspaceError::WatchError {
                        workspace_id: workspace_id.to_string(),
                        message: format!("Failed to watch directory {}: {}", path.display(), e),
                    })?;
                watched_count += 1;
            }
        }

        log::info!("Watching {watched_count} directories for workspace {workspace_id}",);
        Ok(())
    }
}

fn is_relevant_file(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();

    // Only check if this is content we care about (not ignore logic)
    path.is_dir()
        || !path.exists() // If the file doesn't exist, we may have deleted it
        || path
            .file_name()
            .map(|f| f.to_string_lossy().eq_ignore_ascii_case("atuin.toml"))
            .unwrap_or(false)
        || path
            .extension()
            .map(|f| f.to_string_lossy().eq_ignore_ascii_case("atrb"))
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::{Path, PathBuf},
        sync::Arc,
        time::Duration,
    };

    use serde_json::json;
    use tempfile::TempDir;
    use tokio::sync::Mutex;
    use tokio::time::sleep;

    struct TestEventCollector {
        events: Arc<Mutex<Vec<WorkspaceEvent>>>,
    }

    impl TestEventCollector {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }

        async fn get_events(&self) -> Vec<WorkspaceEvent> {
            self.events.lock().await.clone()
        }

        async fn clear_events(&self) {
            self.events.lock().await.clear();
        }

        async fn wait_for_events(
            &self,
            expected_count: usize,
            timeout_ms: u64,
        ) -> Vec<WorkspaceEvent> {
            let start = std::time::Instant::now();
            let timeout = Duration::from_millis(timeout_ms);

            loop {
                let events = self.get_events().await;
                if events.len() >= expected_count || start.elapsed() > timeout {
                    return events;
                }
                sleep(Duration::from_millis(10)).await;
            }
        }

        fn create_handler(&self) -> impl Fn(WorkspaceEvent) + Send + Sync + 'static + use<> {
            let events = self.events.clone();
            move |event| {
                let events = events.clone();
                tokio::spawn(async move {
                    events.lock().await.push(event);
                });
            }
        }
    }

    async fn setup_test_workspace() -> (TempDir, PathBuf) {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace_path = temp_dir.path().to_path_buf();

        // Create atuin.toml
        let config = toml::to_string(&crate::workspaces::fs_ops::WorkspaceConfig {
            workspace: crate::workspaces::fs_ops::WorkspaceConfigDetails {
                id: "test-workspace".to_string(),
                name: "Test Workspace".to_string(),
            },
        })
        .unwrap();

        tokio::fs::write(workspace_path.join("atuin.toml"), config)
            .await
            .unwrap();

        (temp_dir, workspace_path)
    }

    async fn create_test_runbook(path: &Path, name: &str, id: &str) {
        let runbook_content = json!({
            "id": id,
            "name": name,
            "version": 1,
            "content": []
        });

        tokio::fs::write(
            path.join(format!("{name}.atrb")),
            runbook_content.to_string(),
        )
        .await
        .unwrap();
    }

    async fn create_gitignore(path: &Path, patterns: &[&str]) {
        let content = patterns.join("\n");
        tokio::fs::write(path.join(".gitignore"), content)
            .await
            .unwrap();
    }

    async fn wait_for_state_condition<F>(
        manager_arc: &Arc<Mutex<Option<WorkspaceManager>>>,
        workspace_id: &str,
        predicate: F,
        timeout_ms: u64,
    ) -> bool
    where
        F: Fn(&WorkspaceState) -> bool,
    {
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(timeout_ms);

        loop {
            let manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_ref().unwrap();
            if let Some(workspace) = manager.workspaces.get(workspace_id) {
                if let Ok(state) = &workspace.state {
                    if predicate(state) {
                        return true;
                    }
                }
            }
            drop(manager_guard);

            if start.elapsed() > timeout {
                return false;
            }
            sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_folder_creation_and_runbook_operations() {
        let (temp_dir, workspace_path) = setup_test_workspace().await;
        let collector = TestEventCollector::new();
        let manager = WorkspaceManager::new();
        let manager_arc = Arc::new(Mutex::new(Some(manager)));

        // Start watching the workspace
        {
            let mut manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_mut().unwrap();
            manager
                .watch_workspace(
                    &workspace_path,
                    "test-workspace",
                    collector.create_handler(),
                    manager_arc.clone(),
                )
                .await
                .unwrap();
        }

        // Wait for initial state event
        let initial_events = collector.wait_for_events(1, 1000).await;
        assert!(!initial_events.is_empty());
        collector.clear_events().await;

        // Give the workspace manager time to set up file watching
        sleep(Duration::from_millis(250)).await;

        // Create a new folder
        let folder_path = workspace_path.join("test_folder");
        tokio::fs::create_dir(&folder_path).await.unwrap();

        // Wait for folder creation to be detected
        let folder_events = collector.wait_for_events(1, 2000).await;
        assert!(!folder_events.is_empty());
        collector.clear_events().await;

        // Create a runbook in the new folder
        create_test_runbook(&folder_path, "test_runbook", "test-runbook-id").await;

        // Give a moment for the directory watcher to be set up
        sleep(Duration::from_millis(200)).await;

        // Wait for runbook creation to be detected
        let runbook_events = collector.wait_for_events(1, 5000).await;
        assert!(
            !runbook_events.is_empty(),
            "Expected runbook creation events, got: {runbook_events:?}"
        );
        collector.clear_events().await;

        // Verify the runbook was added to state using polling
        assert!(
            wait_for_state_condition(
                &manager_arc,
                "test-workspace",
                |state| state.runbooks.contains_key("test-runbook-id"),
                2000
            )
            .await,
            "Runbook was not added to state within timeout"
        );

        // Verify runbook properties
        {
            let manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_ref().unwrap();
            if let Some(workspace) = manager.workspaces.get("test-workspace") {
                if let Ok(state) = &workspace.state {
                    let runbook = state.runbooks.get("test-runbook-id").unwrap();
                    assert_eq!(runbook.name, "test_runbook");
                    assert_eq!(runbook.id, "test-runbook-id");
                } else {
                    panic!("Workspace state should be Ok");
                }
            } else {
                panic!("Workspace should exist");
            }
        }

        // Remove the runbook
        tokio::fs::remove_file(folder_path.join("test_runbook.atrb"))
            .await
            .unwrap();

        // Wait for runbook deletion to be detected
        let deletion_events = collector.wait_for_events(1, 5000).await;
        assert!(
            !deletion_events.is_empty(),
            "Expected runbook deletion events, got: {deletion_events:?}"
        );
        collector.clear_events().await;

        // Verify the runbook was removed from state using polling
        assert!(
            wait_for_state_condition(
                &manager_arc,
                "test-workspace",
                |state| !state.runbooks.contains_key("test-runbook-id"),
                2000
            )
            .await,
            "Runbook was not removed from state within timeout"
        );

        // Clean up
        {
            let mut manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_mut().unwrap();
            manager.unwatch_workspace("test-workspace").await.unwrap();
        }

        drop(temp_dir);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_common_directories_ignored() {
        let (temp_dir, workspace_path) = setup_test_workspace().await;
        let collector = TestEventCollector::new();
        let manager = WorkspaceManager::new();
        let manager_arc = Arc::new(Mutex::new(Some(manager)));

        // Start watching the workspace
        {
            let mut manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_mut().unwrap();
            manager
                .watch_workspace(
                    &workspace_path,
                    "test-workspace",
                    collector.create_handler(),
                    manager_arc.clone(),
                )
                .await
                .unwrap();
        }

        // Wait for initial state event
        let initial_events = collector.wait_for_events(1, 1000).await;
        assert!(!initial_events.is_empty());
        collector.clear_events().await;

        // Create common directories that should be ignored
        let ignored_dirs = [
            "node_modules",
            "target",
            "dist",
            "build",
            "__pycache__",
            "venv",
            "env",
            ".env",
        ];

        for dir_name in &ignored_dirs {
            let dir_path = workspace_path.join(dir_name);
            tokio::fs::create_dir(&dir_path).await.unwrap();

            // Create a runbook in the ignored directory
            create_test_runbook(&dir_path, "ignored_runbook", &format!("ignored-{dir_name}")).await;
        }

        // Wait a bit to ensure any events would have been processed
        sleep(Duration::from_millis(200)).await;

        // Check that no events were generated for ignored directories
        let events = collector.get_events().await;

        for event in events.iter() {
            match event {
                WorkspaceEvent::State(state) => {
                    for entry in state.entries.iter() {
                        let ignored = ignored_dirs
                            .iter()
                            .any(|dir| entry.path.display().to_string().contains(dir));

                        assert!(!ignored);
                    }
                }
                _ => {}
            }
        }

        // Verify that ignored directories and their contents are not in the workspace state
        {
            let manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_ref().unwrap();
            if let Some(workspace) = manager.workspaces.get("test-workspace") {
                if let Ok(state) = &workspace.state {
                    for dir_name in &ignored_dirs {
                        assert!(!state.runbooks.contains_key(&format!("ignored-{dir_name}")));
                    }
                } else {
                    panic!("Workspace state should be Ok");
                }
            } else {
                panic!("Workspace should exist");
            }
        }

        // Clean up
        {
            let mut manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_mut().unwrap();
            manager.unwatch_workspace("test-workspace").await.unwrap();
        }

        drop(temp_dir);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_gitignore_in_root_directory() {
        let (temp_dir, workspace_path) = setup_test_workspace().await;
        let collector = TestEventCollector::new();
        let manager = WorkspaceManager::new();
        let manager_arc = Arc::new(Mutex::new(Some(manager)));

        // Create .gitignore in root
        create_gitignore(
            &workspace_path,
            &["ignored_folder/", "*.tmp", "temp_file.atrb"],
        )
        .await;

        // Start watching the workspace
        {
            let mut manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_mut().unwrap();
            manager
                .watch_workspace(
                    &workspace_path,
                    "test-workspace",
                    collector.create_handler(),
                    manager_arc.clone(),
                )
                .await
                .unwrap();
        }

        // Wait for initial state event
        let initial_events = collector.wait_for_events(1, 1000).await;
        assert!(!initial_events.is_empty());
        collector.clear_events().await;

        // Create ignored folder and runbook
        let ignored_folder = workspace_path.join("ignored_folder");
        tokio::fs::create_dir(&ignored_folder).await.unwrap();
        create_test_runbook(&ignored_folder, "ignored_runbook", "ignored-runbook-id").await;

        // Create ignored file
        create_test_runbook(&workspace_path, "temp_file", "temp-file-id").await;

        // Create a temporary file that should be ignored
        tokio::fs::write(workspace_path.join("test.tmp"), "temp content")
            .await
            .unwrap();

        // Wait to ensure any events would have been processed (if they were going to arrive)
        sleep(Duration::from_millis(500)).await;

        // Verify no events were generated for ignored files
        let events = collector.get_events().await;
        for event in events.iter() {
            if let WorkspaceEvent::State(state) = event {
                assert!(
                    !state.runbooks.contains_key("ignored-runbook-id"),
                    "Ignored runbook should not be in state events"
                );
                assert!(
                    !state.runbooks.contains_key("temp-file-id"),
                    "Ignored file should not be in state events"
                );
            }
        }

        // Verify that gitignored content is not in the workspace state
        assert!(
            wait_for_state_condition(
                &manager_arc,
                "test-workspace",
                |state| {
                    !state.runbooks.contains_key("ignored-runbook-id")
                        && !state.runbooks.contains_key("temp-file-id")
                },
                2000
            )
            .await,
            "Ignored items should not be in state"
        );

        // Clean up
        {
            let mut manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_mut().unwrap();
            manager.unwatch_workspace("test-workspace").await.unwrap();
        }

        drop(temp_dir);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_gitignore_in_subdirectory() {
        let (temp_dir, workspace_path) = setup_test_workspace().await;
        let collector = TestEventCollector::new();
        let manager = WorkspaceManager::new();
        let manager_arc = Arc::new(Mutex::new(Some(manager)));

        create_gitignore(&workspace_path, &["subdir/local_ignored/"]).await;

        // Create a subdirectory
        let subdir = workspace_path.join("subdir");
        tokio::fs::create_dir(&subdir).await.unwrap();

        // Create .gitignore in subdirectory
        create_gitignore(&subdir, &["local_ignored/", "*.local.atrb"]).await;

        // Start watching the workspace
        {
            let mut manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_mut().unwrap();
            manager
                .watch_workspace(
                    &workspace_path,
                    "test-workspace",
                    collector.create_handler(),
                    manager_arc.clone(),
                )
                .await
                .unwrap();
        }

        // Wait for initial state event
        let initial_events = collector.wait_for_events(1, 1000).await;
        assert!(!initial_events.is_empty());
        collector.clear_events().await;

        // Create ignored folder in subdirectory
        let ignored_subfolder = subdir.join("local_ignored");
        tokio::fs::create_dir(&ignored_subfolder).await.unwrap();
        create_test_runbook(&ignored_subfolder, "local_runbook", "local-runbook-id").await;

        // Create ignored file in subdirectory
        create_test_runbook(&subdir, "test.local", "local-file-id").await;

        // Create a valid runbook in subdirectory (should not be ignored)
        create_test_runbook(&subdir, "valid_runbook", "valid-runbook-id").await;

        // Wait for events with timeout - without this pattern, this test can be flaky
        let events_future = async {
            loop {
                let events = collector.get_events().await;
                if !events.is_empty() {
                    break events;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            }
        };
        let timeout_future = tokio::time::sleep(tokio::time::Duration::from_millis(1000));

        match tokio::select! {
            events = events_future => Ok(events),
            _ = timeout_future => Err("Timeout waiting for events"),
        } {
            Ok(events) => assert!(
                !events.is_empty(),
                "Events should be generated for valid runbook"
            ),
            Err(e) => panic!("{}", e),
        };

        // Verify that only the valid runbook is in the workspace state
        {
            let manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_ref().unwrap();
            if let Some(workspace) = manager.workspaces.get("test-workspace") {
                if let Ok(state) = &workspace.state {
                    assert!(!state.runbooks.contains_key("local-runbook-id"));
                    assert!(!state.runbooks.contains_key("local-file-id"));
                    assert!(state.runbooks.contains_key("valid-runbook-id"));
                } else {
                    panic!("Workspace state should be Ok");
                }
            } else {
                panic!("Workspace should exist");
            }
        }

        // Clean up
        {
            let mut manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_mut().unwrap();
            manager.unwatch_workspace("test-workspace").await.unwrap();
        }

        drop(temp_dir);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_gitignore_in_parent_directory() {
        let (temp_dir, workspace_path) = setup_test_workspace().await;
        let collector = TestEventCollector::new();
        let manager = WorkspaceManager::new();
        let manager_arc = Arc::new(Mutex::new(Some(manager)));

        // Create .gitignore in parent directory (temp_dir)
        create_gitignore(temp_dir.path(), &["parent_ignored/", "*.parent.atrb"]).await;

        // Start watching the workspace
        {
            let mut manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_mut().unwrap();
            manager
                .watch_workspace(
                    &workspace_path,
                    "test-workspace",
                    collector.create_handler(),
                    manager_arc.clone(),
                )
                .await
                .unwrap();
        }

        // Wait for initial state event
        let initial_events = collector.wait_for_events(1, 1000).await;
        assert!(!initial_events.is_empty());
        collector.clear_events().await;

        // Create ignored folder in workspace (should be ignored due to parent .gitignore)
        let ignored_folder = workspace_path.join("parent_ignored");
        tokio::fs::create_dir(&ignored_folder).await.unwrap();
        create_test_runbook(&ignored_folder, "parent_runbook", "parent-runbook-id").await;

        // Create ignored file in workspace
        create_test_runbook(&workspace_path, "test.parent", "parent-file-id").await;

        // Create a valid runbook (should not be ignored)
        create_test_runbook(&workspace_path, "valid_runbook", "valid-runbook-id").await;

        // Wait for the valid runbook to appear in state
        assert!(
            wait_for_state_condition(
                &manager_arc,
                "test-workspace",
                |state| state.runbooks.contains_key("valid-runbook-id"),
                2000
            )
            .await,
            "Valid runbook should be added to state"
        );

        // Wait a bit longer to ensure any events for ignored items would have arrived (if they were going to)
        sleep(Duration::from_millis(300)).await;

        // Check that only the valid runbook generated events
        let events = collector.get_events().await;
        assert!(
            !events.is_empty(),
            "Events should be generated for valid runbook"
        );

        // Verify no events contain the ignored items
        for event in events.iter() {
            if let WorkspaceEvent::State(state) = event {
                assert!(
                    !state.runbooks.contains_key("parent-runbook-id"),
                    "Parent-ignored runbook should not be in state events"
                );
                assert!(
                    !state.runbooks.contains_key("parent-file-id"),
                    "Parent-ignored file should not be in state events"
                );
            }
        }

        // Verify that only the valid runbook is in the workspace state
        assert!(
            wait_for_state_condition(
                &manager_arc,
                "test-workspace",
                |state| {
                    !state.runbooks.contains_key("parent-runbook-id")
                        && !state.runbooks.contains_key("parent-file-id")
                        && state.runbooks.contains_key("valid-runbook-id")
                },
                2000
            )
            .await,
            "Only valid runbook should be in state, ignored items should not be present"
        );

        // Clean up
        {
            let mut manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_mut().unwrap();
            manager.unwatch_workspace("test-workspace").await.unwrap();
        }

        drop(temp_dir);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_hidden_files_ignored() {
        let (temp_dir, workspace_path) = setup_test_workspace().await;
        let collector = TestEventCollector::new();
        let manager = WorkspaceManager::new();
        let manager_arc = Arc::new(Mutex::new(Some(manager)));

        // Start watching the workspace
        {
            let mut manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_mut().unwrap();
            manager
                .watch_workspace(
                    &workspace_path,
                    "test-workspace",
                    collector.create_handler(),
                    manager_arc.clone(),
                )
                .await
                .unwrap();
        }

        // Wait for initial state event
        let initial_events = collector.wait_for_events(1, 1000).await;
        assert!(!initial_events.is_empty());
        collector.clear_events().await;

        // Create hidden directories and files (should be ignored)
        let hidden_dirs = [".hidden_dir", ".git", ".vscode", ".idea"];
        for dir_name in &hidden_dirs {
            let dir_path = workspace_path.join(dir_name);
            tokio::fs::create_dir(&dir_path).await.unwrap();
            create_test_runbook(&dir_path, "hidden_runbook", &format!("hidden-{dir_name}")).await;
        }

        // Create hidden files
        create_test_runbook(&workspace_path, ".hidden_file", "hidden-file-id").await;

        // Create a valid runbook (should not be ignored)
        create_test_runbook(&workspace_path, "valid_runbook", "valid-runbook-id").await;

        // Wait for the valid runbook to appear in state
        assert!(
            wait_for_state_condition(
                &manager_arc,
                "test-workspace",
                |state| state.runbooks.contains_key("valid-runbook-id"),
                2000
            )
            .await,
            "Valid runbook should be added to state"
        );

        // Wait a bit longer to ensure any events for hidden items would have arrived (if they were going to)
        sleep(Duration::from_millis(300)).await;

        // Check that only the valid runbook generated events
        let events = collector.get_events().await;
        assert!(
            !events.is_empty(),
            "Events should be generated for valid runbook"
        );

        // Verify no events contain the hidden items
        for event in events.iter() {
            if let WorkspaceEvent::State(state) = event {
                for dir_name in &hidden_dirs {
                    assert!(
                        !state.runbooks.contains_key(&format!("hidden-{dir_name}")),
                        "Hidden directory runbook should not be in state events: {dir_name}"
                    );
                }
                assert!(
                    !state.runbooks.contains_key("hidden-file-id"),
                    "Hidden file should not be in state events"
                );
            }
        }

        // Verify that only the valid runbook is in the workspace state
        assert!(
            wait_for_state_condition(
                &manager_arc,
                "test-workspace",
                |state| {
                    let no_hidden_dirs = hidden_dirs
                        .iter()
                        .all(|dir| !state.runbooks.contains_key(&format!("hidden-{dir}")));
                    no_hidden_dirs
                        && !state.runbooks.contains_key("hidden-file-id")
                        && state.runbooks.contains_key("valid-runbook-id")
                },
                2000
            )
            .await,
            "Only valid runbook should be in state, hidden items should not be present"
        );

        // Clean up
        {
            let mut manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_mut().unwrap();
            manager.unwatch_workspace("test-workspace").await.unwrap();
        }

        drop(temp_dir);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_debounced_events() {
        let (temp_dir, workspace_path) = setup_test_workspace().await;
        let collector = TestEventCollector::new();
        let manager = WorkspaceManager::new();
        let manager_arc = Arc::new(Mutex::new(Some(manager)));

        // Start watching the workspace
        {
            let mut manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_mut().unwrap();
            manager
                .watch_workspace(
                    &workspace_path,
                    "test-workspace",
                    collector.create_handler(),
                    manager_arc.clone(),
                )
                .await
                .unwrap();
        }

        // Wait for initial state event
        let initial_events = collector.wait_for_events(1, 1000).await;
        assert!(!initial_events.is_empty());
        collector.clear_events().await;

        // Rapidly create multiple runbooks to test debouncing
        for i in 0..5 {
            create_test_runbook(
                &workspace_path,
                &format!("runbook_{i}"),
                &format!("runbook-{i}"),
            )
            .await;
            // Small delay to ensure events are separate but within debounce window
            sleep(Duration::from_millis(10)).await;
        }

        // Wait for debounced events to be processed
        let events = collector.wait_for_events(1, 1000).await;
        assert!(
            !events.is_empty(),
            "Events should be generated for runbook creation"
        );

        // Verify all runbooks are in the workspace state
        {
            let manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_ref().unwrap();
            if let Some(workspace) = manager.workspaces.get("test-workspace") {
                if let Ok(state) = &workspace.state {
                    for i in 0..5 {
                        assert!(state.runbooks.contains_key(&format!("runbook-{i}")));
                    }
                } else {
                    panic!("Workspace state should be Ok");
                }
            } else {
                panic!("Workspace should exist");
            }
        }

        // Clean up
        {
            let mut manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_mut().unwrap();
            manager.unwatch_workspace("test-workspace").await.unwrap();
        }

        drop(temp_dir);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_complex_rename() {
        let (temp_dir, workspace_path) = setup_test_workspace().await;
        let collector = TestEventCollector::new();
        let manager = WorkspaceManager::new();
        let manager_arc = Arc::new(Mutex::new(Some(manager)));

        let subdir = workspace_path.join("subdir");
        tokio::fs::create_dir(&subdir).await.unwrap();

        create_gitignore(&subdir, &["test.atrb"]).await;

        // Start watching the workspace
        {
            let mut manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_mut().unwrap();
            manager
                .watch_workspace(
                    &workspace_path,
                    "test-workspace",
                    collector.create_handler(),
                    manager_arc.clone(),
                )
                .await
                .unwrap();
        }

        // Wait for initial state event
        let initial_events = collector.wait_for_events(1, 1000).await;
        assert!(!initial_events.is_empty());
        collector.clear_events().await;

        create_test_runbook(&workspace_path, "test", "test-runbook-id").await;
        create_test_runbook(&workspace_path, "test2", "test-runbook-id2").await;

        let events = collector.wait_for_events(1, 1000).await;
        assert!(!events.is_empty());
        collector.clear_events().await;

        {
            let manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_ref().unwrap();
            if let Some(workspace) = manager.workspaces.get("test-workspace") {
                if let Ok(state) = &workspace.state {
                    assert!(state.runbooks.contains_key("test-runbook-id"));
                    assert!(state.runbooks.contains_key("test-runbook-id2"));
                }
            }
        }

        // Rename makes this an ignored file
        tokio::fs::rename(&workspace_path.join("test.atrb"), &subdir.join("test.atrb"))
            .await
            .unwrap();
        tokio::fs::rename(
            &workspace_path.join("test2.atrb"),
            &subdir.join("test2.atrb"),
        )
        .await
        .unwrap();

        let events = collector.wait_for_events(1, 1000).await;
        assert!(!events.is_empty());

        {
            let manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_ref().unwrap();
            if let Some(workspace) = manager.workspaces.get("test-workspace") {
                if let Ok(state) = &workspace.state {
                    assert!(!state.runbooks.contains_key("test-runbook-id"));
                    assert!(state.runbooks.contains_key("test-runbook-id2"));
                }
            }
        }

        // Clean up
        {
            let mut manager_guard = manager_arc.lock().await;
            let manager = manager_guard.as_mut().unwrap();
            manager.unwatch_workspace("test-workspace").await.unwrap();
        }

        drop(temp_dir);
    }
}
