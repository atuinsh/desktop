use std::collections::HashMap;
use std::sync::Arc;

use tauri::async_runtime::RwLock;
use tauri::ipc::Channel;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::ai::fsm::State as FsmState;
use crate::ai::session::{AISession, ChargeTarget, SessionEvent};
use crate::ai::storage::AISessionStorage;
use crate::ai::types::{AIMessage, AIToolCall, ModelSelection};
use crate::state::AtuinState;

/// Data to replay to frontend after subscription for a restored session
struct PendingReplay {
    fsm_state: FsmState,
    history: Vec<AIMessage>,
    pending_tools: Vec<AIToolCall>,
}

// Track sessions that have pending replay data after subscription
lazy_static::lazy_static! {
    static ref PENDING_REPLAY: Arc<RwLock<HashMap<Uuid, PendingReplay>>> = Arc::new(RwLock::new(HashMap::new()));
}

/// Create or restore an AI session for a runbook.
/// Returns the session ID.
///
/// If `restore_previous` is true, attempts to restore the most recent session for this runbook.
/// If false, always creates a fresh session.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn ai_create_session(
    state: tauri::State<'_, AtuinState>,
    runbook_id: Uuid,
    block_types: Vec<String>,
    block_summary: String,
    desktop_username: String,
    charge_target: ChargeTarget,
    hub_endpoint: String,
    restore_previous: bool,
) -> Result<Uuid, String> {
    // Create storage from db pool
    let pool = state
        .db_instances
        .get_pool("ai")
        .await
        .map_err(|e| format!("Failed to get AI database pool: {}", e))?;
    let storage = Arc::new(AISessionStorage::new(pool));

    // Check for existing session for this runbook (only if restore requested)
    let existing = if restore_previous {
        storage
            .find_most_recent_for_runbook(&runbook_id)
            .await
            .map_err(|e| format!("Failed to check for existing session: {}", e))?
    } else {
        None
    };

    // Create output channel for session events
    let (output_tx, mut output_rx) = mpsc::channel::<SessionEvent>(32);

    // TODO: Get model selection from settings/frontend
    let default_model = ModelSelection::AtuinHub {
        model: "claude-opus-4-5-20251101".to_string(),
        uri: Some(hub_endpoint),
    };

    let (session, handle, replay_data) = if let Some(saved) = existing {
        log::info!(
            "Restoring AI session {} for runbook {}",
            saved.id,
            runbook_id
        );

        // Capture state before consuming saved
        let fsm_state = saved.agent_state.clone();

        log::debug!(
            "Restoring session - fsm_state: {:?}, pending_tools keys: {:?}",
            fsm_state,
            saved.agent_context.pending_tools.keys().collect::<Vec<_>>()
        );

        // Get history before restoring (so we can replay it to frontend)
        let history: Vec<AIMessage> = saved
            .agent_context
            .conversation
            .iter()
            .map(|msg| AIMessage::from(msg.clone()))
            .collect();

        // Get pending tool calls if session was in PendingTools state
        let pending_tools: Vec<AIToolCall> = saved
            .agent_context
            .pending_tools
            .values()
            .cloned()
            .map(AIToolCall::from)
            .collect();

        log::debug!(
            "Extracted {} pending tool calls with IDs: {:?}",
            pending_tools.len(),
            pending_tools.iter().map(|t| &t.id).collect::<Vec<_>>()
        );

        let (session, handle) = AISession::from_saved(
            saved,
            output_tx,
            block_types,
            block_summary,
            desktop_username,
            charge_target,
            state.secret_cache(),
            storage,
        );

        let replay = PendingReplay {
            fsm_state,
            history,
            pending_tools,
        };
        (session, handle, Some(replay))
    } else {
        log::info!("Creating new AI session for runbook {}", runbook_id);

        let (session, handle) = AISession::new(
            runbook_id,
            default_model,
            output_tx,
            block_types,
            block_summary,
            desktop_username,
            charge_target,
            state.secret_cache(),
            storage,
        );

        (session, handle, None)
    };

    let session_id = session.id();

    // Store pending replay data for after subscription
    if let Some(replay) = replay_data {
        if !replay.history.is_empty() || !replay.pending_tools.is_empty() {
            PENDING_REPLAY.write().await.insert(session_id, replay);
        }
    }

    // Store the handle
    state.ai_sessions.write().await.insert(session_id, handle);

    // Spawn the session event loop
    tokio::spawn(session.run());

    // Spawn a task to forward events to the frontend channel (once subscribed)
    let ai_session_channels = state.ai_session_channels.clone();
    let sessions = state.ai_sessions.clone();
    tokio::spawn(async move {
        while let Some(event) = output_rx.recv().await {
            let channels = ai_session_channels.read().await;
            if let Some(channel) = channels.get(&session_id) {
                if let Err(e) = channel.send(event) {
                    log::error!("Failed to send session event to frontend: {}", e);
                    break;
                }
            }
            // If no channel subscribed yet, events are dropped
            // This is fine - the frontend will subscribe shortly after creation
        }

        // Session ended, clean up
        log::debug!("Session {} output channel closed, cleaning up", session_id);
        sessions.write().await.remove(&session_id);
        ai_session_channels.write().await.remove(&session_id);
    });

    log::info!("Created/restored AI session {}", session_id);
    Ok(session_id)
}

/// Subscribe to events from an AI session.
/// If the session was restored, history will be replayed immediately.
#[tauri::command]
pub async fn ai_subscribe_session(
    state: tauri::State<'_, AtuinState>,
    session_id: Uuid,
    channel: Channel<SessionEvent>,
) -> Result<(), String> {
    // Verify session exists
    let sessions = state.ai_sessions.read().await;
    if !sessions.contains_key(&session_id) {
        return Err(format!("Session {} not found", session_id));
    }
    drop(sessions);

    // Store the channel
    state
        .ai_session_channels
        .write()
        .await
        .insert(session_id, channel.clone());

    // Check for pending replay data (restored session) or send defaults (fresh session)
    let replay = PENDING_REPLAY.write().await.remove(&session_id);

    // Extract data from replay or use defaults
    let fsm_state = replay
        .as_ref()
        .map(|r| r.fsm_state.clone())
        .unwrap_or(FsmState::Idle);
    let history = replay
        .as_ref()
        .map(|r| r.history.clone())
        .unwrap_or_default();
    let pending_tool_calls = replay.map(|r| r.pending_tools).unwrap_or_default();

    log::debug!(
        "Sending state {:?}, {} history messages, and {} pending tool calls for session {}",
        fsm_state,
        history.len(),
        pending_tool_calls.len(),
        session_id
    );

    // Send current FSM state so frontend knows if session is idle, pending tools, etc.
    if let Err(e) = channel.send(SessionEvent::StateChanged { state: fsm_state }) {
        log::error!("Failed to send state to frontend: {}", e);
    }

    // Send history and pending tool calls
    if let Err(e) = channel.send(SessionEvent::History {
        messages: history,
        pending_tool_calls,
    }) {
        log::error!("Failed to send history to frontend: {}", e);
    }

    log::debug!("Frontend subscribed to session {}", session_id);
    Ok(())
}

/// Change the model of an AI session.
#[tauri::command]
pub async fn ai_change_model(
    state: tauri::State<'_, AtuinState>,
    session_id: Uuid,
    model: ModelSelection,
) -> Result<(), String> {
    let sessions = state.ai_sessions.read().await;
    let handle = sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    handle.change_model(model).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_change_charge_target(
    state: tauri::State<'_, AtuinState>,
    session_id: Uuid,
    charge_target: ChargeTarget,
) -> Result<(), String> {
    let sessions = state.ai_sessions.read().await;
    let handle = sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    handle
        .change_charge_target(charge_target)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn ai_change_user(
    state: tauri::State<'_, AtuinState>,
    session_id: Uuid,
    user: String,
) -> Result<(), String> {
    let sessions = state.ai_sessions.read().await;
    let handle = sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    handle.change_user(user).await.map_err(|e| e.to_string())?;

    Ok(())
}

/// Send a user message to an AI session.
#[tauri::command]
pub async fn ai_send_message(
    state: tauri::State<'_, AtuinState>,
    session_id: Uuid,
    message: String,
) -> Result<(), String> {
    let sessions = state.ai_sessions.read().await;
    let handle = sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    handle
        .send_user_message(message)
        .await
        .map_err(|e| e.to_string())
}

/// Send a tool result to an AI session.
#[tauri::command]
pub async fn ai_send_tool_result(
    state: tauri::State<'_, AtuinState>,
    session_id: Uuid,
    tool_call_id: String,
    success: bool,
    result: String,
) -> Result<(), String> {
    let sessions = state.ai_sessions.read().await;
    let handle = sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    handle
        .send_tool_result(tool_call_id, success, result)
        .await
        .map_err(|e| e.to_string())
}

/// Cancel the current operation in an AI session.
#[tauri::command]
pub async fn ai_cancel_session(
    state: tauri::State<'_, AtuinState>,
    session_id: Uuid,
) -> Result<(), String> {
    let sessions = state.ai_sessions.read().await;
    let handle = sessions
        .get(&session_id)
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    handle.cancel().await.map_err(|e| e.to_string())
}

/// Destroy an AI session and clean up resources.
#[tauri::command]
pub async fn ai_destroy_session(
    state: tauri::State<'_, AtuinState>,
    session_id: Uuid,
) -> Result<(), String> {
    // Remove handle (this will cause the session's event channel to close,
    // which will end the session's run loop)
    let removed = state.ai_sessions.write().await.remove(&session_id);

    if removed.is_none() {
        return Err(format!("Session {} not found", session_id));
    }

    // Remove frontend channel
    state.ai_session_channels.write().await.remove(&session_id);

    // Clean up any pending history
    PENDING_REPLAY.write().await.remove(&session_id);

    log::info!("Destroyed AI session {}", session_id);
    Ok(())
}

/// Get the conversation history from an AI session.
#[tauri::command]
pub async fn ai_get_history(
    _state: tauri::State<'_, AtuinState>,
    _session_id: Uuid,
) -> Result<Vec<AIMessage>, String> {
    // History is now replayed automatically via SessionEvent::History
    // This endpoint is kept for potential future use
    Err("Use session subscription for history replay".to_string())
}
