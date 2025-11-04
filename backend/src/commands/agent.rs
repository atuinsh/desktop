use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex as TokioMutex};
use uuid::Uuid;

// ============================================================================
// Agent Session Registry - Global state for routing messages to agents
// ============================================================================

#[derive(Clone)]
pub struct AgentSessionHandle {
    pub user_message_tx: mpsc::Sender<String>,
    pub hitl_request_tx: mpsc::Sender<HitlRequest>,
    pub hitl_responses: Arc<TokioMutex<HashMap<String, oneshot::Sender<HitlResponse>>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HitlRequest {
    pub id: String,
    pub prompt: String,
    pub options: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HitlResponse {
    pub decision: String,
    pub data: Option<serde_json::Value>,
}

pub struct AgentSessionRegistry {
    sessions: Arc<TokioMutex<HashMap<(Uuid, Uuid), AgentSessionHandle>>>,
}

impl AgentSessionRegistry {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(TokioMutex::new(HashMap::new())),
        }
    }

    pub async fn register(
        &self,
        runbook_id: Uuid,
        block_id: Uuid,
        user_message_tx: mpsc::Sender<String>,
        hitl_request_tx: mpsc::Sender<HitlRequest>,
        hitl_responses: Arc<TokioMutex<HashMap<String, oneshot::Sender<HitlResponse>>>>,
    ) {
        let handle = AgentSessionHandle {
            user_message_tx,
            hitl_request_tx,
            hitl_responses,
        };
        self.sessions
            .lock()
            .await
            .insert((runbook_id, block_id), handle);
        log::debug!(
            "Registered agent session for runbook {} block {}",
            runbook_id,
            block_id
        );
    }

    pub async fn unregister(&self, runbook_id: Uuid, block_id: Uuid) {
        self.sessions.lock().await.remove(&(runbook_id, block_id));
        log::debug!(
            "Unregistered agent session for runbook {} block {}",
            runbook_id,
            block_id
        );
    }

    pub async fn get_session(
        &self,
        runbook_id: Uuid,
        block_id: Uuid,
    ) -> Option<AgentSessionHandle> {
        self.sessions
            .lock()
            .await
            .get(&(runbook_id, block_id))
            .cloned()
    }
}

impl Default for AgentSessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tauri Commands
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageRequest {
    pub message: String,
}

#[tauri::command]
pub async fn agent_send_message(
    state: tauri::State<'_, crate::state::AtuinState>,
    runbook_id: String,
    block_id: String,
    request: SendMessageRequest,
) -> Result<(), String> {
    let runbook_uuid = Uuid::parse_str(&runbook_id).map_err(|e| e.to_string())?;
    let block_uuid = Uuid::parse_str(&block_id).map_err(|e| e.to_string())?;

    let session = state.agent_session_registry
        .get_session(runbook_uuid, block_uuid)
        .await
        .ok_or_else(|| {
            log::error!("No agent session found for runbook {} block {}", runbook_id, block_id);
            format!("No agent session found for block {}. Make sure the block has been executed first.", block_id)
        })?;

    log::debug!("Sending message to agent session for block {}", block_id);
    session
        .user_message_tx
        .send(request.message)
        .await
        .map_err(|e| format!("Failed to send message to agent: {}", e))?;

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveHitlRequest {
    pub decision: String,
    pub data: Option<serde_json::Value>,
}

#[tauri::command]
pub async fn agent_resolve_hitl(
    state: tauri::State<'_, crate::state::AtuinState>,
    runbook_id: String,
    block_id: String,
    hitl_id: String,
    request: ResolveHitlRequest,
) -> Result<(), String> {
    let runbook_uuid = Uuid::parse_str(&runbook_id).map_err(|e| e.to_string())?;
    let block_uuid = Uuid::parse_str(&block_id).map_err(|e| e.to_string())?;

    let session = state.agent_session_registry
        .get_session(runbook_uuid, block_uuid)
        .await
        .ok_or_else(|| format!("No agent session found for block {}", block_id))?;

    let response_tx = session
        .hitl_responses
        .lock()
        .await
        .remove(&hitl_id)
        .ok_or_else(|| format!("No pending HITL request with id {}", hitl_id))?;

    let response = HitlResponse {
        decision: request.decision,
        data: request.data,
    };

    response_tx
        .send(response)
        .map_err(|_| "Failed to send HITL response".to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn agent_cancel_session(
    state: tauri::State<'_, crate::state::AtuinState>,
    runbook_id: String,
    block_id: String,
) -> Result<(), String> {
    let runbook_uuid = Uuid::parse_str(&runbook_id).map_err(|e| e.to_string())?;
    let block_uuid = Uuid::parse_str(&block_id).map_err(|e| e.to_string())?;

    // The cancellation is handled by the block's CancellationToken
    // This command just signals that we want to cancel
    // TODO: Trigger actual cancellation via the execution handle
    
    state.agent_session_registry.unregister(runbook_uuid, block_uuid).await;

    Ok(())
}
