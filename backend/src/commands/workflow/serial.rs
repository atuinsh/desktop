use tauri::State;
use uuid::Uuid;

use crate::state::AtuinState;
use atuin_desktop_runtime::blocks::Block;
use atuin_desktop_runtime::workflow::WorkflowEvent;

#[tauri::command]
pub async fn workflow_block_start_event(
    state: State<'_, AtuinState>,
    block: Uuid,
) -> Result<(), String> {
    let event_sender = state.event_sender();
    event_sender
        .send(WorkflowEvent::BlockStarted { id: block })
        .expect("Failed to send start block event");

    Ok(())
}

#[tauri::command]
pub async fn workflow_serial(
    state: State<'_, AtuinState>,
    id: Uuid,
    workflow: Vec<Block>,
) -> Result<(), String> {
    println!("workflow_serial command received");
    state.executor().run_workflow(id, workflow).await;

    Ok(())
}

#[tauri::command]
pub async fn workflow_stop(state: State<'_, AtuinState>, id: Uuid) -> Result<(), String> {
    state.executor().stop_workflow(id).await;

    Ok(())
}
