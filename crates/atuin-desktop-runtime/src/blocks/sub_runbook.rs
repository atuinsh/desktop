//! Sub-runbook block type
//!
//! This module provides the SubRunbook block which allows embedding and executing
//! another runbook within a parent runbook. The sub-runbook inherits context from
//! its parent but maintains isolated context (changes don't propagate back).

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::blocks::{Block, BlockBehavior, FromDocument};
use crate::client::{RunbookLoadError, SubRunbookRef};
use crate::context::{BlockState, BlockVars};
use crate::events::MemoryEventBus;
use crate::execution::{ExecutionContext, ExecutionHandle, ExecutionResult};

/// State representing the progress of a sub-runbook execution
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SubRunbookState {
    /// Total number of blocks in the sub-runbook
    pub total_blocks: usize,
    /// Number of blocks that have completed
    pub completed_blocks: usize,
    /// Name of the block currently being executed
    pub current_block_name: Option<String>,
    /// Current execution status
    pub status: SubRunbookStatus,
}

impl BlockState for SubRunbookState {}

/// Status of sub-runbook execution
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SubRunbookStatus {
    /// Not currently executing
    #[default]
    Idle,
    /// Loading the referenced runbook
    Loading,
    /// Executing blocks sequentially
    Running,
    /// All blocks completed successfully
    Success,
    /// Execution failed with an error
    Failed { error: String },
    /// Execution was cancelled by user
    Cancelled,
    /// Referenced runbook was not found
    NotFound,
    /// Recursion detected (runbook is already in execution stack)
    RecursionDetected,
}

/// A block that embeds and executes another runbook
///
/// When executed, this block loads the referenced runbook and executes
/// all its blocks sequentially. The sub-runbook inherits context from
/// the parent (environment variables, working directory, variables, SSH host)
/// but changes made within the sub-runbook do not propagate back to the parent
/// unless the corresponding export options are enabled.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct SubRunbook {
    /// Unique identifier for this block instance
    #[builder(setter(into))]
    pub id: Uuid,

    /// Display name for this block
    #[builder(setter(into))]
    pub name: String,

    /// Reference to the runbook to execute
    #[builder(default)]
    pub runbook_ref: SubRunbookRef,

    /// Cached display name of the referenced runbook (optional)
    #[builder(default)]
    pub runbook_name: Option<String>,

    /// Export environment variables set by the sub-runbook to the parent
    #[builder(default)]
    pub export_env: bool,

    /// Export template variables set by the sub-runbook to the parent
    #[builder(default)]
    pub export_vars: bool,

    /// Export working directory from the sub-runbook to the parent
    #[builder(default)]
    pub export_cwd: bool,
}

impl FromDocument for SubRunbook {
    fn from_document(block_data: &serde_json::Value) -> Result<Self, String> {
        let block_id = block_data
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or("Block has no id")?;

        let props = block_data
            .get("props")
            .and_then(|p| p.as_object())
            .ok_or("Block has no props")?;

        let id = Uuid::parse_str(block_id).map_err(|e| e.to_string())?;

        // Parse runbook reference from props
        let runbook_ref = SubRunbookRef {
            id: props
                .get("runbookId")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()),
            uri: props
                .get("runbookUri")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()),
            path: props
                .get("runbookPath")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()),
        };

        let export_env = props
            .get("exportEnv")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let export_vars = props
            .get("exportVars")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let export_cwd = props
            .get("exportCwd")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let sub_runbook = SubRunbook::builder()
            .id(id)
            .name(
                props
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Sub-Runbook")
                    .to_string(),
            )
            .runbook_ref(runbook_ref)
            .runbook_name(
                props
                    .get("runbookName")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
            )
            .export_env(export_env)
            .export_vars(export_vars)
            .export_cwd(export_cwd)
            .build();

        Ok(sub_runbook)
    }
}

#[async_trait]
impl BlockBehavior for SubRunbook {
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::SubRunbook(self)
    }

    fn create_state(&self) -> Option<Box<dyn BlockState>> {
        Some(Box::new(SubRunbookState::default()))
    }

    async fn execute(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        tracing::trace!("Executing sub-runbook block {id}", id = self.id);

        // Check if runbook reference is specified
        if self.runbook_ref.is_empty() {
            let _ = context.block_started().await;
            let _ = context
                .update_block_state::<SubRunbookState, _>(self.id, |state| {
                    state.status = SubRunbookStatus::Failed {
                        error: "No runbook selected".to_string(),
                    };
                })
                .await;
            let _ = context
                .block_failed("No runbook selected".to_string())
                .await;
            return Ok(Some(context.handle()));
        }

        // Check if runbook loader is available
        let runbook_loader = match context.runbook_loader() {
            Some(loader) => loader.clone(),
            None => {
                let _ = context.block_started().await;
                let _ = context
                    .update_block_state::<SubRunbookState, _>(self.id, |state| {
                        state.status = SubRunbookStatus::Failed {
                            error: "Sub-runbook execution not available".to_string(),
                        };
                    })
                    .await;
                let _ = context
                    .block_failed(
                        "Sub-runbook execution not available (no runbook loader configured)"
                            .to_string(),
                    )
                    .await;
                return Ok(Some(context.handle()));
            }
        };

        // Clone context to get handle for return value (original moves into spawned task)
        let handle_context = context.clone();
        let block_id = self.id;
        let runbook_ref = self.runbook_ref.clone();
        let export_env = self.export_env;
        let export_vars = self.export_vars;
        let export_cwd = self.export_cwd;
        // Use runbook_name if set, otherwise fall back to display_id
        let runbook_name = self
            .runbook_name
            .clone()
            .unwrap_or_else(|| self.runbook_ref.display_id());

        tokio::spawn(async move {
            // Mark block as started
            let _ = context.block_started().await;

            // Update state to loading
            let _ = context
                .update_block_state::<SubRunbookState, _>(block_id, |state| {
                    state.status = SubRunbookStatus::Loading;
                })
                .await;

            // Check for recursion before loading (use display_id for stack tracking)
            let stack_id = runbook_ref.display_id();
            if context.is_in_execution_stack(&stack_id) {
                tracing::warn!(
                    "Recursion detected for sub-runbook {}: already in stack {:?}",
                    stack_id,
                    context.execution_stack()
                );
                let _ = context
                    .update_block_state::<SubRunbookState, _>(block_id, |state| {
                        state.status = SubRunbookStatus::RecursionDetected;
                    })
                    .await;
                let _ = context
                    .block_failed(format!(
                        "Recursion detected: runbook '{}' is already being executed",
                        runbook_name
                    ))
                    .await;
                return;
            }

            // Load the runbook
            let loaded_runbook = match runbook_loader.load_runbook(&runbook_ref).await {
                Ok(loaded) => loaded,
                Err(RunbookLoadError::NotFound { .. }) => {
                    let _ = context
                        .update_block_state::<SubRunbookState, _>(block_id, |state| {
                            state.status = SubRunbookStatus::NotFound;
                        })
                        .await;
                    let _ = context
                        .block_failed(format!("Runbook '{}' not found", runbook_name))
                        .await;
                    return;
                }
                Err(RunbookLoadError::LoadFailed { message, .. }) => {
                    let error_msg = message.clone();
                    let _ = context
                        .update_block_state::<SubRunbookState, _>(block_id, move |state| {
                            state.status = SubRunbookStatus::Failed { error: error_msg };
                        })
                        .await;
                    let _ = context
                        .block_failed(format!(
                            "Failed to load runbook '{}': {}",
                            runbook_name, message
                        ))
                        .await;
                    return;
                }
            };

            let sub_runbook_id = loaded_runbook.id;
            let runbook_content = loaded_runbook.content;

            // Create a child DocumentHandle for the sub-runbook
            // Use the actual sub-runbook's UUID as the document ID
            let sub_document_id = sub_runbook_id.to_string();

            // Create a simple no-op message channel for the sub-document
            // Actual output will flow through the parent's channel via with_sub_runbook
            struct NoOpChannel;
            #[async_trait::async_trait]
            impl crate::client::MessageChannel<crate::client::DocumentBridgeMessage> for NoOpChannel {
                async fn send(
                    &self,
                    _message: crate::client::DocumentBridgeMessage,
                ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                    Ok(())
                }
            }

            let sub_document = crate::document::DocumentHandle::new(
                sub_document_id.clone(),
                Arc::new(MemoryEventBus::new()),
                Arc::new(NoOpChannel),
                context.block_local_value_provider(), // inherit from parent
                None, // context_storage (sub-runbooks don't persist context)
                Some(runbook_loader.clone()),
            );

            // Set parent context so the sub-runbook inherits vars, env_vars, cwd, ssh_host
            if let Err(e) = sub_document
                .set_parent_context(context.context_resolver.clone())
                .await
            {
                let error = format!("Failed to set parent context: {}", e);
                let _ = context
                    .update_block_state::<SubRunbookState, _>(block_id, move |state| {
                        state.status = SubRunbookStatus::Failed { error };
                    })
                    .await;
                let _ = context
                    .block_failed(format!("Failed to set parent context: {}", e))
                    .await;
                return;
            }

            // Load the runbook content into the document
            if let Err(e) = sub_document.put_document(runbook_content.clone()).await {
                let error = format!("Failed to initialize sub-runbook document: {}", e);
                let _ = context
                    .update_block_state::<SubRunbookState, _>(block_id, move |state| {
                        state.status = SubRunbookStatus::Failed { error };
                    })
                    .await;
                let _ = context
                    .block_failed(format!("Failed to initialize sub-runbook document: {}", e))
                    .await;
                return;
            }

            // Get blocks from the document
            let blocks = match sub_document.blocks().await {
                Ok(blocks) => blocks,
                Err(e) => {
                    let error = format!("Failed to get blocks from sub-runbook: {}", e);
                    let _ = context
                        .update_block_state::<SubRunbookState, _>(block_id, move |state| {
                            state.status = SubRunbookStatus::Failed { error };
                        })
                        .await;
                    let _ = context
                        .block_failed(format!("Failed to get blocks from sub-runbook: {}", e))
                        .await;
                    return;
                }
            };

            let total_blocks = blocks.len();

            // Update state with total blocks
            let _ = context
                .update_block_state::<SubRunbookState, _>(block_id, move |state| {
                    state.total_blocks = total_blocks;
                    state.completed_blocks = 0;
                    state.status = SubRunbookStatus::Running;
                })
                .await;

            // If no blocks, we're done
            if blocks.is_empty() {
                let _ = context
                    .update_block_state::<SubRunbookState, _>(block_id, |state| {
                        state.status = SubRunbookStatus::Success;
                    })
                    .await;
                let _ = context.block_finished(Some(0), true).await;
                return;
            }

            // Execute blocks sequentially
            for (index, block) in blocks.iter().enumerate() {
                // Update progress state
                let block_name = block.name();
                let current_name = if block_name.is_empty() {
                    None
                } else {
                    Some(block_name)
                };

                let _ = context
                    .update_block_state::<SubRunbookState, _>(block_id, move |state| {
                        state.completed_blocks = index;
                        state.current_block_name = current_name;
                    })
                    .await;

                // Create execution context for this block using the sub-document
                let sub_block_context = match sub_document
                    .create_execution_context(
                        block.id(),
                        None, // SSH pool will be set via with_resources
                        None, // PTY store will be set via with_resources
                        None, // extra_template_context
                    )
                    .await
                {
                    Ok(ctx) => ctx,
                    Err(e) => {
                        let error = format!("Failed to create execution context: {}", e);
                        let _ = context
                            .update_block_state::<SubRunbookState, _>(block_id, move |state| {
                                state.status = SubRunbookStatus::Failed { error };
                            })
                            .await;
                        let _ = context
                            .block_failed(format!("Failed to create execution context: {}", e))
                            .await;
                        return;
                    }
                };

                let sub_context =
                    match sub_block_context.configure_for_sub_runbook(&context, stack_id.clone()) {
                        Ok(ctx) => ctx.with_resources(context.ssh_pool(), context.pty_store()),
                        Err(e) => {
                            let error = e.to_string();
                            let _ = context
                                .update_block_state::<SubRunbookState, _>(block_id, move |state| {
                                    state.status = SubRunbookStatus::Failed { error };
                                })
                                .await;
                            let _ = context.block_failed(e.to_string()).await;
                            return;
                        }
                    };

                // Execute the block
                let execution_handle = match block.clone().execute(sub_context).await {
                    Ok(handle) => handle,
                    Err(e) => {
                        let error = e.to_string();
                        let _ = context
                            .update_block_state::<SubRunbookState, _>(block_id, move |state| {
                                state.status = SubRunbookStatus::Failed { error };
                            })
                            .await;
                        let _ = context.block_failed(e.to_string()).await;
                        return;
                    }
                };

                // Wait for block to complete (if it has an execution handle)
                // Passive-only blocks (env, var, etc.) return None
                if let Some(handle) = execution_handle {
                    let result = handle.wait_for_completion().await;

                    match result {
                        ExecutionResult::Success => {
                            // Success - continue to next block
                        }
                        ExecutionResult::Failure => {
                            let error = format!("Block '{}' failed", block.name());
                            let _ = context
                                .update_block_state::<SubRunbookState, _>(block_id, move |state| {
                                    state.status = SubRunbookStatus::Failed { error };
                                })
                                .await;
                            let _ = context
                                .block_failed(format!("Block '{}' failed", block.name()))
                                .await;
                            return;
                        }
                        ExecutionResult::Cancelled => {
                            let _ = context
                                .update_block_state::<SubRunbookState, _>(block_id, |state| {
                                    state.status = SubRunbookStatus::Cancelled;
                                })
                                .await;
                            let _ = context.block_cancelled().await;
                            return;
                        }
                        ExecutionResult::Paused => {
                            // Pause blocks are not supported in sub-runbooks
                            let error =
                                "Pause blocks are not supported in sub-runbooks".to_string();
                            let _ = context
                                .update_block_state::<SubRunbookState, _>(block_id, move |state| {
                                    state.status = SubRunbookStatus::Failed { error };
                                })
                                .await;
                            let _ = context
                                .block_failed(
                                    "Pause blocks are not supported in sub-runbooks".to_string(),
                                )
                                .await;
                            return;
                        }
                    }
                }
            }

            // All blocks completed successfully
            let _ = context
                .update_block_state::<SubRunbookState, _>(block_id, move |state| {
                    state.completed_blocks = total_blocks;
                    state.current_block_name = None;
                    state.status = SubRunbookStatus::Success;
                })
                .await;

            // Export context to parent if any export options are enabled
            if export_env || export_vars || export_cwd {
                // Get final context resolver from sub-document (includes all block contexts)
                let final_resolver = match sub_document.get_context_resolver().await {
                    Ok(resolver) => resolver,
                    Err(e) => {
                        tracing::warn!("Failed to get context resolver for context export: {}", e);
                        // Don't fail the whole block just because we couldn't export context
                        let _ = context.block_finished(Some(0), true).await;
                        return;
                    }
                };

                // Collect items to export
                let mut new_env_vars: Vec<(String, String)> = Vec::new();
                let mut new_vars: Vec<(String, String, String)> = Vec::new();
                let mut new_cwd: Option<String> = None;

                // Export environment variables
                if export_env {
                    let child_env_vars = final_resolver.env_vars();
                    let parent_env_vars = context.context_resolver.env_vars();

                    tracing::debug!(
                        "export_env: child has {} env vars, parent has {} env vars",
                        child_env_vars.len(),
                        parent_env_vars.len()
                    );

                    new_env_vars = child_env_vars
                        .iter()
                        .filter(|(k, v)| {
                            parent_env_vars.get(*k).map(|pv| pv.as_str()) != Some(v.as_str())
                        })
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();

                    if !new_env_vars.is_empty() {
                        tracing::info!(
                            "Exporting {} env vars from sub-runbook to parent: {:?}",
                            new_env_vars.len(),
                            new_env_vars.iter().map(|(k, _)| k).collect::<Vec<_>>()
                        );
                    }
                }

                // Export template variables
                if export_vars {
                    let child_vars = final_resolver.vars();
                    let parent_vars = context.context_resolver.vars();

                    tracing::debug!(
                        "export_vars: child has {} vars, parent has {} vars",
                        child_vars.len(),
                        parent_vars.len()
                    );

                    new_vars = child_vars
                        .iter()
                        .filter(|(k, v)| {
                            parent_vars.get(*k).map(|pv| pv.as_str()) != Some(v.as_str())
                        })
                        .map(|(k, v)| (k.clone(), v.clone(), "(sub-runbook export)".to_string()))
                        .collect();

                    if !new_vars.is_empty() {
                        tracing::info!(
                            "Exporting {} vars from sub-runbook to parent: {:?}",
                            new_vars.len(),
                            new_vars.iter().map(|(k, _, _)| k).collect::<Vec<_>>()
                        );
                    }
                }

                // Export working directory
                if export_cwd {
                    let child_cwd = final_resolver.cwd();
                    let parent_cwd = context.context_resolver.cwd();

                    if child_cwd != parent_cwd {
                        tracing::info!(
                            "Exporting cwd from sub-runbook to parent: {} -> {}",
                            parent_cwd,
                            child_cwd
                        );
                        new_cwd = Some(child_cwd.to_string());
                    }
                }

                // Apply all context updates
                if !new_env_vars.is_empty() || !new_vars.is_empty() || new_cwd.is_some() {
                    let _ = context
                        .update_active_context(block_id, move |ctx| {
                            for (name, value) in new_env_vars {
                                ctx.add_env(name, value);
                            }
                            for (name, value, source) in new_vars {
                                ctx.add_var(name, value, source);
                            }
                            if let Some(cwd) = new_cwd {
                                ctx.set_cwd(cwd);
                            }
                        })
                        .await;
                }
            }

            let _ = context.block_finished(Some(0), true).await;
        });

        Ok(Some(handle_context.handle()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;

    use crate::client::{DocumentBridgeMessage, MemoryRunbookContentLoader, MessageChannel};
    use crate::context::{BlockContext, BlockContextStorage};
    use crate::document::DocumentHandle;
    use crate::events::MemoryEventBus;

    /// In-memory storage for block contexts (test-only)
    struct MemoryBlockContextStorage {
        contexts: std::sync::Mutex<HashMap<String, BlockContext>>,
    }

    impl MemoryBlockContextStorage {
        fn new() -> Self {
            Self {
                contexts: std::sync::Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl BlockContextStorage for MemoryBlockContextStorage {
        async fn save(
            &self,
            document_id: &str,
            block_id: &Uuid,
            context: &BlockContext,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            let key = format!("{}:{}", document_id, block_id);
            self.contexts.lock().unwrap().insert(key, context.clone());
            Ok(())
        }

        async fn load(
            &self,
            document_id: &str,
            block_id: &Uuid,
        ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
            let key = format!("{}:{}", document_id, block_id);
            Ok(self.contexts.lock().unwrap().get(&key).cloned())
        }

        async fn delete(
            &self,
            document_id: &str,
            block_id: &Uuid,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            let key = format!("{}:{}", document_id, block_id);
            self.contexts.lock().unwrap().remove(&key);
            Ok(())
        }

        async fn delete_for_document(
            &self,
            runbook_id: &str,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            let prefix = format!("{}:", runbook_id);
            self.contexts
                .lock()
                .unwrap()
                .retain(|k, _| !k.starts_with(&prefix));
            Ok(())
        }
    }

    /// No-op message channel for tests
    struct NoOpMessageChannel;

    #[async_trait]
    impl MessageChannel<DocumentBridgeMessage> for NoOpMessageChannel {
        async fn send(
            &self,
            _message: DocumentBridgeMessage,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }
    }

    /// Helper to set up test infrastructure for sub-runbook execution
    async fn setup_test_document(
        runbook_loader: Arc<MemoryRunbookContentLoader>,
    ) -> (Arc<DocumentHandle>, Arc<MemoryEventBus>) {
        let event_bus = Arc::new(MemoryEventBus::new());
        let message_channel: Arc<dyn MessageChannel<DocumentBridgeMessage>> =
            Arc::new(NoOpMessageChannel);
        let context_storage = Box::new(MemoryBlockContextStorage::new());

        let document_handle = DocumentHandle::new(
            Uuid::new_v4().to_string(),
            event_bus.clone(),
            message_channel,
            None, // block_local_value_provider
            Some(context_storage),
            Some(runbook_loader),
        );

        (document_handle, event_bus)
    }

    /// Test: Sub-runbook creates a file, parent runbook reads it
    ///
    /// Runbook A: Creates file with "test content"
    /// Runbook B: Calls A as sub-runbook, then cats the file
    #[tokio::test]
    async fn test_sub_runbook_creates_file_parent_reads_it() {
        // Create a temp file path for the test
        let test_file = std::env::temp_dir().join(format!("sub_runbook_test_{}", Uuid::new_v4()));
        let test_file_path = test_file.to_string_lossy().to_string();

        // Clean up any existing file
        let _ = std::fs::remove_file(&test_file);

        // Define runbook A: creates the file
        let runbook_a_id = "runbook-a";
        let script_block_id = Uuid::new_v4();
        let runbook_a_content = vec![json!({
            "id": script_block_id.to_string(),
            "type": "script",
            "props": {
                "name": "Create File",
                "code": format!("echo -n 'test content' > {}", test_file_path),
                "interpreter": "bash"
            }
        })];

        // Define runbook B: calls A, then cats the file
        let sub_runbook_block_id = Uuid::new_v4();
        let cat_script_block_id = Uuid::new_v4();
        let runbook_b_content = vec![
            json!({
                "id": sub_runbook_block_id.to_string(),
                "type": "sub-runbook",
                "props": {
                    "name": "Run Setup",
                    "runbookPath": runbook_a_id,
                    "runbookName": "Setup Runbook"
                }
            }),
            json!({
                "id": cat_script_block_id.to_string(),
                "type": "script",
                "props": {
                    "name": "Read File",
                    "code": format!("cat {}", test_file_path),
                    "interpreter": "bash",
                    "outputVariable": "file_content"
                }
            }),
        ];

        // Set up the runbook loader with both runbooks
        let runbook_loader = Arc::new(
            MemoryRunbookContentLoader::new().with_runbook(runbook_a_id, runbook_a_content),
        );

        let (document_handle, event_bus) = setup_test_document(runbook_loader.clone()).await;

        // Load runbook B into the document
        document_handle
            .update_document(runbook_b_content)
            .await
            .expect("Should load document");

        // Execute the sub-runbook block
        let exec_context = document_handle
            .create_execution_context(sub_runbook_block_id, None, None, None)
            .await
            .expect("Should create execution context");

        let sub_runbook_block = SubRunbook::builder()
            .id(sub_runbook_block_id)
            .name("Run Setup")
            .runbook_ref(SubRunbookRef {
                id: None,
                uri: None,
                path: Some(runbook_a_id.to_string()),
            })
            .runbook_name(Some("Setup Runbook".to_string()))
            .build();

        let handle = sub_runbook_block
            .execute(exec_context)
            .await
            .expect("Should execute sub-runbook");

        // Wait for execution to complete
        if let Some(handle) = handle {
            let _ = handle.wait_for_completion().await;
        }

        // Verify the file was created by the sub-runbook
        let file_contents = std::fs::read_to_string(&test_file)
            .expect("File should exist after sub-runbook execution");
        assert_eq!(file_contents, "test content");

        // Now execute the cat script block to verify reading works
        let cat_exec_context = document_handle
            .create_execution_context(cat_script_block_id, None, None, None)
            .await
            .expect("Should create execution context for cat");

        let cat_block = crate::blocks::script::Script::builder()
            .id(cat_script_block_id)
            .name("Read File")
            .code(format!("cat {}", test_file_path))
            .interpreter("bash")
            .output_variable(Some("file_content".to_string()))
            .build();

        let cat_handle = cat_block
            .execute(cat_exec_context)
            .await
            .expect("Should execute cat script");

        // Wait for cat to complete
        if let Some(handle) = cat_handle {
            let _ = handle.wait_for_completion().await;
        }

        // Verify events were emitted
        let events = event_bus.events();
        assert!(!events.is_empty(), "Should have emitted events");

        // Clean up
        let _ = std::fs::remove_file(&test_file);
    }

    /// Simpler test: verify sub-runbook executes child blocks and reports correct progress
    #[tokio::test]
    async fn test_sub_runbook_executes_multiple_blocks() {
        // Create temp files to track execution order
        let marker_dir =
            std::env::temp_dir().join(format!("sub_runbook_markers_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&marker_dir).expect("Should create marker dir");

        let marker1 = marker_dir.join("marker1");
        let marker2 = marker_dir.join("marker2");

        // Define a sub-runbook with two script blocks
        let sub_runbook_id = "multi-block-runbook";
        let block1_id = Uuid::new_v4();
        let block2_id = Uuid::new_v4();

        let sub_runbook_content = vec![
            json!({
                "id": block1_id.to_string(),
                "type": "script",
                "props": {
                    "name": "Create Marker 1",
                    "code": format!("echo 'first' > {}", marker1.to_string_lossy()),
                    "interpreter": "bash"
                }
            }),
            json!({
                "id": block2_id.to_string(),
                "type": "script",
                "props": {
                    "name": "Create Marker 2",
                    "code": format!("echo 'second' > {}", marker2.to_string_lossy()),
                    "interpreter": "bash"
                }
            }),
        ];

        // Parent runbook just calls the sub-runbook
        let parent_sub_block_id = Uuid::new_v4();
        let parent_content = vec![json!({
            "id": parent_sub_block_id.to_string(),
            "type": "sub-runbook",
            "props": {
                "name": "Run Multi-Block",
                "runbookPath": sub_runbook_id
            }
        })];

        let runbook_loader = Arc::new(
            MemoryRunbookContentLoader::new().with_runbook(sub_runbook_id, sub_runbook_content),
        );

        let (document_handle, _event_bus) = setup_test_document(runbook_loader).await;

        document_handle
            .update_document(parent_content)
            .await
            .expect("Should load document");

        let exec_context = document_handle
            .create_execution_context(parent_sub_block_id, None, None, None)
            .await
            .expect("Should create execution context");

        let sub_runbook_block = SubRunbook::builder()
            .id(parent_sub_block_id)
            .name("Run Multi-Block")
            .runbook_ref(SubRunbookRef {
                id: None,
                uri: None,
                path: Some(sub_runbook_id.to_string()),
            })
            .build();

        let handle = sub_runbook_block
            .execute(exec_context)
            .await
            .expect("Should execute");

        // Wait for completion
        if let Some(handle) = handle {
            let _ = handle.wait_for_completion().await;
        }

        // Verify both markers were created (blocks executed in order)
        assert!(marker1.exists(), "First marker should exist");
        assert!(marker2.exists(), "Second marker should exist");

        let content1 = std::fs::read_to_string(&marker1).expect("Should read marker1");
        let content2 = std::fs::read_to_string(&marker2).expect("Should read marker2");

        assert_eq!(content1.trim(), "first");
        assert_eq!(content2.trim(), "second");

        // Clean up
        let _ = std::fs::remove_dir_all(&marker_dir);
    }

    #[test]
    fn test_from_document() {
        let block_data = json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "type": "sub-runbook",
            "props": {
                "name": "Setup Environment",
                "runbookId": "abc123",
                "runbookName": "Common Setup"
            }
        });

        let sub_runbook = SubRunbook::from_document(&block_data).unwrap();

        assert_eq!(
            sub_runbook.id,
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap()
        );
        assert_eq!(sub_runbook.name, "Setup Environment");
        assert_eq!(sub_runbook.runbook_ref.id, Some("abc123".to_string()));
        assert_eq!(sub_runbook.runbook_name, Some("Common Setup".to_string()));
    }

    #[test]
    fn test_from_document_defaults() {
        let block_data = json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "type": "sub-runbook",
            "props": {}
        });

        let sub_runbook = SubRunbook::from_document(&block_data).unwrap();

        assert_eq!(sub_runbook.name, "Sub-Runbook");
        assert!(sub_runbook.runbook_ref.is_empty());
        assert_eq!(sub_runbook.runbook_name, None);
    }

    #[test]
    fn test_state_serialization() {
        let state = SubRunbookState {
            total_blocks: 5,
            completed_blocks: 2,
            current_block_name: Some("Script Block".to_string()),
            status: SubRunbookStatus::Running,
        };

        let json = serde_json::to_string(&state).unwrap();
        let parsed: SubRunbookState = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.total_blocks, 5);
        assert_eq!(parsed.completed_blocks, 2);
        assert_eq!(parsed.current_block_name, Some("Script Block".to_string()));
        assert_eq!(parsed.status, SubRunbookStatus::Running);
    }

    /// Test: Recursion detection prevents infinite loops
    ///
    /// Runbook A calls itself as a sub-runbook - this should be detected and fail
    #[tokio::test]
    async fn test_recursion_detection() {
        // Define runbook A that calls itself
        let runbook_a_id = "recursive-runbook";
        let sub_runbook_block_id = Uuid::new_v4();

        let runbook_a_content = vec![json!({
            "id": sub_runbook_block_id.to_string(),
            "type": "sub-runbook",
            "props": {
                "name": "Call Self",
                "runbookPath": runbook_a_id
            }
        })];

        let runbook_loader = Arc::new(
            MemoryRunbookContentLoader::new().with_runbook(runbook_a_id, runbook_a_content.clone()),
        );

        let (document_handle, event_bus) = setup_test_document(runbook_loader.clone()).await;

        // Load runbook A
        document_handle
            .update_document(runbook_a_content)
            .await
            .expect("Should load document");

        // Execute the sub-runbook block (which tries to call itself)
        let exec_context = document_handle
            .create_execution_context(sub_runbook_block_id, None, None, None)
            .await
            .expect("Should create execution context");

        let sub_runbook_block = SubRunbook::builder()
            .id(sub_runbook_block_id)
            .name("Call Self")
            .runbook_ref(SubRunbookRef {
                id: None,
                uri: None,
                path: Some(runbook_a_id.to_string()),
            })
            .build();

        let handle = sub_runbook_block
            .execute(exec_context)
            .await
            .expect("Should start execution");

        // Wait for execution to complete
        if let Some(handle) = handle {
            let result = handle.wait_for_completion().await;
            // Should fail due to recursion
            assert_eq!(result, ExecutionResult::Failure);
        }

        // Check that we got a recursion error in events
        let events = event_bus.events();
        let has_recursion_error = events.iter().any(|e| {
            if let crate::events::GCEvent::BlockFailed { error, .. } = e {
                error.contains("Recursion detected")
            } else {
                false
            }
        });
        assert!(has_recursion_error, "Should have emitted recursion error");
    }

    /// Test: Environment variables can be exported from sub-runbook to parent
    ///
    /// Sub-runbook sets MY_VAR=hello, parent should see it after execution with export_env=true
    #[tokio::test]
    async fn test_env_export_from_sub_runbook() {
        // Define sub-runbook that sets an env var
        let sub_runbook_id = "env-setter";
        let env_block_id = Uuid::new_v4();

        let sub_runbook_content = vec![json!({
            "id": env_block_id.to_string(),
            "type": "env",
            "props": {
                "name": "EXPORTED_VAR",
                "value": "hello_from_sub"
            }
        })];

        // Parent runbook calls sub-runbook with export_env=true
        let parent_sub_block_id = Uuid::new_v4();
        let parent_content = vec![json!({
            "id": parent_sub_block_id.to_string(),
            "type": "sub-runbook",
            "props": {
                "name": "Run Env Setter",
                "runbookPath": sub_runbook_id,
                "exportEnv": true
            }
        })];

        let runbook_loader = Arc::new(
            MemoryRunbookContentLoader::new().with_runbook(sub_runbook_id, sub_runbook_content),
        );

        let (document_handle, _event_bus) = setup_test_document(runbook_loader).await;

        document_handle
            .update_document(parent_content)
            .await
            .expect("Should load document");

        // Verify env var is NOT set before execution
        let resolver_before = document_handle
            .get_context_resolver()
            .await
            .expect("Should get resolver");
        assert!(
            resolver_before.env_vars().get("EXPORTED_VAR").is_none(),
            "EXPORTED_VAR should not exist before execution"
        );

        // Execute the sub-runbook block
        let exec_context = document_handle
            .create_execution_context(parent_sub_block_id, None, None, None)
            .await
            .expect("Should create execution context");

        let sub_runbook_block = SubRunbook::builder()
            .id(parent_sub_block_id)
            .name("Run Env Setter")
            .runbook_ref(SubRunbookRef {
                id: None,
                uri: None,
                path: Some(sub_runbook_id.to_string()),
            })
            .export_env(true)
            .build();

        let handle = sub_runbook_block
            .execute(exec_context)
            .await
            .expect("Should execute");

        // Wait for completion
        if let Some(handle) = handle {
            let result = handle.wait_for_completion().await;
            assert_eq!(result, ExecutionResult::Success);
        }

        // Verify env var IS set after execution
        let resolver_after = document_handle
            .get_context_resolver()
            .await
            .expect("Should get resolver");
        assert_eq!(
            resolver_after.env_vars().get("EXPORTED_VAR"),
            Some(&"hello_from_sub".to_string()),
            "EXPORTED_VAR should be exported to parent"
        );
    }

    /// Test: Environment variables are NOT exported when export_env=false
    #[tokio::test]
    async fn test_env_not_exported_by_default() {
        // Define sub-runbook that sets an env var
        let sub_runbook_id = "env-setter-no-export";
        let env_block_id = Uuid::new_v4();

        let sub_runbook_content = vec![json!({
            "id": env_block_id.to_string(),
            "type": "env",
            "props": {
                "name": "PRIVATE_VAR",
                "value": "should_stay_private"
            }
        })];

        // Parent runbook calls sub-runbook WITHOUT export_env
        let parent_sub_block_id = Uuid::new_v4();
        let parent_content = vec![json!({
            "id": parent_sub_block_id.to_string(),
            "type": "sub-runbook",
            "props": {
                "name": "Run Env Setter",
                "runbookPath": sub_runbook_id
                // exportEnv defaults to false
            }
        })];

        let runbook_loader = Arc::new(
            MemoryRunbookContentLoader::new().with_runbook(sub_runbook_id, sub_runbook_content),
        );

        let (document_handle, _event_bus) = setup_test_document(runbook_loader).await;

        document_handle
            .update_document(parent_content)
            .await
            .expect("Should load document");

        // Execute the sub-runbook block
        let exec_context = document_handle
            .create_execution_context(parent_sub_block_id, None, None, None)
            .await
            .expect("Should create execution context");

        let sub_runbook_block = SubRunbook::builder()
            .id(parent_sub_block_id)
            .name("Run Env Setter")
            .runbook_ref(SubRunbookRef {
                id: None,
                uri: None,
                path: Some(sub_runbook_id.to_string()),
            })
            .export_env(false) // explicitly false
            .build();

        let handle = sub_runbook_block
            .execute(exec_context)
            .await
            .expect("Should execute");

        // Wait for completion
        if let Some(handle) = handle {
            let result = handle.wait_for_completion().await;
            assert_eq!(result, ExecutionResult::Success);
        }

        // Verify env var is NOT exported to parent
        let resolver_after = document_handle
            .get_context_resolver()
            .await
            .expect("Should get resolver");
        assert!(
            resolver_after.env_vars().get("PRIVATE_VAR").is_none(),
            "PRIVATE_VAR should NOT be exported to parent when export_env=false"
        );
    }

    #[tokio::test]
    async fn test_sub_runbook_script_output_available_to_subsequent_blocks() {
        let sub_runbook_id = "var-chain";
        let script1_id = Uuid::new_v4();
        let script2_id = Uuid::new_v4();

        let sub_runbook_content = vec![
            json!({
                "id": script1_id.to_string(),
                "type": "script",
                "props": {
                    "name": "Set Variable",
                    "code": "echo -n 'generated-value-12345'",
                    "interpreter": "bash",
                    "outputVariable": "myvar"
                }
            }),
            json!({
                "id": script2_id.to_string(),
                "type": "script",
                "props": {
                    "name": "Use Variable",
                    "code": "test '{{ var.myvar }}' = 'generated-value-12345' || exit 1",
                    "interpreter": "bash"
                }
            }),
        ];

        let parent_sub_block_id = Uuid::new_v4();
        let parent_content = vec![json!({
            "id": parent_sub_block_id.to_string(),
            "type": "sub-runbook",
            "props": {
                "name": "Run Var Chain",
                "runbookPath": sub_runbook_id
            }
        })];

        let runbook_loader = Arc::new(
            MemoryRunbookContentLoader::new().with_runbook(sub_runbook_id, sub_runbook_content),
        );

        let (document_handle, _event_bus) = setup_test_document(runbook_loader).await;

        document_handle
            .update_document(parent_content)
            .await
            .expect("Should load document");

        let exec_context = document_handle
            .create_execution_context(parent_sub_block_id, None, None, None)
            .await
            .expect("Should create execution context");

        let sub_runbook_block = SubRunbook::builder()
            .id(parent_sub_block_id)
            .name("Run Var Chain")
            .runbook_ref(SubRunbookRef {
                id: None,
                uri: None,
                path: Some(sub_runbook_id.to_string()),
            })
            .build();

        let handle = sub_runbook_block
            .execute(exec_context)
            .await
            .expect("Should execute");

        if let Some(handle) = handle {
            let result = handle.wait_for_completion().await;
            assert_eq!(
                result,
                ExecutionResult::Success,
                "Script 2 should see the variable set by Script 1 and succeed"
            );
        }
    }

    /// Test: Parent context (env vars) is inherited by sub-runbook
    ///
    /// Parent sets PARENT_VAR=from_parent, sub-runbook script should be able to read it
    #[tokio::test]
    async fn test_parent_env_inherited_by_sub_runbook() {
        let sub_runbook_id = "inherit-test";
        let script_id = Uuid::new_v4();

        // Sub-runbook script checks that PARENT_VAR is visible
        let sub_runbook_content = vec![json!({
            "id": script_id.to_string(),
            "type": "script",
            "props": {
                "name": "Check Parent Env",
                "code": "test \"$PARENT_VAR\" = 'from_parent' || exit 1",
                "interpreter": "bash"
            }
        })];

        // Parent sets env var then calls sub-runbook
        let env_block_id = Uuid::new_v4();
        let parent_sub_block_id = Uuid::new_v4();
        let parent_content = vec![
            json!({
                "id": env_block_id.to_string(),
                "type": "env",
                "props": {
                    "name": "PARENT_VAR",
                    "value": "from_parent"
                }
            }),
            json!({
                "id": parent_sub_block_id.to_string(),
                "type": "sub-runbook",
                "props": {
                    "name": "Run Inherit Test",
                    "runbookPath": sub_runbook_id
                }
            }),
        ];

        let runbook_loader = Arc::new(
            MemoryRunbookContentLoader::new().with_runbook(sub_runbook_id, sub_runbook_content),
        );

        let (document_handle, _event_bus) = setup_test_document(runbook_loader).await;

        document_handle
            .update_document(parent_content)
            .await
            .expect("Should load document");

        // First execute the env block to set PARENT_VAR
        let env_context = document_handle
            .create_execution_context(env_block_id, None, None, None)
            .await
            .expect("Should create env execution context");

        let env_block = crate::blocks::environment::Environment::builder()
            .id(env_block_id)
            .name("PARENT_VAR")
            .value("from_parent")
            .build();

        let env_handle = env_block
            .execute(env_context)
            .await
            .expect("Should execute env");
        if let Some(h) = env_handle {
            h.wait_for_completion().await;
        }

        // Now execute the sub-runbook - it should inherit PARENT_VAR
        let exec_context = document_handle
            .create_execution_context(parent_sub_block_id, None, None, None)
            .await
            .expect("Should create execution context");

        let sub_runbook_block = SubRunbook::builder()
            .id(parent_sub_block_id)
            .name("Run Inherit Test")
            .runbook_ref(SubRunbookRef {
                id: None,
                uri: None,
                path: Some(sub_runbook_id.to_string()),
            })
            .build();

        let handle = sub_runbook_block
            .execute(exec_context)
            .await
            .expect("Should execute");

        if let Some(handle) = handle {
            let result = handle.wait_for_completion().await;
            assert_eq!(
                result,
                ExecutionResult::Success,
                "Sub-runbook script should see PARENT_VAR from parent context"
            );
        }
    }

    /// Test: export_vars exports template variables to parent
    ///
    /// Sub-runbook sets a var via outputVariable, parent should see it with export_vars=true
    #[tokio::test]
    async fn test_vars_export_from_sub_runbook() {
        let sub_runbook_id = "var-exporter";
        let script_id = Uuid::new_v4();

        let sub_runbook_content = vec![json!({
            "id": script_id.to_string(),
            "type": "script",
            "props": {
                "name": "Set Var",
                "code": "echo -n 'exported_value'",
                "interpreter": "bash",
                "outputVariable": "exported_var"
            }
        })];

        let parent_sub_block_id = Uuid::new_v4();
        let parent_content = vec![json!({
            "id": parent_sub_block_id.to_string(),
            "type": "sub-runbook",
            "props": {
                "name": "Run Var Exporter",
                "runbookPath": sub_runbook_id,
                "exportVars": true
            }
        })];

        let runbook_loader = Arc::new(
            MemoryRunbookContentLoader::new().with_runbook(sub_runbook_id, sub_runbook_content),
        );

        let (document_handle, _event_bus) = setup_test_document(runbook_loader).await;

        document_handle
            .update_document(parent_content)
            .await
            .expect("Should load document");

        // Verify var doesn't exist before
        let resolver_before = document_handle
            .get_context_resolver()
            .await
            .expect("Should get resolver");
        assert!(
            resolver_before.vars().get("exported_var").is_none(),
            "exported_var should not exist before execution"
        );

        let exec_context = document_handle
            .create_execution_context(parent_sub_block_id, None, None, None)
            .await
            .expect("Should create execution context");

        let sub_runbook_block = SubRunbook::builder()
            .id(parent_sub_block_id)
            .name("Run Var Exporter")
            .runbook_ref(SubRunbookRef {
                id: None,
                uri: None,
                path: Some(sub_runbook_id.to_string()),
            })
            .export_vars(true)
            .build();

        let handle = sub_runbook_block
            .execute(exec_context)
            .await
            .expect("Should execute");

        if let Some(handle) = handle {
            let result = handle.wait_for_completion().await;
            assert_eq!(result, ExecutionResult::Success);
        }

        // Verify var IS exported to parent
        let resolver_after = document_handle
            .get_context_resolver()
            .await
            .expect("Should get resolver");
        assert_eq!(
            resolver_after.vars().get("exported_var"),
            Some(&"exported_value".to_string()),
            "exported_var should be exported to parent when export_vars=true"
        );
    }

    /// Test: export_vars=false does NOT export template variables
    #[tokio::test]
    async fn test_vars_not_exported_by_default() {
        let sub_runbook_id = "var-no-export";
        let script_id = Uuid::new_v4();

        let sub_runbook_content = vec![json!({
            "id": script_id.to_string(),
            "type": "script",
            "props": {
                "name": "Set Var",
                "code": "echo -n 'private_value'",
                "interpreter": "bash",
                "outputVariable": "private_var"
            }
        })];

        let parent_sub_block_id = Uuid::new_v4();
        let parent_content = vec![json!({
            "id": parent_sub_block_id.to_string(),
            "type": "sub-runbook",
            "props": {
                "name": "Run Var No Export",
                "runbookPath": sub_runbook_id
                // exportVars defaults to false
            }
        })];

        let runbook_loader = Arc::new(
            MemoryRunbookContentLoader::new().with_runbook(sub_runbook_id, sub_runbook_content),
        );

        let (document_handle, _event_bus) = setup_test_document(runbook_loader).await;

        document_handle
            .update_document(parent_content)
            .await
            .expect("Should load document");

        let exec_context = document_handle
            .create_execution_context(parent_sub_block_id, None, None, None)
            .await
            .expect("Should create execution context");

        let sub_runbook_block = SubRunbook::builder()
            .id(parent_sub_block_id)
            .name("Run Var No Export")
            .runbook_ref(SubRunbookRef {
                id: None,
                uri: None,
                path: Some(sub_runbook_id.to_string()),
            })
            .export_vars(false)
            .build();

        let handle = sub_runbook_block
            .execute(exec_context)
            .await
            .expect("Should execute");

        if let Some(handle) = handle {
            let result = handle.wait_for_completion().await;
            assert_eq!(result, ExecutionResult::Success);
        }

        // Verify var is NOT exported
        let resolver_after = document_handle
            .get_context_resolver()
            .await
            .expect("Should get resolver");
        assert!(
            resolver_after.vars().get("private_var").is_none(),
            "private_var should NOT be exported when export_vars=false"
        );
    }

    /// Test: export_cwd exports working directory to parent
    #[tokio::test]
    async fn test_cwd_export_from_sub_runbook() {
        let sub_runbook_id = "cwd-exporter";
        let dir_block_id = Uuid::new_v4();

        // Sub-runbook changes to /tmp
        let sub_runbook_content = vec![json!({
            "id": dir_block_id.to_string(),
            "type": "directory",
            "props": {
                "path": "/tmp"
            }
        })];

        let parent_sub_block_id = Uuid::new_v4();
        let parent_content = vec![json!({
            "id": parent_sub_block_id.to_string(),
            "type": "sub-runbook",
            "props": {
                "name": "Run CWD Exporter",
                "runbookPath": sub_runbook_id,
                "exportCwd": true
            }
        })];

        let runbook_loader = Arc::new(
            MemoryRunbookContentLoader::new().with_runbook(sub_runbook_id, sub_runbook_content),
        );

        let (document_handle, _event_bus) = setup_test_document(runbook_loader).await;

        document_handle
            .update_document(parent_content)
            .await
            .expect("Should load document");

        let resolver_before = document_handle
            .get_context_resolver()
            .await
            .expect("Should get resolver");
        let cwd_before = resolver_before.cwd().to_string();

        let exec_context = document_handle
            .create_execution_context(parent_sub_block_id, None, None, None)
            .await
            .expect("Should create execution context");

        let sub_runbook_block = SubRunbook::builder()
            .id(parent_sub_block_id)
            .name("Run CWD Exporter")
            .runbook_ref(SubRunbookRef {
                id: None,
                uri: None,
                path: Some(sub_runbook_id.to_string()),
            })
            .export_cwd(true)
            .build();

        let handle = sub_runbook_block
            .execute(exec_context)
            .await
            .expect("Should execute");

        if let Some(handle) = handle {
            let result = handle.wait_for_completion().await;
            assert_eq!(result, ExecutionResult::Success);
        }

        let resolver_after = document_handle
            .get_context_resolver()
            .await
            .expect("Should get resolver");

        // Parent cwd should now be /tmp (exported from sub-runbook)
        assert_ne!(
            cwd_before,
            resolver_after.cwd(),
            "CWD should have changed after export"
        );
        assert_eq!(
            resolver_after.cwd(),
            "/tmp",
            "CWD should be exported from sub-runbook"
        );
    }

    /// Test: Sub-runbook block failure propagates to parent
    #[tokio::test]
    async fn test_sub_runbook_failure_propagates() {
        let sub_runbook_id = "failing-runbook";
        let script_id = Uuid::new_v4();

        // Sub-runbook has a failing script
        let sub_runbook_content = vec![json!({
            "id": script_id.to_string(),
            "type": "script",
            "props": {
                "name": "Failing Script",
                "code": "exit 1",
                "interpreter": "bash"
            }
        })];

        let parent_sub_block_id = Uuid::new_v4();
        let parent_content = vec![json!({
            "id": parent_sub_block_id.to_string(),
            "type": "sub-runbook",
            "props": {
                "name": "Run Failing Runbook",
                "runbookPath": sub_runbook_id
            }
        })];

        let runbook_loader = Arc::new(
            MemoryRunbookContentLoader::new().with_runbook(sub_runbook_id, sub_runbook_content),
        );

        let (document_handle, event_bus) = setup_test_document(runbook_loader).await;

        document_handle
            .update_document(parent_content)
            .await
            .expect("Should load document");

        let exec_context = document_handle
            .create_execution_context(parent_sub_block_id, None, None, None)
            .await
            .expect("Should create execution context");

        let sub_runbook_block = SubRunbook::builder()
            .id(parent_sub_block_id)
            .name("Run Failing Runbook")
            .runbook_ref(SubRunbookRef {
                id: None,
                uri: None,
                path: Some(sub_runbook_id.to_string()),
            })
            .build();

        let handle = sub_runbook_block
            .execute(exec_context)
            .await
            .expect("Should execute");

        if let Some(handle) = handle {
            let result = handle.wait_for_completion().await;
            assert_eq!(
                result,
                ExecutionResult::Failure,
                "Sub-runbook should report failure when inner block fails"
            );
        }

        // Verify failure event was emitted
        let events = event_bus.events();
        let has_failure = events
            .iter()
            .any(|e| matches!(e, crate::events::GCEvent::BlockFailed { .. }));
        assert!(has_failure, "Should emit BlockFailed event");
    }

    /// Test: Indirect recursion detection (A -> B -> A)
    #[tokio::test]
    async fn test_indirect_recursion_detection() {
        let runbook_a_id = "runbook-a";
        let runbook_b_id = "runbook-b";

        // Runbook A calls Runbook B
        let sub_block_in_a = Uuid::new_v4();
        let runbook_a_content = vec![json!({
            "id": sub_block_in_a.to_string(),
            "type": "sub-runbook",
            "props": {
                "name": "Call B",
                "runbookPath": runbook_b_id
            }
        })];

        // Runbook B calls Runbook A (creates cycle)
        let sub_block_in_b = Uuid::new_v4();
        let runbook_b_content = vec![json!({
            "id": sub_block_in_b.to_string(),
            "type": "sub-runbook",
            "props": {
                "name": "Call A",
                "runbookPath": runbook_a_id
            }
        })];

        let runbook_loader = Arc::new(
            MemoryRunbookContentLoader::new()
                .with_runbook(runbook_a_id, runbook_a_content.clone())
                .with_runbook(runbook_b_id, runbook_b_content),
        );

        let (document_handle, event_bus) = setup_test_document(runbook_loader).await;

        // Load runbook A as the main document
        document_handle
            .update_document(runbook_a_content)
            .await
            .expect("Should load document");

        let exec_context = document_handle
            .create_execution_context(sub_block_in_a, None, None, None)
            .await
            .expect("Should create execution context");

        let sub_runbook_block = SubRunbook::builder()
            .id(sub_block_in_a)
            .name("Call B")
            .runbook_ref(SubRunbookRef {
                id: None,
                uri: None,
                path: Some(runbook_b_id.to_string()),
            })
            .build();

        let handle = sub_runbook_block
            .execute(exec_context)
            .await
            .expect("Should execute");

        if let Some(handle) = handle {
            let result = handle.wait_for_completion().await;
            assert_eq!(
                result,
                ExecutionResult::Failure,
                "Should fail due to indirect recursion"
            );
        }

        // Check for recursion error
        let events = event_bus.events();
        let has_recursion_error = events.iter().any(|e| {
            if let crate::events::GCEvent::BlockFailed { error, .. } = e {
                error.contains("Recursion detected")
            } else {
                false
            }
        });
        assert!(
            has_recursion_error,
            "Should detect indirect recursion (A -> B -> A)"
        );
    }

    /// Test: Nested sub-runbooks pass context correctly (A -> B -> C)
    #[tokio::test]
    async fn test_nested_sub_runbooks_context_flow() {
        let runbook_c_id = "runbook-c";
        let runbook_b_id = "runbook-b";

        // Runbook C: sets a variable
        let script_in_c = Uuid::new_v4();
        let runbook_c_content = vec![json!({
            "id": script_in_c.to_string(),
            "type": "script",
            "props": {
                "name": "Set Deep Var",
                "code": "echo -n 'from_c'",
                "interpreter": "bash",
                "outputVariable": "deep_var"
            }
        })];

        // Runbook B: calls C with export_vars, then uses the var
        let sub_block_in_b = Uuid::new_v4();
        let verify_in_b = Uuid::new_v4();
        let runbook_b_content = vec![
            json!({
                "id": sub_block_in_b.to_string(),
                "type": "sub-runbook",
                "props": {
                    "name": "Call C",
                    "runbookPath": runbook_c_id,
                    "exportVars": true
                }
            }),
            json!({
                "id": verify_in_b.to_string(),
                "type": "script",
                "props": {
                    "name": "Verify Deep Var",
                    "code": "test '{{ var.deep_var }}' = 'from_c' || exit 1",
                    "interpreter": "bash"
                }
            }),
        ];

        // Parent (A): calls B
        let sub_block_in_a = Uuid::new_v4();
        let parent_content = vec![json!({
            "id": sub_block_in_a.to_string(),
            "type": "sub-runbook",
            "props": {
                "name": "Call B",
                "runbookPath": runbook_b_id
            }
        })];

        let runbook_loader = Arc::new(
            MemoryRunbookContentLoader::new()
                .with_runbook(runbook_c_id, runbook_c_content)
                .with_runbook(runbook_b_id, runbook_b_content),
        );

        let (document_handle, _event_bus) = setup_test_document(runbook_loader).await;

        document_handle
            .update_document(parent_content)
            .await
            .expect("Should load document");

        let exec_context = document_handle
            .create_execution_context(sub_block_in_a, None, None, None)
            .await
            .expect("Should create execution context");

        let sub_runbook_block = SubRunbook::builder()
            .id(sub_block_in_a)
            .name("Call B")
            .runbook_ref(SubRunbookRef {
                id: None,
                uri: None,
                path: Some(runbook_b_id.to_string()),
            })
            .build();

        let handle = sub_runbook_block
            .execute(exec_context)
            .await
            .expect("Should execute");

        if let Some(handle) = handle {
            let result = handle.wait_for_completion().await;
            assert_eq!(
                result,
                ExecutionResult::Success,
                "Nested chain A->B->C should work: C sets var, B sees it after export"
            );
        }
    }
}
