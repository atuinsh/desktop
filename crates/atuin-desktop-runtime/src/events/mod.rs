//! Event system for runtime monitoring
//!
//! This module provides an event bus abstraction for monitoring runtime execution.
//! Events include block lifecycle, SSH connections, PTY operations, and runbook
//! execution state changes.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::pty::PtyMetadata;

/// Events emitted by the runtime for monitoring and telemetry
///
/// These events provide visibility into runtime operations including block execution,
/// SSH connections, PTY lifecycle, and runbook state changes.
#[derive(TS, Debug, Clone, Serialize, Deserialize)]
#[ts(tag = "type", content = "data", export)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum GCEvent {
    /// Serial execution started
    SerialExecutionStarted { runbook_id: Uuid },

    /// Serial execution completed
    SerialExecutionCompleted { runbook_id: Uuid },

    /// Serial execution cancelled
    SerialExecutionCancelled { runbook_id: Uuid },

    /// Serial execution failed
    SerialExecutionFailed { runbook_id: Uuid, error: String },

    /// Serial execution paused at a pause block
    SerialExecutionPaused { runbook_id: Uuid, block_id: Uuid },

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

    /// SSH certificate file exists but failed to load (likely corrupted or invalid)
    /// This is a warning - authentication will fall back to key-based auth
    SshCertificateLoadFailed {
        host: String,
        cert_path: String,
        error: String,
    },

    /// SSH certificate has expired
    /// This is a warning - authentication fell back to key-based auth
    SshCertificateExpired {
        host: String,
        cert_path: String,
        valid_until: String,
    },

    /// SSH certificate is not yet valid
    /// This is a warning - authentication fell back to key-based auth
    SshCertificateNotYetValid {
        host: String,
        cert_path: String,
        valid_from: String,
    },

    /// Runbook execution started
    RunbookStarted { runbook_id: Uuid },

    /// Runbook execution completed
    RunbookCompleted { runbook_id: Uuid },

    /// Runbook execution failed
    RunbookFailed { runbook_id: Uuid, error: String },
}

/// Trait for emitting events from the runtime
///
/// Implementations of this trait handle the delivery of runtime events
/// to monitoring systems, logs, or other consumers.
#[async_trait]
pub trait EventBus: Send + Sync {
    /// Emit an event to the event bus
    ///
    /// # Arguments
    /// * `event` - The event to emit
    ///
    /// # Errors
    /// Returns an error if the event cannot be emitted
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

/// Event bus that collects events in memory
///
/// Useful for testing or scenarios where events need to be collected
/// and inspected programmatically.
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
