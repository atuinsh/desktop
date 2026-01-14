use tauri::ipc::Channel;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::ai::session::{AISession, ChargeTarget, SessionEvent};
use crate::ai::types::{AIMessage, ModelSelection};
use crate::state::AtuinState;

/// Create a new AI session.
/// Returns the session ID.
#[tauri::command]
pub async fn ai_create_session(
    state: tauri::State<'_, AtuinState>,
    block_types: Vec<String>,
    block_summary: String,
    desktop_username: String,
    charge_target: ChargeTarget,
) -> Result<Uuid, String> {
    // Create output channel for session events
    let (output_tx, mut output_rx) = mpsc::channel::<SessionEvent>(32);

    // TODO: Get model selection from settings/frontend
    let model = ModelSelection::AtuinHub {
        model: "claude-opus-4-5-20251101".to_string(),
        uri: Some("http://localhost:4000/api/ai/proxy/".to_string()),
    };

    // Create the session
    let (session, handle) = AISession::new(
        model,
        output_tx,
        block_types,
        block_summary,
        desktop_username,
        charge_target,
        state.secret_cache(),
    );
    let session_id = session.id();

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

    log::info!("Created AI session {}", session_id);
    Ok(session_id)
}

/// Subscribe to events from an AI session.
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
        .insert(session_id, channel);

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

    log::info!("Destroyed AI session {}", session_id);
    Ok(())
}

/// Get the conversation history from an AI session.
#[tauri::command]
pub async fn ai_get_history(
    state: tauri::State<'_, AtuinState>,
    session_id: Uuid,
) -> Result<Vec<AIMessage>, String> {
    // TODO: We need a way to read from the session's context
    // For now, return empty - we'll need to add a method to SessionHandle
    // or store conversation separately
    Err("Not yet implemented".to_string())
}
