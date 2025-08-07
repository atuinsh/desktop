use tauri::{ipc::Channel, AppHandle, Manager, State};
use uuid::Uuid;

use crate::commands::events::ChannelEventBus;
use crate::runtime::blocks::handler::BlockOutput;
use crate::runtime::blocks::registry::BlockRegistry;
use crate::runtime::blocks::Block;
use crate::runtime::workflow::context_builder::ContextBuilder;
use crate::state::AtuinState;

/// Convert editor document block to runtime Block enum
fn document_to_block(block_data: &serde_json::Value) -> Result<Block, String> {
    Block::from_document(block_data)
}

#[tauri::command]
pub async fn execute_block(
    state: State<'_, AtuinState>,
    app_handle: AppHandle,
    block_id: String,
    runbook_id: String,
    editor_document: Vec<serde_json::Value>,
    output_channel: Channel<BlockOutput>,
) -> Result<String, String> {
    // Build execution context
    let mut context = ContextBuilder::build_context(&block_id, &editor_document, &runbook_id)
        .await
        .map_err(|e| e.to_string())?;

    // Add SSH pool to context
    context.ssh_pool = Some(state.ssh_pool());

    // Add output storage to context
    context.output_storage = Some(state.runbook_output_variables.clone());

    // Add PTY store to context
    context.pty_store = Some(state.pty_store());

    // Add event bus to context
    let gc_sender = state.gc_event_sender();
    let event_bus = std::sync::Arc::new(ChannelEventBus::new(gc_sender));
    context.event_bus = Some(event_bus);

    // Find the block in the document
    let block_data = editor_document
        .iter()
        .find(|b| b.get("id").and_then(|v| v.as_str()) == Some(&block_id))
        .ok_or("Block not found")?;

    // Convert document block to runtime block
    let block = document_to_block(block_data)?;

    // Get event sender from state
    let event_sender = state.event_sender();

    // Create registry and execute
    let registry = BlockRegistry::new();

    match registry
        .execute_block(&block, context, event_sender, Some(output_channel))
        .await
    {
        Ok(handle) => {
            let execution_id = handle.id;
            // Store the execution handle for cancellation
            if let Some(state) = app_handle.try_state::<AtuinState>() {
                state
                    .block_executions
                    .write()
                    .await
                    .insert(execution_id, handle.clone());
            }
            Ok(execution_id.to_string())
        }
        Err(e) => Err(format!("Execution failed: {}", e)),
    }
}

#[tauri::command]
pub async fn cancel_block_execution(
    app_handle: AppHandle,
    execution_id: String,
) -> Result<(), String> {
    let execution_uuid = Uuid::parse_str(&execution_id).map_err(|e| e.to_string())?;

    if let Some(state) = app_handle.try_state::<AtuinState>() {
        let mut executions = state.block_executions.write().await;
        if let Some(handle) = executions.remove(&execution_uuid) {
            // Cancel the execution
            handle.cancellation_token.cancel();
            Ok(())
        } else {
            Err("Execution not found".to_string())
        }
    } else {
        Err("State not available".to_string())
    }
}
