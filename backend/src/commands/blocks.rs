use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use tauri::{ipc::Channel, AppHandle, Manager, State};
use uuid::Uuid;

use crate::commands::events::ChannelEventBus;
use crate::runtime::blocks::document::actor::DocumentHandle;
use crate::runtime::blocks::document::block_context::ResolvedContext;
use crate::runtime::blocks::document::bridge::DocumentBridgeMessage;
use crate::runtime::blocks::handler::BlockOutput;
use crate::runtime::ClientMessageChannel;
use crate::state::AtuinState;

#[async_trait]
impl<M: Serialize + Send + Sync> ClientMessageChannel<M> for tauri::ipc::Channel<M> {
    async fn send(&self, message: M) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.send(message).map_err(|e| e.into())
    }
}

#[tauri::command]
pub async fn execute_block(
    state: State<'_, AtuinState>,
    block_id: String,
    runbook_id: String,
    editor_document: Vec<serde_json::Value>,
    output_channel: Channel<BlockOutput>,
) -> Result<String, String> {
    let block_id = Uuid::parse_str(&block_id).map_err(|e| e.to_string())?;

    let documents = state.documents.read().await;
    let document = documents.get(&runbook_id).ok_or("Document not found")?;
    document
        .update_document(editor_document)
        .await
        .map_err(|e| e.to_string())?;

    // Start execution and get immutable snapshot
    // TODO: output_channel
    let exec_view = document
        .start_execution(block_id)
        .await
        .map_err(|e| e.to_string())?;

    // TODO: Actually execute the block with exec_view
    // This will require updating block handlers to use DocumentExecutionView
    // For now, just return the block ID

    Ok(block_id.to_string())
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
    document_bridge: Channel<DocumentBridgeMessage>,
) -> Result<(), String> {
    let mut documents = state.documents.write().await;
    if let Some(document) = documents.get_mut(&document_id) {
        document
            .update_bridge_channel(Box::new(document_bridge))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    let event_bus = Arc::new(ChannelEventBus::new(state.gc_event_sender()));
    let document_handle = DocumentHandle::new(document_id.clone(), event_bus, document_bridge);

    document_handle
        .put_document(document)
        .await
        .map_err(|e| e.to_string())?;

    documents.insert(document_id, document_handle);

    Ok(())
}

#[tauri::command]
pub async fn update_document(
    state: State<'_, AtuinState>,
    document_id: String,
    document_content: Vec<serde_json::Value>,
) -> Result<(), String> {
    let documents = state.documents.read().await;
    let document = documents.get(&document_id).ok_or("Document not found")?;
    document
        .put_document(document_content)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn get_flattened_block_context(
    state: State<'_, AtuinState>,
    document_id: String,
    block_id: String,
) -> Result<ResolvedContext, String> {
    let documents = state.documents.read().await;
    let document = documents.get(&document_id).ok_or("Document not found")?;
    let context = document
        .get_resolved_context(Uuid::parse_str(&block_id).map_err(|e| e.to_string())?)
        .await
        .map_err(|e| e.to_string())?;
    Ok(context)
}
