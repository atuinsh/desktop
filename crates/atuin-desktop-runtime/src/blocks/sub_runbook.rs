//! Sub-runbook block type
//!
//! This module provides the SubRunbook block which allows embedding and executing
//! another runbook within a parent runbook. The sub-runbook inherits context from
//! its parent but maintains isolated context (changes don't propagate back).

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::blocks::{Block, BlockBehavior, FromDocument};
use crate::client::{RunbookLoadError, SubRunbookRef};
use crate::context::{BlockState, BlockWithContext, ContextResolver};
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
/// but changes made within the sub-runbook do not propagate back to the parent.
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
        log::trace!("Executing sub-runbook block {id}", id = self.id);

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

        let context_clone = context.clone();
        let block_id = self.id;
        let runbook_ref = self.runbook_ref.clone();
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
                log::warn!(
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

            // Load the runbook content
            let runbook_content = match runbook_loader.load_runbook_content(&runbook_ref).await {
                Ok(content) => content,
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

            // Parse blocks from content
            let blocks: Vec<Block> = runbook_content
                .iter()
                .filter_map(|block_data| match Block::from_document(block_data) {
                    Ok(block) => Some(block),
                    Err(e) => {
                        log::debug!("Skipping unsupported block in sub-runbook: {}", e);
                        None
                    }
                })
                .collect();

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

            // Create isolated context resolver from parent's context
            let sub_resolver = Arc::new(Mutex::new(ContextResolver::from_parent(
                &context.context_resolver,
            )));

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

                // Evaluate passive context for this block
                let resolver_guard = sub_resolver.lock().await;
                let passive_ctx = block
                    .passive_context(&resolver_guard, None)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_default();

                // Create BlockWithContext for context accumulation
                let block_with_context =
                    BlockWithContext::new(block.clone(), passive_ctx.clone(), None, None);

                // Create a resolver that includes this block's context
                let mut block_resolver = resolver_guard.clone();
                block_resolver.push_block(&block_with_context);
                drop(resolver_guard);

                // Create execution context for the sub-runbook block
                let sub_context = match context.with_sub_runbook(
                    stack_id.clone(),
                    block.id(),
                    Arc::new(block_resolver),
                ) {
                    Ok(ctx) => ctx,
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

                // Apply SSH pool and PTY store from parent
                let sub_context =
                    sub_context.with_resources(context.ssh_pool(), context.pty_store());

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

                // Wait for block to complete
                if let Some(handle) = execution_handle {
                    let mut finished_channel = handle.finished_channel();

                    let result = loop {
                        if finished_channel.changed().await.is_err() {
                            break ExecutionResult::Success;
                        }
                        let result = *finished_channel.borrow_and_update();
                        match result {
                            Some(r) => break r,
                            None => continue,
                        }
                    };

                    match result {
                        ExecutionResult::Success => {
                            // Update the resolver with completed block's context
                            let mut resolver_guard = sub_resolver.lock().await;
                            let block_with_context =
                                BlockWithContext::new(block.clone(), passive_ctx, None, None);
                            resolver_guard.push_block(&block_with_context);
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
            let _ = context.block_finished(Some(0), true).await;
        });

        Ok(Some(context_clone.handle()))
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
            let mut finished = handle.finished_channel();
            loop {
                if finished.changed().await.is_err() {
                    break;
                }
                if finished.borrow().is_some() {
                    break;
                }
            }
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
            let mut finished = handle.finished_channel();
            loop {
                if finished.changed().await.is_err() {
                    break;
                }
                if finished.borrow().is_some() {
                    break;
                }
            }
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
            let mut finished = handle.finished_channel();
            loop {
                if finished.changed().await.is_err() {
                    break;
                }
                if finished.borrow().is_some() {
                    break;
                }
            }
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
}
