use std::path::PathBuf;

use atuin_desktop_runtime::client::{
    load_runbook_from_id, load_runbook_from_uri, DocumentBridgeMessage, HubClient, LoadedRunbook,
    LocalValueProvider, MessageChannel, RunbookContentLoader, RunbookLoadError, SubRunbookRef,
};
use atuin_desktop_runtime::context::{BlockContext, BlockContextStorage};
use atuin_desktop_runtime::events::{EventBus, GCEvent};
use tokio::sync::mpsc;
use uuid::Uuid;

pub struct NullEventBus;

#[async_trait::async_trait]
impl EventBus for NullEventBus {
    async fn emit(
        &self,
        _event: GCEvent,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

pub struct NullDocumentBridge;

#[async_trait::async_trait]
impl MessageChannel<DocumentBridgeMessage> for NullDocumentBridge {
    async fn send(
        &self,
        _message: DocumentBridgeMessage,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}
pub struct ChannelDocumentBridge {
    sender: mpsc::Sender<DocumentBridgeMessage>,
}

impl ChannelDocumentBridge {
    pub fn new(sender: mpsc::Sender<DocumentBridgeMessage>) -> Self {
        Self { sender }
    }
}

#[async_trait::async_trait]
impl MessageChannel<DocumentBridgeMessage> for ChannelDocumentBridge {
    async fn send(
        &self,
        message: DocumentBridgeMessage,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.sender.send(message).await.map_err(|e| e.into())
    }
}

pub struct TempNullLocalValueProvider;

#[async_trait::async_trait]
impl LocalValueProvider for TempNullLocalValueProvider {
    async fn get_block_local_value(
        &self,
        _block_id: Uuid,
        _property_name: &str,
    ) -> std::result::Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
}

pub struct TempNullContextStorage;

#[async_trait::async_trait]
impl BlockContextStorage for TempNullContextStorage {
    async fn save(
        &self,
        _document_id: &str,
        _block_id: &Uuid,
        _context: &BlockContext,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn load(
        &self,
        _document_id: &str,
        _block_id: &Uuid,
    ) -> std::result::Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    async fn delete(
        &self,
        _document_id: &str,
        _block_id: &Uuid,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn delete_for_document(
        &self,
        _runbook_id: &str,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

/// Runbook loader that resolves references as file paths relative to a base directory,
/// or fetches from Atuin Hub for remote references.
///
/// Resolution order:
/// 1. If `path` is set: Try as relative path, then absolute path
/// 2. If `uri` is set: Fetch from hub by NWO (user/runbook:tag)
/// 3. If `id` is set: Fetch from hub by ID
pub struct FileRunbookLoader {
    /// Base directory for resolving relative paths (typically the directory containing the parent runbook)
    base_dir: PathBuf,
    /// Hub API client for fetching remote runbooks
    hub_client: HubClient,
}

impl FileRunbookLoader {
    /// Create a loader from a runbook file path (uses the parent directory as base)
    pub fn from_runbook_path(runbook_path: &std::path::Path) -> Self {
        let base_dir = runbook_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        Self {
            base_dir,
            hub_client: HubClient::new(),
        }
    }

    /// Try to resolve a path (relative to base_dir or absolute)
    fn resolve_path(&self, path_str: &str) -> Option<PathBuf> {
        // Try relative path first
        let relative_path = self.base_dir.join(path_str);
        if relative_path.is_file() {
            return Some(relative_path);
        }

        // Try as absolute path
        let absolute_path = PathBuf::from(path_str);
        if absolute_path.is_file() {
            return Some(absolute_path);
        }

        None
    }

    /// Load runbook from a hub URI (user/runbook or user/runbook:tag)
    async fn load_from_uri(
        &self,
        uri: &str,
        display_id: &str,
    ) -> Result<LoadedRunbook, RunbookLoadError> {
        load_runbook_from_uri(&self.hub_client, uri, display_id).await
    }

    /// Load runbook from hub by ID
    async fn load_from_hub_id(
        &self,
        id: &str,
        display_id: &str,
    ) -> Result<LoadedRunbook, RunbookLoadError> {
        load_runbook_from_id(&self.hub_client, id, display_id).await
    }

    /// Load runbook from a file path
    async fn load_from_path(
        &self,
        path: &PathBuf,
        display_id: &str,
    ) -> Result<LoadedRunbook, RunbookLoadError> {
        // Read and parse the file
        let file_content =
            tokio::fs::read_to_string(path)
                .await
                .map_err(|e| RunbookLoadError::LoadFailed {
                    runbook_id: display_id.to_string(),
                    message: format!("Failed to read file: {}", e),
                })?;

        // Parse YAML (which is a superset of JSON)
        let yaml_value: serde_yaml::Value =
            serde_yaml::from_str(&file_content).map_err(|e| RunbookLoadError::LoadFailed {
                runbook_id: display_id.to_string(),
                message: format!("Failed to parse YAML: {}", e),
            })?;

        let json_value: serde_json::Value =
            serde_yaml::from_value(yaml_value).map_err(|e| RunbookLoadError::LoadFailed {
                runbook_id: display_id.to_string(),
                message: format!("Failed to convert to JSON: {}", e),
            })?;

        // Try to get runbook ID from file, otherwise generate one
        let id = json_value
            .get("id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| RunbookLoadError::LoadFailed {
                runbook_id: display_id.to_string(),
                message: format!("Runbook at path {:?} has no valid 'id' field", path),
            })?;

        // Extract content array
        let content = json_value
            .get("content")
            .and_then(|v| v.as_array())
            .cloned()
            .ok_or_else(|| RunbookLoadError::LoadFailed {
                runbook_id: display_id.to_string(),
                message: "Runbook file missing 'content' array".to_string(),
            })?;

        Ok(LoadedRunbook { id, content })
    }
}

#[async_trait::async_trait]
impl RunbookContentLoader for FileRunbookLoader {
    async fn load_runbook(
        &self,
        runbook_ref: &SubRunbookRef,
    ) -> Result<LoadedRunbook, RunbookLoadError> {
        let display_id = runbook_ref.display_id();

        // 1. Try path first (most specific for CLI use)
        if let Some(path_str) = &runbook_ref.path {
            if let Some(resolved_path) = self.resolve_path(path_str) {
                return self.load_from_path(&resolved_path, &display_id).await;
            }
            // Path was specified but not found - fail early with helpful message
            return Err(RunbookLoadError::NotFound {
                runbook_id: format!("{} (path not found: {})", display_id, path_str),
            });
        }

        // 2. Try URI (hub fetch by NWO)
        if let Some(uri) = &runbook_ref.uri {
            return self.load_from_uri(uri, &display_id).await;
        }

        // 3. Try ID (hub fetch by ID)
        if let Some(id) = &runbook_ref.id {
            return self.load_from_hub_id(id, &display_id).await;
        }

        // No reference provided
        Err(RunbookLoadError::NotFound {
            runbook_id: "No runbook reference provided".to_string(),
        })
    }
}
