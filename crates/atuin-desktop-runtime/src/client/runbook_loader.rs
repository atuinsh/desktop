//! Runbook content loading trait
//!
//! This module provides the `RunbookContentLoader` trait which allows the runtime
//! to load runbook content on-demand without knowing the underlying storage mechanism.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Reference to a sub-runbook that can be resolved in different ways
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SubRunbookRef {
    /// UUID of the runbook (set by desktop app, used for workspace lookup)
    pub id: Option<String>,
    /// Hub URI: "hub.atuin.sh/user/runbook" or "user/runbook" or "user/runbook:tag"
    pub uri: Option<String>,
    /// File path (relative or absolute) for CLI use
    pub path: Option<String>,
}

impl SubRunbookRef {
    /// Check if any reference is set
    pub fn is_empty(&self) -> bool {
        self.id.is_none() && self.uri.is_none() && self.path.is_none()
    }

    /// Get a display-friendly identifier for error messages
    /// Prefers: uri > path > id
    pub fn display_id(&self) -> String {
        self.uri
            .as_ref()
            .or(self.path.as_ref())
            .or(self.id.as_ref())
            .cloned()
            .unwrap_or_else(|| "unknown".to_string())
    }
}

/// Trait for loading runbook content on-demand
///
/// This abstraction allows the runtime to load sub-runbooks without knowing
/// whether they come from a local workspace, cloud storage, or elsewhere.
///
/// Implementors decide how to interpret and resolve the reference based on
/// which fields are populated (id, uri, path).
#[async_trait]
pub trait RunbookContentLoader: Send + Sync {
    /// Load a runbook by reference
    ///
    /// # Arguments
    /// * `runbook_ref` - A reference to the runbook (can have id, uri, and/or path)
    ///
    /// # Returns
    /// The loaded runbook with its ID and content, or an error if not found
    async fn load_runbook(
        &self,
        runbook_ref: &SubRunbookRef,
    ) -> Result<LoadedRunbook, RunbookLoadError>;
}

/// Errors that can occur when loading runbook content
#[derive(Debug, Clone)]
pub enum RunbookLoadError {
    /// The runbook was not found
    NotFound { runbook_id: String },
    /// Failed to load the runbook
    LoadFailed { runbook_id: String, message: String },
}

impl std::fmt::Display for RunbookLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunbookLoadError::NotFound { runbook_id } => {
                write!(f, "Runbook not found: {}", runbook_id)
            }
            RunbookLoadError::LoadFailed {
                runbook_id,
                message,
            } => {
                write!(f, "Failed to load runbook {}: {}", runbook_id, message)
            }
        }
    }
}

impl std::error::Error for RunbookLoadError {}

/// Result of loading a runbook - contains both the ID and content
#[derive(Debug, Clone)]
pub struct LoadedRunbook {
    /// The runbook's unique identifier (UUID)
    pub id: Uuid,
    /// The runbook content as a JSON array of blocks
    pub content: Vec<serde_json::Value>,
}

#[cfg(test)]
pub struct MemoryRunbookContentLoader {
    runbooks: std::collections::HashMap<String, (Uuid, Vec<serde_json::Value>)>,
}

#[cfg(test)]
impl MemoryRunbookContentLoader {
    pub fn new() -> Self {
        Self {
            runbooks: std::collections::HashMap::new(),
        }
    }

    pub fn with_runbook(mut self, id: &str, content: Vec<serde_json::Value>) -> Self {
        // Generate a UUID for test runbooks, or parse if it's already a UUID
        let uuid = Uuid::parse_str(id).unwrap_or_else(|_| Uuid::new_v4());
        self.runbooks.insert(id.to_string(), (uuid, content));
        self
    }
}

#[cfg(test)]
#[async_trait]
impl RunbookContentLoader for MemoryRunbookContentLoader {
    async fn load_runbook(
        &self,
        runbook_ref: &SubRunbookRef,
    ) -> Result<LoadedRunbook, RunbookLoadError> {
        let display_id = runbook_ref.display_id();
        let (id, content) =
            self.runbooks
                .get(&display_id)
                .cloned()
                .ok_or_else(|| RunbookLoadError::NotFound {
                    runbook_id: display_id,
                })?;
        Ok(LoadedRunbook { id, content })
    }
}
