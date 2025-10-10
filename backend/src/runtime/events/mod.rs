use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::pty::PtyMetadata;

/// Grand Central Event - all events that can be emitted by the runtime
#[derive(TS, Debug, Clone, Serialize, Deserialize)]
#[ts(tag = "type", content = "data", export)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum GCEvent {
    /// PTY was opened and is ready for use
    PtyOpened(PtyMetadata),

    /// PTY was closed
    PtyClosed { pty_id: Uuid },

    /// Block execution started
    BlockStarted { block_id: Uuid, runbook_id: Uuid },

    /// Block execution finished
    BlockFinished {
        block_id: Uuid,
        runbook_id: Uuid,
        success: bool,
    },

    /// Block execution failed
    BlockFailed {
        block_id: Uuid,
        runbook_id: Uuid,
        error: String,
    },

    /// Block execution was cancelled
    BlockCancelled { block_id: Uuid, runbook_id: Uuid },

    /// SSH connection established
    SshConnected {
        host: String,
        username: Option<String>,
    },

    /// SSH connection failed
    SshConnectionFailed { host: String, error: String },

    /// SSH connection closed
    SshDisconnected { host: String },

    /// Runbook execution started
    RunbookStarted { runbook_id: Uuid },

    /// Runbook execution completed
    RunbookCompleted { runbook_id: Uuid },

    /// Runbook execution failed
    RunbookFailed { runbook_id: Uuid, error: String },
}

/// Trait for emitting events from the runtime layer
/// This abstracts away the actual event delivery mechanism
#[async_trait]
pub trait EventBus: Send + Sync {
    /// Emit an event to the event bus
    async fn emit(&self, event: GCEvent) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// No-op event bus for testing or when events are not needed
#[allow(dead_code)]
pub struct NoOpEventBus;

#[async_trait]
impl EventBus for NoOpEventBus {
    async fn emit(&self, _event: GCEvent) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Do nothing
        Ok(())
    }
}

/// Event bus that collects events in memory (useful for testing)
#[allow(dead_code)]
#[derive(Default)]
pub struct MemoryEventBus {
    events: std::sync::Arc<std::sync::Mutex<Vec<GCEvent>>>,
}

#[allow(dead_code)]
impl MemoryEventBus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn events(&self) -> Vec<GCEvent> {
        self.events.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.events.lock().unwrap().clear();
    }
}

#[async_trait]
impl EventBus for MemoryEventBus {
    async fn emit(&self, event: GCEvent) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}
