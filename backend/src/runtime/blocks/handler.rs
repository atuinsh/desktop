use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{ipc::Channel, AppHandle};
use tokio::sync::{broadcast, oneshot, RwLock};
use uuid::Uuid;

use crate::runtime::workflow::event::WorkflowEvent;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub runbook_id: Uuid,
    pub cwd: String,
    pub env: HashMap<String, String>,
    pub variables: HashMap<String, String>,
    pub ssh_host: Option<String>,
    pub document: Vec<serde_json::Value>, // For template resolution
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self {
            runbook_id: Uuid::new_v4(),
            cwd: std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            env: HashMap::new(),
            variables: HashMap::new(),
            ssh_host: None,
            document: Vec::new(),
        }
    }
}

// Channel-based cancellation token
#[derive(Clone)]
pub struct CancellationToken {
    sender: Arc<std::sync::Mutex<Option<oneshot::Sender<()>>>>,
    receiver: Arc<std::sync::Mutex<Option<oneshot::Receiver<()>>>>,
}

impl CancellationToken {
    pub fn new() -> Self {
        let (sender, receiver) = oneshot::channel();
        Self {
            sender: Arc::new(std::sync::Mutex::new(Some(sender))),
            receiver: Arc::new(std::sync::Mutex::new(Some(receiver))),
        }
    }
    
    pub fn cancel(&self) {
        if let Ok(mut sender_guard) = self.sender.lock() {
            if let Some(sender) = sender_guard.take() {
                let _ = sender.send(()); // Ignore error if receiver already dropped
            }
        }
    }
    
    pub fn take_receiver(&self) -> Option<oneshot::Receiver<()>> {
        if let Ok(mut receiver_guard) = self.receiver.lock() {
            receiver_guard.take()
        } else {
            None
        }
    }
}

#[derive(Clone)]
pub struct ExecutionHandle {
    pub id: Uuid,
    #[allow(dead_code)] // Used for tracking but not currently accessed
    pub block_id: Uuid,
    pub cancellation_token: CancellationToken,
    pub status: Arc<RwLock<ExecutionStatus>>,
    pub output_variable: Option<String>,
}

#[derive(Clone, Debug)]
pub enum ExecutionStatus {
    Running,
    Success(String), // The output value
    #[allow(dead_code)] // Error message is used but compiler doesn't see reads
    Failed(String),  // Error message
    #[allow(dead_code)] // Used for cancellation but not currently constructed in tests
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockOutput {
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub lifecycle: Option<BlockLifecycleEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlockLifecycleEvent {
    Started,
    Finished { exit_code: Option<i32>, success: bool },
    Cancelled,
    Error { message: String },
}

#[async_trait]
pub trait BlockHandler: Send + Sync {
    type Block: Send + Sync;

    #[allow(dead_code)] // Used for identification but not currently called
    fn block_type(&self) -> &'static str;
    
    /// Get the output variable name from the block if it has one
    #[allow(dead_code)] // Used for output variable extraction but not currently called
    fn output_variable(&self, block: &Self::Block) -> Option<String>;

    async fn execute(
        &self,
        block: Self::Block,
        context: ExecutionContext,
        event_sender: broadcast::Sender<WorkflowEvent>,
        output_channel: Option<Channel<BlockOutput>>,
        app_handle: AppHandle,
    ) -> Result<ExecutionHandle, Box<dyn std::error::Error + Send + Sync>>;

    #[allow(dead_code)] // Used for cancellation but not currently called directly
    async fn cancel(
        &self,
        handle: &ExecutionHandle,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        handle.cancellation_token.cancel();
        Ok(())
    }
}

#[async_trait]
pub trait ContextProvider: Send + Sync {
    type Block: Send + Sync;

    #[allow(dead_code)] // Used for identification but not currently called
    fn block_type(&self) -> &'static str;

    #[allow(dead_code)] // Used by context builder but not currently called directly
    fn apply_context(
        &self,
        block: &Self::Block,
        context: &mut ExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}