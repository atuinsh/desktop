use tauri::ipc::Channel;
use uuid::Uuid;

use crate::ai::session::SessionEvent;
use crate::ai::types::{BlockInfo, ChargeTarget, ModelSelection, SessionConfig};
use crate::state::AtuinState;

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
    model: Option<ModelSelection>,
    block_infos: Vec<BlockInfo>,
    desktop_username: String,
    charge_target: ChargeTarget,
    hub_endpoint: String,
    restore_previous: bool,
) -> Result<Uuid, String> {
    let ai_manager = state.ai_manager().await;

    let model = model.unwrap_or(ModelSelection::AtuinHub {
        model: "claude-opus-4-5-20251101".to_string(),
        uri: Some(hub_endpoint),
    });

    let handle = ai_manager
        .create_chat_session(
            runbook_id,
            block_infos,
            SessionConfig {
                model,
                desktop_username,
                charge_target,
            },
            restore_previous,
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(handle.id)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn ai_create_generator_session(
    state: tauri::State<'_, AtuinState>,
    runbook_id: Uuid,
    model: Option<ModelSelection>,
    block_infos: Vec<BlockInfo>,
    current_document: serde_json::Value,
    insert_after: Uuid,
    desktop_username: String,
    charge_target: ChargeTarget,
    hub_endpoint: String,
) -> Result<Uuid, String> {
    let ai_manager = state.ai_manager().await;
    let handle = ai_manager
        .create_generator_session(
            runbook_id,
            block_infos,
            current_document,
            insert_after,
            SessionConfig {
                model: model.unwrap_or(ModelSelection::AtuinHub {
                    model: "claude-opus-4-5-20251101".to_string(),
                    uri: Some(hub_endpoint),
                }),
                desktop_username,
                charge_target,
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(handle.id)
}

/// Subscribe to events from an AI session.
/// If the session was restored, history will be replayed immediately.
#[tauri::command]
pub async fn ai_subscribe_session(
    state: tauri::State<'_, AtuinState>,
    session_id: Uuid,
    channel: Channel<SessionEvent>,
) -> Result<(), String> {
    let ai_manager = state.ai_manager().await;
    ai_manager
        .subscribe(session_id, channel)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Change the model of an AI session.
#[tauri::command]
pub async fn ai_change_model(
    state: tauri::State<'_, AtuinState>,
    session_id: Uuid,
    model: ModelSelection,
) -> Result<(), String> {
    let ai_manager = state.ai_manager().await;
    let handle = ai_manager
        .get_handle(session_id)
        .await
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    handle.change_model(model).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_change_charge_target(
    state: tauri::State<'_, AtuinState>,
    session_id: Uuid,
    charge_target: ChargeTarget,
) -> Result<(), String> {
    let ai_manager = state.ai_manager().await;
    let handle = ai_manager
        .get_handle(session_id)
        .await
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    handle
        .change_charge_target(charge_target)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_change_user(
    state: tauri::State<'_, AtuinState>,
    session_id: Uuid,
    user: String,
) -> Result<(), String> {
    let ai_manager = state.ai_manager().await;
    let handle = ai_manager
        .get_handle(session_id)
        .await
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    handle.change_user(user).await.map_err(|e| e.to_string())
}

/// Send a user message to an AI session.
#[tauri::command]
pub async fn ai_send_message(
    state: tauri::State<'_, AtuinState>,
    session_id: Uuid,
    message: String,
) -> Result<(), String> {
    let ai_manager = state.ai_manager().await;
    let handle = ai_manager
        .get_handle(session_id)
        .await
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
    let ai_manager = state.ai_manager().await;
    let handle = ai_manager
        .get_handle(session_id)
        .await
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
    let ai_manager = state.ai_manager().await;
    let handle = ai_manager
        .get_handle(session_id)
        .await
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    handle.cancel().await.map_err(|e| e.to_string())
}

/// Destroy an AI session and clean up resources.
#[tauri::command]
pub async fn ai_destroy_session(
    state: tauri::State<'_, AtuinState>,
    session_id: Uuid,
) -> Result<(), String> {
    let ai_manager = state.ai_manager().await;
    ai_manager.destroy(session_id).await;
    Ok(())
}

/// Send an edit request to an InlineBlockGeneration session.
/// This continues the conversation after submit_blocks with the user's edit instructions.
#[tauri::command]
pub async fn ai_send_edit_request(
    state: tauri::State<'_, AtuinState>,
    session_id: Uuid,
    edit_prompt: String,
    tool_call_id: String,
) -> Result<(), String> {
    let ai_manager = state.ai_manager().await;
    let handle = ai_manager
        .get_handle(session_id)
        .await
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    handle
        .send_edit_request(edit_prompt, tool_call_id)
        .await
        .map_err(|e| e.to_string())
}
