use tauri::{ipc::Channel, AppHandle, Manager, State};
use uuid::Uuid;

use crate::runtime::blocks::handler::{BlockOutput, ExecutionContext};
use crate::runtime::blocks::registry::BlockRegistry;
use crate::runtime::blocks::script::Script;
use crate::runtime::blocks::terminal::Terminal;
use crate::runtime::blocks::Block;
use crate::runtime::workflow::context_builder::ContextBuilder;
use crate::state::AtuinState;

/// Convert editor document block to runtime Block enum
fn document_to_block(block_data: &serde_json::Value) -> Result<Block, String> {
    let block_type = block_data
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or("Block has no type")?;

    let block_id = block_data
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("Block has no id")?;

    let props = block_data
        .get("props")
        .and_then(|p| p.as_object())
        .ok_or("Block has no props")?;

    let id = Uuid::parse_str(block_id).map_err(|e| e.to_string())?;

    match block_type {
        "script" => {
            let script = Script::builder()
                .id(id)
                .name(
                    props
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Script")
                        .to_string(),
                )
                .code(
                    props
                        .get("code")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                )
                .interpreter(
                    props
                        .get("interpreter")
                        .and_then(|v| v.as_str())
                        .unwrap_or("bash")
                        .to_string(),
                )
                .output_variable(
                    props
                        .get("outputVariable")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                )
                .output_visible(
                    props
                        .get("outputVisible")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true),
                )
                .build();
            Ok(Block::Script(script))
        }
        "terminal" => {
            let terminal = Terminal::builder()
                .id(id)
                .name(
                    props
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Terminal")
                        .to_string(),
                )
                .code(
                    props
                        .get("code")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                )
                .output_visible(
                    props
                        .get("outputVisible")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true),
                )
                .build();
            Ok(Block::Terminal(terminal))
        }
        // Add other block types as needed
        _ => Err(format!("Unsupported block type: {}", block_type)),
    }
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
    let context = ContextBuilder::build_context(&block_id, &editor_document, &runbook_id)
        .await
        .map_err(|e| e.to_string())?;

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
        .execute_block(
            &block,
            context,
            event_sender,
            Some(output_channel),
            app_handle.clone(),
        )
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
            Ok(format!("Execution started with handle: {}", execution_id))
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
