use serde::{Deserialize, Serialize};
use tauri::Emitter;

use atuin_desktop_runtime::blocks::Block;
use atuin_desktop_runtime::workflow::WorkflowEvent;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ExecLogCompletedEvent {
    pub block_id: String,
    pub start_time: u64,
    pub end_time: u64,
    pub output: String,
}

#[tauri::command]
pub async fn log_execution(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::state::AtuinState>,
    block: Block,
    start_time: u64,
    end_time: u64,
    output: String,
) -> Result<(), String> {
    log::debug!("Logging execution for block: {block:?}");
    state
        .exec_log()
        .log_execution(block.clone(), start_time, end_time, output.clone())
        .await
        .map_err(|e| e.to_string())?;

    let event_sender = state.event_sender();
    event_sender
        .send(WorkflowEvent::BlockFinished { id: block.id() })
        .expect("Failed to send stop block event");

    app.emit(
        format!("exec_log_completed:{}", block.id()).as_str(),
        ExecLogCompletedEvent {
            block_id: block.id().to_string(),
            start_time,
            end_time,
            output,
        },
    )
    .map_err(|e| e.to_string())
}
