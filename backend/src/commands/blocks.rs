use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use atuin_desktop_runtime::context::BlockContextStorage;
use atuin_desktop_runtime::events::GCEvent;
use atuin_desktop_runtime::execution::ExecutionHandle;
use atuin_desktop_runtime::execution::ExecutionResult;
use atuin_desktop_runtime::pty::PtyStoreHandle;
use atuin_desktop_runtime::ssh::SshPoolHandle;
use serde_json::Value;
use tauri::Manager;
use tauri::{ipc::Channel, AppHandle, State};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::blocks::sqlite_context_storage::SqliteContextStorage;
use crate::commands::events::ChannelEventBus;
use crate::kv;
use crate::state::AtuinState;
use atuin_desktop_runtime::client::LocalValueProvider;
use atuin_desktop_runtime::client::MessageChannel;
use atuin_desktop_runtime::client::{ClientPromptResult, DocumentBridgeMessage};
use atuin_desktop_runtime::context::ResolvedContext;
use atuin_desktop_runtime::document::DocumentHandle;

#[derive(Clone)]
struct DocumentBridgeChannel {
    runbook_id: String,
    channel: Arc<Channel<DocumentBridgeMessage>>,
}

#[async_trait]
impl MessageChannel<DocumentBridgeMessage> for DocumentBridgeChannel {
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
impl LocalValueProvider for KvBlockLocalValueProvider {
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

    let mut workspace_context = HashMap::new();
    let workspace_root = if let Some(workspace_manager) = state.workspaces.lock().await.as_ref() {
        workspace_manager
            .workspace_root(&runbook_id)
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };
    workspace_context.insert("root".to_string(), workspace_root.to_string());

    let mut extra_template_context = HashMap::new();
    extra_template_context.insert("workspace".to_string(), workspace_context);

    let execution_handle = execute_single_block(
        runbook_id,
        document,
        block_id,
        ssh_pool,
        pty_store,
        extra_template_context,
    )
    .await?;

    // Store execution handle if one was returned
    if let Some(handle) = execution_handle {
        let id = handle.id;

        let mut executions = state.block_executions.write().await;
        executions.insert(id, handle.clone());

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
            .map_err(|e| format!("Failed to update document bridge channel: {}", e))?;
        return Ok(());
    }

    log::debug!("Opening document {document_id}");

    let event_bus = Arc::new(ChannelEventBus::new(state.gc_event_sender()));
    let context_storage = SqliteContextStorage::new(
        state
            .db_instances
            .get_pool("context")
            .await
            .map_err(|e| format!("Failed to get context storage pool: {}", e))?,
    )
    .await
    .map_err(|e| format!("Failed to create context storage: {}", e))?;
    let document_handle = DocumentHandle::new(
        document_id.clone(),
        event_bus,
        document_bridge,
        Some(Box::new(KvBlockLocalValueProvider::new(app.clone()))),
        Some(Box::new(context_storage)),
    );

    document_handle
        .put_document(document)
        .await
        .map_err(|e| format!("Failed to put document: {}", e))?;

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
        .map_err(|e| format!("Failed to update document: {}", e))?;

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
        .map_err(|e| format!("Failed to notify block KV value changed: {}", e))?;
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
        .map_err(|e| format!("Failed to get flattened block context: {}", e))?;
    Ok(context)
}

#[tauri::command]
pub async fn get_block_state(
    state: State<'_, AtuinState>,
    document_id: String,
    block_id: String,
) -> Result<Value, String> {
    let documents = state.documents.read().await;
    let document = documents.get(&document_id).ok_or("Document not found")?;
    let state = document
        .get_block_state(Uuid::parse_str(&block_id).map_err(|e| e.to_string())?)
        .await
        .map_err(|e| format!("Failed to get block state: {}", e))?;
    Ok(state)
}

#[tauri::command]
pub async fn reset_runbook_state(
    state: State<'_, AtuinState>,
    document_id: String,
) -> Result<(), String> {
    let documents = state.documents.read().await;
    let document = documents.get(&document_id).ok_or("Document not found")?;
    document
        .reset_state()
        .await
        .map_err(|e| format!("Failed to reset runbook state: {}", e))?;
    Ok(())
}

#[tauri::command]
pub async fn respond_to_block_prompt(
    state: State<'_, AtuinState>,
    execution_id: Uuid,
    prompt_id: Uuid,
    answer: ClientPromptResult,
) -> Result<(), String> {
    let executions = state.block_executions.write().await;
    if let Some(handle) = executions.get(&execution_id) {
        let mut callbacks = handle.prompt_callbacks.lock().await;
        let sender = callbacks.remove(&prompt_id).ok_or("Prompt not found")?;
        sender
            .send(answer)
            .map_err(|_| "Failed to send answer to prompt".to_string())?;
        Ok(())
    } else {
        Err("Execution not found".to_string())
    }
}

#[tauri::command]
pub async fn remove_stored_context_for_document(
    state: State<'_, AtuinState>,
    document_id: String,
) -> Result<(), String> {
    let context_storage = SqliteContextStorage::new(
        state
            .db_instances
            .get_pool("context")
            .await
            .map_err(|e| format!("Failed to get context storage pool: {}", e))?,
    )
    .await
    .map_err(|e| format!("Failed to create context storage: {}", e))?;

    context_storage
        .delete_for_document(&document_id)
        .await
        .map_err(|e| {
            format!(
                "Failed to remove stored context for document {document_id}: {}",
                e
            )
        })?;
    Ok(())
}

#[tauri::command]
pub async fn start_serial_execution(
    app: AppHandle,
    state: State<'_, AtuinState>,
    document_id: String,
) -> Result<(), String> {
    let mut serial_executions = state.serial_executions.write().await;
    if serial_executions.contains_key(&document_id) {
        return Err("Serial execution already started".to_string());
    }

    log::debug!("Starting serial execution for document {document_id}");

    let documents = state.documents.read().await;
    let document = documents.get(&document_id).ok_or("Document not found")?;

    let pty_store = state.pty_store();
    let ssh_pool = state.ssh_pool();

    let block_ids = document
        .blocks()
        .await
        .map_err(|e| format!("Failed to get blocks from document {document_id}: {}", e))?
        .iter()
        .map(|b| b.id())
        .collect::<Vec<_>>();

    let mut workspace_context = HashMap::new();
    let workspace_root = if let Some(workspace_manager) = state.workspaces.lock().await.as_ref() {
        workspace_manager
            .workspace_root(&document_id)
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };

    workspace_context.insert("root".to_string(), workspace_root.to_string());

    let (tx, mut rx) = oneshot::channel();
    serial_executions.insert(document_id.clone(), tx);

    let document_uuid = Uuid::parse_str(&document_id).map_err(|e| e.to_string())?;
    let document = document.clone();
    document
        .event_bus()
        .emit(GCEvent::SerialExecutionStarted {
            runbook_id: document_uuid,
        })
        .await
        .map_err(|e| format!("Failed to emit serial execution started event: {}", e))?;
    tokio::spawn(async move {
        let mut exit_type: ExecutionResult = ExecutionResult::Success;

        let mut extra_template_context = HashMap::new();
        extra_template_context.insert("workspace".to_string(), workspace_context);

        log::trace!("Starting serial execution for document {document_id}; blocks: {block_ids:?}");
        'outer: for block_id in &block_ids {
            log::trace!("Executing block {block_id} in document {document_id}");
            let handle = execute_single_block(
                document_id.clone(),
                &document,
                *block_id,
                ssh_pool.clone(),
                pty_store.clone(),
                extra_template_context.clone(),
            )
            .await;

            let app_clone = app.clone();
            let cleanup = async move |handle_id: Uuid| {
                let state = app_clone.state::<AtuinState>();
                let mut executions = state.block_executions.write().await;
                executions.remove(&handle_id);
            };

            match handle {
                Ok(Some(handle)) => {
                    let mut finished_channel = handle.finished_channel();
                    let state = app.state::<AtuinState>();
                    let mut executions = state.block_executions.write().await;
                    executions.insert(handle.id, handle.clone());
                    drop(executions);

                    let mut stop_serial_exec = false;
                    tokio::select! {
                        _ = finished_channel.changed() => {
                            match *(finished_channel.borrow_and_update()) {
                                None => {
                                    log::debug!("Block {block_id} in document {document_id} still running");
                                }
                                Some(ExecutionResult::Success) => {
                                    log::debug!("Block {block_id} in document {document_id} finished successfully");
                                }
                                Some(ExecutionResult::Failure) => {
                                    log::debug!("Block {block_id} in document {document_id} failed");
                                    exit_type = ExecutionResult::Failure;
                                    stop_serial_exec = true;
                                }
                                Some(ExecutionResult::Cancelled) => {
                                    log::debug!("Block {block_id} in document {document_id} cancelled");
                                    exit_type = ExecutionResult::Cancelled;
                                    stop_serial_exec = true;
                                }
                            }

                            log::trace!("Cleaning up execution handle for block {block_id} in document {document_id}");
                            cleanup(handle.id).await;
                            if stop_serial_exec {
                                log::trace!("Stopping serial execution for document {document_id} because block {block_id} failed or was cancelled");
                                break 'outer;
                            }
                        }
                        _ = &mut rx => {
                            handle.cancellation_token.cancel();
                            log::debug!("Serial execution cancelled for document {document_id}");
                            cleanup(handle.id).await;
                            exit_type = ExecutionResult::Cancelled;
                            break 'outer;
                        }
                    }
                }
                Err(e) => {
                    log::error!(
                        "Failed to execute block {block_id} in document {document_id}: {e}"
                    );
                    exit_type = ExecutionResult::Failure;
                    break 'outer;
                }
                Ok(None) => {
                    // Block did not return an execution handle; move to the next block.
                }
            }
        }

        match exit_type {
            ExecutionResult::Success => {
                let _ = document
                    .event_bus()
                    .emit(GCEvent::SerialExecutionCompleted {
                        runbook_id: document_uuid,
                    })
                    .await
                    .map_err(|e| format!("Failed to emit serial execution completed event: {}", e));
            }
            ExecutionResult::Failure => {
                let _ = document
                    .event_bus()
                    .emit(GCEvent::SerialExecutionFailed {
                        runbook_id: document_uuid,
                        error: "Serial execution failed".to_string(),
                    })
                    .await
                    .map_err(|e| format!("Failed to emit serial execution failed event: {}", e));
            }
            ExecutionResult::Cancelled => {
                let _ = document
                    .event_bus()
                    .emit(GCEvent::SerialExecutionCancelled {
                        runbook_id: document_uuid,
                    })
                    .await
                    .map_err(|e| format!("Failed to emit serial execution cancelled event: {}", e));
            }
        };

        log::trace!("Serial execution for document {document_id} completed; blocks: {block_ids:?}");
        let state = app.state::<AtuinState>();
        let mut serial_executions = state.serial_executions.write().await;
        serial_executions.remove(&document_id);

        log::debug!("Serial execution for document {document_id} completed");
    });

    Ok(())
}

#[tauri::command]
pub async fn stop_serial_execution(
    state: State<'_, AtuinState>,
    document_id: String,
) -> Result<(), String> {
    let mut serial_executions = state.serial_executions.write().await;
    if let Some(tx) = serial_executions.remove(&document_id) {
        tx.send(())
            .map_err(|_| "Failed to send stop signal to serial execution")?;
    }
    Ok(())
}

async fn execute_single_block(
    document_id: String,
    document: &Arc<DocumentHandle>,
    block_id: Uuid,
    ssh_pool: SshPoolHandle,
    pty_store: PtyStoreHandle,
    extra_template_context: HashMap<String, HashMap<String, String>>,
) -> Result<Option<ExecutionHandle>, String> {
    log::debug!("Starting block execution for block {block_id} in document {document_id}");

    // Get execution context
    let context = document
        .create_execution_context(
            block_id,
            Some(ssh_pool),
            Some(pty_store),
            Some(extra_template_context),
        )
        .await
        .map_err(|e| format!("Failed to start execution: {}", e))?;
    // Reset the active context for the block
    context
        .clear_active_context(block_id)
        .await
        .map_err(|e| format!("Failed to clear active context: {}", e))?;

    // // Ensure that we send the block started event to the client
    // // so that they have the execution ID to use for cancellation.
    // context
    //     .block_started()
    //     .await
    //     .map_err(|e| format!("Failed to start block: {}", e))?;

    // Get the block to execute
    let block = document
        .get_block(block_id)
        .await
        .ok_or("Failed to execute block: block not found")?;

    // Execute the block
    block.execute(context).await.map_err(|e| e.to_string())
}
