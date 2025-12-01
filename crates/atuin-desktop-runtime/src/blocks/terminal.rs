use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

use std::io::Read;

use crate::blocks::{Block, BlockBehavior, FromDocument};
use crate::events::GCEvent;
use crate::execution::{
    BlockOutput, CancellationToken, ExecutionContext, ExecutionHandle, ExecutionStatus,
};
use crate::pty::{Pty, PtyLike};
use crate::ssh::SshPty;

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
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::Terminal(self)
    }

    async fn execute(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        let _ = context.block_started().await;

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
                BlockOutput::builder()
                    .block_id(self.id)
                    .object(
                        serde_json::to_value(metadata.clone())
                            .map_err(|e| format!("Failed to serialize PTY metadata: {}", e))?,
                    )
                    .build(),
            )
            .await;

        let handle_clone = context.handle();

        tokio::spawn(async move {
            // `run_terminal` handles all lifecycle events.
            let _ = self
                .run_terminal(context.clone(), metadata, context.cancellation_token())
                .await;
        });

        Ok(Some(handle_clone))
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
        context: ExecutionContext,
        metadata: crate::pty::PtyMetadata,
        cancellation_token: CancellationToken,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        // Get PTY store from context
        let pty_store = context
            .pty_store
            .clone()
            .ok_or("PTY store not available in execution context")?;

        // Take the cancellation receiver once at the start so we can use it throughout
        let mut cancel_rx = cancellation_token
            .take_receiver()
            .ok_or("Cancellation receiver already taken")?;

        // Open PTY based on context (local or SSH)
        let cancellation_token_clone = cancellation_token.clone();
        let pty: Box<dyn PtyLike + Send> =
            if let Some(ssh_host) = context.context_resolver.ssh_host() {
                // Parse SSH host
                let (username, hostname) = Self::parse_ssh_host(ssh_host);

                // Get SSH pool from context
                let ssh_pool = context
                    .ssh_pool
                    .clone()
                    .ok_or("SSH pool not available in execution context")?;

                // Create SSH PTY with cancellation support
                let (output_sender, mut output_receiver) = tokio::sync::mpsc::channel(100);
                let hostname_clone = hostname.clone();
                let username_clone = username.clone();
                let pty_id_str = self.id.to_string();
                let ssh_pool_clone = ssh_pool.clone();

                // Open SSH PTY with cancellation support - use the receiver we took earlier
                let ssh_result = tokio::select! {
                    result = ssh_pool_clone.open_pty(
                        &hostname_clone,
                        username_clone.as_deref(),
                        &pty_id_str,
                        output_sender.clone(),
                        80,
                        24,
                    ) => {
                        result.map_err(|e| format!("Failed to open SSH PTY: {}", e))
                    }
                    _ = &mut cancel_rx => {
                        let _ = ssh_pool_clone.close_pty(&pty_id_str).await;
                        let _ = context.block_cancelled().await;
                        return Err("SSH PTY connection cancelled".into());
                    }
                };

                let (pty_tx, resize_tx) = ssh_result?;

                // Forward SSH output to binary channel
                let context_clone = context.clone();
                let block_id = self.id;
                tokio::spawn(async move {
                    while let Some(output) = output_receiver.recv().await {
                        let _ = context_clone
                            .send_output(
                                BlockOutput::builder()
                                    .block_id(block_id)
                                    .binary(output.as_bytes().to_vec())
                                    .build(),
                            )
                            .await;
                    }

                    cancellation_token_clone.cancel();
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

                let cancellation_token_clone = cancellation_token_clone.clone();
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
                                let _ = context_clone.block_finished(Some(0), true).await;
                                cancellation_token_clone.cancel();
                                break;
                            }
                            Ok(Ok((n, buf))) => {
                                // Send raw binary data
                                let _ = context_clone
                                    .send_output(
                                        BlockOutput::builder()
                                            .block_id(block_id)
                                            .binary(buf[..n].to_vec())
                                            .build(),
                                    )
                                    .await;
                            }
                            Ok(Err(e)) => {
                                // Send error
                                let _ = context_clone
                                    .block_failed(format!("PTY read error: {}", e))
                                    .await;
                                cancellation_token_clone.cancel();
                                break;
                            }
                            Err(e) => {
                                // Task join error
                                let _ = context_clone
                                    .block_failed(format!("Task error: {}", e))
                                    .await;
                                cancellation_token_clone.cancel();
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
        // TODO: do we need this?? doesn't seem to be used on client.
        let _ = context
            .emit_gc_event(GCEvent::PtyOpened(metadata.clone()))
            .await;

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
                    .block_failed(format!("Failed to write command to PTY: {}", e))
                    .await;
            }
        }

        // For terminals, we don't wait for them to finish naturally
        // They stay running until cancelled
        // Natural termination is handled by the PTY reader loop detecting EOF, usually because the
        // user has run 'exit', pressed ctrl-d, or similar.
        tracing::trace!(
            "Awaiting terminal cancellation for block {id}",
            id = self.id
        );

        let _ = cancel_rx.await;

        tracing::debug!("Cancelling terminal execution for block {id}", id = self.id);

        // Remove PTY from store (this will also kill it)
        let _ = pty_store.remove_pty(self.id).await;
        let _ = context
            .emit_gc_event(GCEvent::PtyClosed { pty_id: self.id })
            .await;

        // If the block is still running, cancel it.
        // This will happen when the user manually cancels the block execution.
        if *context.handle().status.read().await == ExecutionStatus::Running {
            let _ = context.block_cancelled().await;
        }
        Ok(true)
    }
}
