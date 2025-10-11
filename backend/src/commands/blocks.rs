use std::sync::Arc;

use std::ops::DerefMut;
use tauri::{ipc::Channel, AppHandle, Manager, State};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::commands::events::ChannelEventBus;
use crate::runtime::blocks::document::{BlockNoteChange, Document};
use crate::runtime::blocks::handler::BlockOutput;
use crate::runtime::blocks::BlockBehavior;
use crate::runtime::blocks::BlockExecutionContext;
// use crate::runtime::blocks::registry::BlockRegistry;
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
    let block_id = Uuid::parse_str(&block_id).map_err(|e| e.to_string())?;

    let documents = state.documents.read().await;
    let document = documents.get(&runbook_id).ok_or("Document not found")?;
    let mut document = document.write().await;

    let document_context = document.context_for(&block_id).ok_or("Block not found")?;

    let execution_context = BlockExecutionContext {
        event_sender: state.event_sender(),
        output_channel: Some(output_channel),
        ssh_pool: Some(state.ssh_pool()),
        pty_store: Some(state.pty_store()),
        event_bus: Some(Arc::new(ChannelEventBus::new(state.gc_event_sender()))),
    };

    let block = document.get_block_mut(&block_id).ok_or("Block not found")?;
    let block = block.deref_mut();

    // match block.execute(document_context, execution_context).await {
    //     Ok(()) => Ok(block_id.to_string()),
    //     Err(e) => Err(e.to_string()),
    // }

    // // Convert document block to runtime block
    // let block = document_to_block(block_data)?;

    // match registry
    //     .execute_block(&block, context, event_sender, Some(output_channel))
    //     .await
    // {
    //     Ok(handle) => {
    //         let execution_id = handle.id;
    //         // Store the execution handle for cancellation
    //         if let Some(state) = app_handle.try_state::<AtuinState>() {
    //             state
    //                 .block_executions
    //                 .write()
    //                 .await
    //                 .insert(execution_id, handle.clone());
    //         }
    //         Ok(execution_id.to_string())
    //     }
    //     Err(e) => Err(format!("Execution failed: {}", e)),
    // }

    Ok(block_id.to_string()) // TODO: Return execution ID
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

#[tauri::command]
pub async fn open_document(
    state: State<'_, AtuinState>,
    document_id: String,
    document: Vec<serde_json::Value>,
) -> Result<(), String> {
    let documents = state.documents.read().await;
    if let Some(document) = documents.get(&document_id) {
        // TODO: Update the context
        // context.update(document);
    } else {
        drop(documents);
        let document = Document::new(document_id.clone(), document).map_err(|e| e.to_string())?;
        state
            .documents
            .write()
            .await
            .insert(document_id.clone(), Arc::new(RwLock::new(document)));
    }

    Ok(())
}

#[tauri::command]
pub async fn apply_document_change(
    state: State<'_, AtuinState>,
    document_id: String,
    change: BlockNoteChange,
) -> Result<(), String> {
    let mut documents = state.documents.write().await;
    let document = documents
        .get_mut(&document_id)
        .ok_or("Document not found")?;
    document.write().await.apply_change(change);
    Ok(())
}
