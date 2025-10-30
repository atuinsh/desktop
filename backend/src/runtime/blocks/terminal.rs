use serde::{Deserialize, Serialize};
use serde_json::json;
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::runtime::blocks::{Block, BlockBehavior};

use super::FromDocument;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Terminal {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub name: String,

    #[builder(setter(into))]
    pub code: String,

    #[builder(default = true)]
    pub output_visible: bool,
}

impl FromDocument for Terminal {
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

        Ok(terminal)
    }
}

#[async_trait::async_trait]
impl BlockBehavior for Terminal {
    fn into_block(self) -> Block {
        Block::Terminal(self)
    }

    async fn execute(
        self,
        context: super::handler::ExecutionContext,
    ) -> Result<Option<super::handler::ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>>
    {
        use crate::runtime::blocks::handler::{
            BlockLifecycleEvent, BlockOutput, CancellationToken, ExecutionHandle, ExecutionStatus,
        };
        use crate::runtime::events::GCEvent;
        use crate::runtime::workflow::event::WorkflowEvent;
        use std::sync::Arc;
        use tokio::sync::RwLock;

        let handle = ExecutionHandle {
            id: self.id, // Use block ID as execution ID for simplicity
            block_id: self.id,
            cancellation_token: CancellationToken::new(),
            status: Arc::new(RwLock::new(ExecutionStatus::Running)),
            output_variable: None,
        };

        // Send started event
        let _ = context.emit_workflow_event(WorkflowEvent::BlockStarted { id: self.id });
        let _ = context
            .send_output(
                BlockOutput {
                    block_id: self.id,
                    stdout: None,
                    stderr: None,
                    lifecycle: Some(BlockLifecycleEvent::Started),
                    binary: None,
                    object: None,
                }
                .into(),
            )
            .await;

        // Emit BlockStarted event via Grand Central
        if let Some(event_bus) = &context.gc_event_bus {
            let _ = event_bus
                .emit(GCEvent::BlockStarted {
                    block_id: self.id,
                    runbook_id: context.runbook_id,
                })
                .await;
        }

        let pty_id = self.id;
        let nanoseconds_now = time::OffsetDateTime::now_utc().unix_timestamp_nanos();
        let metadata = crate::pty::PtyMetadata {
            pid: pty_id,
            runbook: context.runbook_id,
            block: self.id.to_string(),
            created_at: nanoseconds_now as u64,
        };

        let _ = context
            .send_output(
                BlockOutput {
                    block_id: self.id,
                    stdout: None,
                    stderr: None,
                    lifecycle: None,
                    binary: None,
                    object: Some(json!({
                        "pty": metadata,
                    })),
                }
                .into(),
            )
            .await;

        let handle_clone = handle.clone();

        tokio::spawn(async move {
            let result = self
                .run_terminal(
                    context.clone(),
                    metadata,
                    handle_clone.cancellation_token.clone(),
                )
                .await;

            let status = match result {
                Ok(false) => ExecutionStatus::Success("Terminal session ended".to_string()),
                Ok(true) => ExecutionStatus::Cancelled,
                Err(e) => {
                    // Emit BlockFailed event via Grand Central
                    if let Some(event_bus) = &context.gc_event_bus {
                        let _ = event_bus
                            .emit(GCEvent::BlockFailed {
                                block_id: self.id,
                                runbook_id: context.runbook_id,
                                error: e.to_string(),
                            })
                            .await;
                    }
                    ExecutionStatus::Failed(e.to_string())
                }
            };

            *handle_clone.status.write().await = status;
        });

        Ok(Some(handle))
    }
}

impl Terminal {
    /// Parse SSH host string to extract username and hostname
    fn parse_ssh_host(ssh_host: &str) -> (Option<String>, String) {
        if let Some(at_pos) = ssh_host.find('@') {
            let username = ssh_host[..at_pos].to_string();
            let host_part = &ssh_host[at_pos + 1..];
            // Remove port if present
            let hostname = if let Some(colon_pos) = host_part.find(':') {
                host_part[..colon_pos].to_string()
            } else {
                host_part.to_string()
            };
            (Some(username), hostname)
        } else {
            // No username specified, just hostname
            let hostname = if let Some(colon_pos) = ssh_host.find(':') {
                ssh_host[..colon_pos].to_string()
            } else {
                ssh_host.to_string()
            };
            (None, hostname)
        }
    }

    async fn run_terminal(
        &self,
        context: super::handler::ExecutionContext,
        metadata: crate::pty::PtyMetadata,
        cancellation_token: super::handler::CancellationToken,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        use crate::pty::Pty;
        use crate::runtime::blocks::handler::{
            BlockErrorData, BlockFinishedData, BlockLifecycleEvent, BlockOutput,
        };
        use crate::runtime::events::GCEvent;
        use crate::runtime::pty_store::PtyLike;
        use crate::runtime::ssh_pool::SshPty;
        use crate::runtime::workflow::event::WorkflowEvent;
        use std::io::Read;

        // Get PTY store from context
        let pty_store = context
            .pty_store
            .clone()
            .ok_or("PTY store not available in execution context")?;

        // Open PTY based on context (local or SSH)
        let pty: Box<dyn PtyLike + Send> =
            if let Some(ssh_host) = context.context_resolver.ssh_host() {
                // Parse SSH host
                let (username, hostname) = Self::parse_ssh_host(ssh_host);

                // Get SSH pool from context
                let ssh_pool = context
                    .ssh_pool
                    .clone()
                    .ok_or("SSH pool not available in execution context")?;

                // Create SSH PTY
                let (output_sender, mut output_receiver) = tokio::sync::mpsc::channel(100);
                let (pty_tx, resize_tx) = ssh_pool
                    .open_pty(
                        &hostname,
                        username.as_deref(),
                        &self.id.to_string(),
                        output_sender,
                        80,
                        24,
                    )
                    .await
                    .map_err(|e| format!("Failed to open SSH PTY: {}", e))?;

                // Forward SSH output to binary channel
                let context_clone = context.clone();
                let block_id = self.id;
                tokio::spawn(async move {
                    while let Some(output) = output_receiver.recv().await {
                        let _ = context_clone
                            .send_output(
                                BlockOutput {
                                    block_id,
                                    stdout: None,
                                    stderr: None,
                                    lifecycle: None,
                                    binary: Some(output.as_bytes().to_vec()),
                                    object: None,
                                }
                                .into(),
                            )
                            .await;
                    }
                });

                // Create SshPty wrapper
                Box::new(SshPty {
                    tx: pty_tx,
                    resize_tx,
                    metadata: metadata.clone(),
                    ssh_pool: ssh_pool.clone(),
                })
            } else {
                // Open local PTY
                let cwd = context.context_resolver.cwd();
                let env_vars = context.context_resolver.env_vars().clone();

                let pty = Pty::open(
                    24,
                    80,
                    Some(cwd.to_string()),
                    env_vars,
                    metadata.clone(),
                    None, // Use default shell
                )
                .await
                .map_err(|e| format!("Failed to open local PTY: {}", e))?;

                // Clone reader before moving PTY
                let reader = pty.reader.clone();

                // Spawn reader task for local PTY
                let context_clone = context.clone();
                let block_id = self.id;

                tokio::spawn(async move {
                    loop {
                        // Use blocking read in a blocking task
                        let read_result = tokio::task::spawn_blocking({
                            let reader = reader.clone();
                            move || {
                                let mut buf = [0u8; 8192];
                                match reader.lock().unwrap().read(&mut buf) {
                                    Ok(n) => Ok((n, buf)),
                                    Err(e) => Err(e),
                                }
                            }
                        })
                        .await;

                        match read_result {
                            Ok(Ok((0, _))) => {
                                // EOF - PTY terminated naturally
                                let _ = context_clone
                                    .send_output(
                                        BlockOutput {
                                            block_id,
                                            stdout: None,
                                            stderr: None,
                                            lifecycle: Some(BlockLifecycleEvent::Finished(
                                                BlockFinishedData {
                                                    exit_code: Some(0), // We don't have access to actual exit code here
                                                    success: true,
                                                },
                                            )),
                                            binary: None,
                                            object: None,
                                        }
                                        .into(),
                                    )
                                    .await;
                                break;
                            }
                            Ok(Ok((n, buf))) => {
                                // Send raw binary data
                                let _ = context_clone
                                    .send_output(
                                        BlockOutput {
                                            block_id,
                                            stdout: None,
                                            stderr: None,
                                            lifecycle: None,
                                            binary: Some(buf[..n].to_vec()),
                                            object: None,
                                        }
                                        .into(),
                                    )
                                    .await;
                            }
                            Ok(Err(e)) => {
                                // Send error
                                let _ = context_clone
                                    .send_output(
                                        BlockOutput {
                                            block_id,
                                            stdout: None,
                                            stderr: None,
                                            lifecycle: Some(BlockLifecycleEvent::Error(
                                                BlockErrorData {
                                                    message: format!("PTY read error: {}", e),
                                                },
                                            )),
                                            binary: None,
                                            object: None,
                                        }
                                        .into(),
                                    )
                                    .await;
                                break;
                            }
                            Err(e) => {
                                // Task join error
                                let _ = context_clone
                                    .send_output(
                                        BlockOutput {
                                            block_id,
                                            stdout: None,
                                            stderr: None,
                                            lifecycle: Some(BlockLifecycleEvent::Error(
                                                BlockErrorData {
                                                    message: format!("Task error: {}", e),
                                                },
                                            )),
                                            binary: None,
                                            object: None,
                                        }
                                        .into(),
                                    )
                                    .await;
                                break;
                            }
                        }
                    }
                });

                Box::new(pty)
            };

        // Add to PTY store
        pty_store
            .add_pty(pty)
            .await
            .map_err(|e| format!("Failed to add PTY to store: {}", e))?;

        // Emit PTY open event via Grand Central
        if let Some(event_bus) = &context.gc_event_bus {
            let _ = event_bus.emit(GCEvent::PtyOpened(metadata.clone())).await;
        }

        // Write the command to the PTY after started event
        if !self.code.is_empty() {
            let command = context.context_resolver.resolve_template(&self.code)?;
            let command = if command.ends_with('\n') {
                command
            } else {
                format!("{}\n", command)
            };

            if let Err(e) = pty_store.write_pty(self.id, command.into()).await {
                // Send error event if command writing fails
                let _ = context
                    .send_output(
                        BlockOutput {
                            block_id: self.id,
                            stdout: None,
                            stderr: None,
                            lifecycle: Some(BlockLifecycleEvent::Error(BlockErrorData {
                                message: format!("Failed to write command to PTY: {}", e),
                            })),
                            binary: None,
                            object: None,
                        }
                        .into(),
                    )
                    .await;
            }
        }

        // For terminals, we don't wait for them to finish naturally
        // They stay running until cancelled
        // Natural termination is handled by the PTY reader loop detecting EOF, usually because the
        // user has run 'exit', pressed ctrl-d, or similar.
        let cancellation_receiver = cancellation_token.take_receiver();
        if let Some(cancel_rx) = cancellation_receiver {
            log::trace!(
                "Awaiting terminal cancellation for block {id}",
                id = self.id
            );

            let _ = cancel_rx.await;

            log::debug!("Cancelling terminal execution for block {id}", id = self.id);

            // Remove PTY from store (this will also kill it)
            let _ = pty_store.remove_pty(self.id).await;

            // Emit BlockCancelled event via Grand Central
            if let Some(event_bus) = &context.gc_event_bus {
                let _ = event_bus
                    .emit(GCEvent::BlockCancelled {
                        block_id: self.id,
                        runbook_id: context.runbook_id,
                    })
                    .await;
            }

            // Send cancelled event to the block channel
            let _ = context.emit_workflow_event(WorkflowEvent::BlockFinished { id: self.id });
            let _ = context
                .send_output(
                    BlockOutput {
                        block_id: self.id,
                        stdout: None,
                        stderr: None,
                        lifecycle: Some(BlockLifecycleEvent::Cancelled),
                        binary: None,
                        object: None,
                    }
                    .into(),
                )
                .await;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
