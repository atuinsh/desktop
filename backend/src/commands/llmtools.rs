use tauri::ipc::Channel;

use crate::ai::types::{LLMToolsEvent, SessionInfo};
use crate::state::AtuinState;

/// List all active AI sessions.
#[tauri::command]
pub async fn llmtools_list_sessions(
    state: tauri::State<'_, AtuinState>,
) -> Result<Vec<SessionInfo>, String> {
    let ai_manager = state.ai_manager().await;
    Ok(ai_manager.list_sessions().await)
}

/// Subscribe to LLM Tools events (session creation, destruction, and session events).
/// Events are streamed to the provided channel.
#[tauri::command]
pub async fn llmtools_subscribe(
    state: tauri::State<'_, AtuinState>,
    channel: Channel<LLMToolsEvent>,
) -> Result<(), String> {
    let ai_manager = state.ai_manager().await;
    let mut rx = ai_manager.subscribe_llmtools();

    // Spawn a task to forward broadcast events to the Tauri channel
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Err(e) = channel.send(event) {
                        log::debug!("LLM Tools channel closed: {}", e);
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    log::debug!("LLM Tools broadcast channel closed");
                    break;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    log::warn!("LLM Tools subscriber lagged by {} events", n);
                    // Continue receiving; we just missed some events
                }
            }
        }
    });

    Ok(())
}
