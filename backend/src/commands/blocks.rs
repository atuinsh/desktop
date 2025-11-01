use std::sync::Arc;

use async_trait::async_trait;
use tauri::{ipc::Channel, AppHandle, Manager, State};
use uuid::Uuid;

use crate::commands::events::ChannelEventBus;
use crate::kv;
use crate::runtime::blocks::document::actor::{BlockLocalValueProvider, DocumentHandle};
use crate::runtime::blocks::document::block_context::ResolvedContext;
use crate::runtime::blocks::document::bridge::DocumentBridgeMessage;
use crate::runtime::ClientMessageChannel;
use crate::state::AtuinState;

#[derive(Clone)]
struct DocumentBridgeChannel {
    runbook_id: String,
    channel: Arc<Channel<DocumentBridgeMessage>>,
}

#[async_trait]
impl ClientMessageChannel<DocumentBridgeMessage> for DocumentBridgeChannel {
    async fn send(
        &self,
        message: DocumentBridgeMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::trace!(
            "Sending message to document bridge for runbook {runbook_id}",
            runbook_id = self.runbook_id
        );
        let result = self.channel.send(message).map_err(|e| e.into());

        if let Err(e) = &result {
            log::error!("Failed to send message to document bridge: {e}");
        }

        result
    }
}

#[derive(Clone)]
struct KvBlockLocalValueProvider {
    app_handle: AppHandle,
}

impl KvBlockLocalValueProvider {
    pub fn new(app_handle: AppHandle) -> Self {
        Self { app_handle }
    }
}

#[async_trait]
impl BlockLocalValueProvider for KvBlockLocalValueProvider {
    async fn get_block_local_value(
        &self,
        block_id: Uuid,
        property_name: &str,
    ) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        let db = kv::open_db(&self.app_handle)
            .await
            .map_err(|_| Box::new(std::io::Error::other("Failed to open KV database")))?;
        let key = format!("block.{block_id}.{property_name}");
        kv::get(&db, &key).await.map_err(|e| e.into())
    }
}

#[tauri::command]
pub async fn execute_block(
    state: State<'_, AtuinState>,
    block_id: String,
    runbook_id: String,
) -> Result<Option<String>, String> {
    let block_id = Uuid::parse_str(&block_id).map_err(|e| e.to_string())?;

    let documents = state.documents.read().await;
    let document = documents.get(&runbook_id).ok_or("Document not found")?;

    // Get resources from state
    let pty_store = state.pty_store();
    let ssh_pool = state.ssh_pool();
    let event_sender = state.event_sender();

    log::debug!("Starting execution of block {block_id} in runbook {runbook_id}");

    // Get execution context
    let context = document
        .start_execution(block_id, event_sender, Some(ssh_pool), Some(pty_store))
        .await
        .map_err(|e| e.to_string())?;
    // Reset the active context for the block
    context
        .clear_active_context(block_id)
        .await
        .map_err(|e| e.to_string())?;

    // Get the block to execute
    let block = document
        .get_block(block_id)
        .await
        .ok_or("Block not found")?;

    // Execute the block
    let execution_handle = block.execute(context).await.map_err(|e| e.to_string())?;

    // Store execution handle if one was returned
    if let Some(handle) = execution_handle {
        let id = handle.id;

        let mut executions = state.block_executions.write().await;
        executions.insert(id, handle);

        Ok(Some(id.to_string()))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub async fn cancel_block_execution(
    state: State<'_, AtuinState>,
    execution_id: String,
) -> Result<(), String> {
    let execution_uuid = Uuid::parse_str(&execution_id).map_err(|e| e.to_string())?;

    let mut executions = state.block_executions.write().await;
    if let Some(handle) = executions.remove(&execution_uuid) {
        log::debug!("Cancelling block execution {execution_id}");
        // Cancel the execution
        handle.cancellation_token.cancel();
        Ok(())
    } else {
        log::error!("Cannot cancel execution; execution ID not found: {execution_id}");
        Err("Execution not found".to_string())
    }
}

#[tauri::command]
pub async fn open_document(
    app: AppHandle,
    state: State<'_, AtuinState>,
    document_id: String,
    document: Vec<serde_json::Value>,
    document_bridge: Channel<DocumentBridgeMessage>,
) -> Result<(), String> {
    let document_bridge = Arc::new(DocumentBridgeChannel {
        runbook_id: document_id.clone(),
        channel: Arc::new(document_bridge),
    });

    let mut documents = state.documents.write().await;
    if let Some(document) = documents.get(&document_id) {
        log::debug!("Updating document bridge channel for document {document_id}");

        document
            .update_bridge_channel(document_bridge)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    log::debug!("Opening document {document_id}");

    let event_bus = Arc::new(ChannelEventBus::new(state.gc_event_sender()));
    let document_handle = DocumentHandle::new(
        document_id.clone(),
        event_bus,
        document_bridge,
        Some(Box::new(KvBlockLocalValueProvider::new(app.clone()))),
    );

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
pub async fn notify_block_kv_value_changed(
    state: State<'_, AtuinState>,
    document_id: String,
    block_id: String,
    _key: String,
    _value: String,
) -> Result<(), String> {
    log::debug!("Notifying block KV value changed for document {document_id}, block {block_id}");

    let documents = state.documents.read().await;
    let document = documents.get(&document_id).ok_or("Document not found")?;
    let block_id = Uuid::parse_str(&block_id).map_err(|e| e.to_string())?;
    document
        .block_local_value_changed(block_id)
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

#[tauri::command]
pub async fn reset_runbook_state(
    state: State<'_, AtuinState>,
    document_id: String,
) -> Result<(), String> {
    let documents = state.documents.read().await;
    let document = documents.get(&document_id).ok_or("Document not found")?;
    document.reset_state().await.map_err(|e| e.to_string())?;
    Ok(())
}
