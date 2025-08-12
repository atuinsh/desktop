use async_trait::async_trait;
use std::sync::Arc;
use tauri::ipc::Channel;
use tokio::sync::{broadcast, RwLock};

use crate::pty::{Pty, PtyMetadata};
use crate::runtime::blocks::handler::{
    BlockErrorData, BlockFinishedData, BlockHandler, BlockLifecycleEvent, BlockOutput,
    CancellationToken, ExecutionContext, ExecutionHandle, ExecutionStatus,
};
use crate::runtime::blocks::terminal::Terminal;
use crate::runtime::events::GCEvent;
use crate::runtime::pty_store::PtyLike;
use crate::runtime::ssh_pool::SshPty;
use crate::runtime::workflow::event::WorkflowEvent;

pub struct TerminalHandler;

#[async_trait]
impl BlockHandler for TerminalHandler {
    type Block = Terminal;

    fn block_type(&self) -> &'static str {
        "terminal"
    }

    fn output_variable(&self, _block: &Self::Block) -> Option<String> {
        None // Terminal blocks don't have output variables
    }

    async fn execute(
        &self,
        terminal: Terminal,
        context: ExecutionContext,
        event_sender: broadcast::Sender<WorkflowEvent>,
        output_channel: Option<Channel<BlockOutput>>,
    ) -> Result<ExecutionHandle, Box<dyn std::error::Error + Send + Sync>> {
        let handle = ExecutionHandle {
            id: terminal.id, // Use block ID as execution ID for simplicity
            block_id: terminal.id,
            cancellation_token: CancellationToken::new(),
            status: Arc::new(RwLock::new(ExecutionStatus::Running)),
            output_variable: None,
        };

        // Send started event
        let _ = event_sender.send(WorkflowEvent::BlockStarted { id: terminal.id });
        if let Some(ref ch) = output_channel {
            let _ = ch.send(BlockOutput {
                stdout: None,
                stderr: None,
                lifecycle: Some(BlockLifecycleEvent::Started),
                binary: None,
                object: None,
            });
        }

        // Emit BlockStarted event via Grand Central
        if let Some(event_bus) = &context.event_bus {
            let _ = event_bus
                .emit(GCEvent::BlockStarted {
                    block_id: terminal.id,
                    runbook_id: context.runbook_id,
                })
                .await;
        }

        let pty_id = terminal.id;
        let nanoseconds_now = time::OffsetDateTime::now_utc().unix_timestamp_nanos();
        let metadata = PtyMetadata {
            pid: pty_id,
            runbook: context.runbook_id,
            block: terminal.id.to_string(),
            created_at: nanoseconds_now as u64,
        };

        // Clone necessary values for the spawned task
        let terminal_clone = terminal.clone();
        let context_clone = context.clone();
        let handle_clone = handle.clone();
        let event_sender_clone = event_sender.clone();
        let output_channel_clone = output_channel.clone();
        let terminal_id = terminal.id;
        let runbook_id = context.runbook_id;
        let event_bus_clone = context.event_bus.clone();

        tokio::spawn(async move {
            let result = Self::run_terminal(
                terminal_clone,
                context_clone,
                metadata,
                handle_clone.cancellation_token.clone(),
                event_sender_clone,
                output_channel_clone,
            )
            .await;

            let status = match result {
                Ok(false) => ExecutionStatus::Success("Terminal session ended".to_string()),
                Ok(true) => ExecutionStatus::Cancelled,
                Err(e) => {
                    // Emit BlockFailed event via Grand Central
                    if let Some(event_bus) = &event_bus_clone {
                        let _ = event_bus
                            .emit(GCEvent::BlockFailed {
                                block_id: terminal_id,
                                runbook_id,
                                error: e.to_string(),
                            })
                            .await;
                    }
                    ExecutionStatus::Failed(e.to_string())
                }
            };

            *handle_clone.status.write().await = status;
        });

        Ok(handle)
    }

    async fn cancel(
        &self,
        handle: &ExecutionHandle,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Cancel via cancellation token
        handle.cancellation_token.cancel();
        Ok(())
    }
}

impl TerminalHandler {
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
        terminal: Terminal,
        context: ExecutionContext,
        metadata: PtyMetadata,
        cancellation_token: CancellationToken,
        event_sender: broadcast::Sender<WorkflowEvent>,
        output_channel: Option<Channel<BlockOutput>>,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        // Get PTY store from context
        let pty_store = context
            .pty_store
            .ok_or("PTY store not available in execution context")?;

        // Open PTY based on context (local or SSH)
        let pty: Box<dyn PtyLike + Send> = if let Some(ssh_host) = context.ssh_host {
            // Parse SSH host
            let (username, hostname) = Self::parse_ssh_host(&ssh_host);

            // Get SSH pool from context
            let ssh_pool = context
                .ssh_pool
                .ok_or("SSH pool not available in execution context")?;

            // Create SSH PTY
            let (output_sender, mut output_receiver) = tokio::sync::mpsc::channel(100);
            let (pty_tx, resize_tx) = ssh_pool
                .open_pty(
                    &hostname,
                    username.as_deref(),
                    &terminal.id.to_string(),
                    output_sender,
                    80,
                    24,
                )
                .await
                .map_err(|e| format!("Failed to open SSH PTY: {}", e))?;

            // Forward SSH output to binary channel
            let output_channel_ssh = output_channel.clone();
            tokio::spawn(async move {
                while let Some(output) = output_receiver.recv().await {
                    if let Some(ref ch) = output_channel_ssh {
                        let _ = ch.send(BlockOutput {
                            stdout: None,
                            stderr: None,
                            lifecycle: None,
                            binary: Some(output.as_bytes().to_vec()),
                            object: None,
                        });
                    }
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
            let pty = Pty::open(
                24,
                80,
                Some(context.cwd.clone()),
                context.env.clone(),
                metadata.clone(),
                None, // Use default shell
            )
            .await
            .map_err(|e| format!("Failed to open local PTY: {}", e))?;

            // Clone reader before moving PTY
            let reader = pty.reader.clone();

            // Spawn reader task for local PTY
            let output_channel_local = output_channel.clone();

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
                            if let Some(ref ch) = output_channel_local {
                                let _ = ch.send(BlockOutput {
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
                                });
                            }
                            break;
                        }
                        Ok(Ok((n, buf))) => {
                            // Send raw binary data
                            if let Some(ref ch) = output_channel_local {
                                let _ = ch.send(BlockOutput {
                                    stdout: None,
                                    stderr: None,
                                    lifecycle: None,
                                    binary: Some(buf[..n].to_vec()),
                                    object: None,
                                });
                            }
                        }
                        Ok(Err(e)) => {
                            // Send error
                            if let Some(ref ch) = output_channel_local {
                                let _ = ch.send(BlockOutput {
                                    stdout: None,
                                    stderr: None,
                                    lifecycle: Some(BlockLifecycleEvent::Error(BlockErrorData {
                                        message: format!("PTY read error: {}", e),
                                    })),
                                    binary: None,
                                    object: None,
                                });
                            }
                            break;
                        }
                        Err(e) => {
                            // Task join error
                            if let Some(ref ch) = output_channel_local {
                                let _ = ch.send(BlockOutput {
                                    stdout: None,
                                    stderr: None,
                                    lifecycle: Some(BlockLifecycleEvent::Error(BlockErrorData {
                                        message: format!("Task error: {}", e),
                                    })),
                                    binary: None,
                                    object: None,
                                });
                            }
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
        if let Some(event_bus) = &context.event_bus {
            let _ = event_bus.emit(GCEvent::PtyOpened(metadata.clone())).await;
        }

        // Write the command to the PTY after started event
        if !terminal.code.is_empty() {
            let command = if terminal.code.ends_with('\n') {
                terminal.code.clone()
            } else {
                format!("{}\n", terminal.code)
            };

            if let Err(e) = pty_store.write_pty(terminal.id, command.into()).await {
                // Send error event if command writing fails
                if let Some(ref ch) = output_channel {
                    let _ = ch.send(BlockOutput {
                        stdout: None,
                        stderr: None,
                        lifecycle: Some(BlockLifecycleEvent::Error(BlockErrorData {
                            message: format!("Failed to write command to PTY: {}", e),
                        })),
                        binary: None,
                        object: None,
                    });
                }
            }
        }

        // For terminals, we don't wait for them to finish naturally
        // They stay running until cancelled
        // Natural termination is handled by the PTY reader loop detecting EOF, usually because the
        // user has run 'exit', pressed ctrl-d, or similar.
        let cancellation_receiver = cancellation_token.take_receiver();
        if let Some(cancel_rx) = cancellation_receiver {
            let _ = cancel_rx.await;

            // Remove PTY from store (this will also kill it)
            let _ = pty_store.remove_pty(terminal.id).await;

            // Emit BlockCancelled event via Grand Central
            if let Some(event_bus) = &context.event_bus {
                let _ = event_bus
                    .emit(GCEvent::BlockCancelled {
                        block_id: terminal.id,
                        runbook_id: context.runbook_id,
                    })
                    .await;
            }

            // Send cancelled event to the block channel
            let _ = event_sender.send(WorkflowEvent::BlockFinished { id: terminal.id });
            if let Some(ref ch) = output_channel {
                let _ = ch.send(BlockOutput {
                    stdout: None,
                    stderr: None,
                    lifecycle: Some(BlockLifecycleEvent::Cancelled),
                    binary: None,
                    object: None,
                });
            }

            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
#[path = "terminal_test.rs"]
mod tests;
