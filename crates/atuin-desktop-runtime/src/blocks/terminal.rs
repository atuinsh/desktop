use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use ts_rs::TS;
use typed_builder::TypedBuilder;
use uuid::Uuid;

use std::io::Read;

use crate::blocks::{Block, BlockBehavior, FromDocument};
use crate::context::{BlockExecutionOutput, BlockVars};
use crate::events::GCEvent;
use crate::execution::{
    CancellationToken, ExecutionContext, ExecutionHandle, ExecutionStatus, StreamingBlockOutput,
};
use crate::pty::{Pty, PtyLike};
use crate::ssh::SshPty;

/// Output structure for Terminal blocks that implements BlockExecutionOutput
/// for template access to terminal output.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TerminalBlockOutput {
    /// Raw terminal output (UTF-8 lossy conversion from PTY bytes)
    pub output: String,
    /// Total number of bytes received
    pub byte_count: usize,
    /// Whether the terminal was cancelled (true) or finished naturally (false)
    pub cancelled: bool,
}

impl TerminalBlockOutput {
    /// Create a new TerminalBlockOutput from accumulated bytes
    pub fn new(bytes: Vec<u8>, cancelled: bool) -> Self {
        let byte_count = bytes.len();
        let output = String::from_utf8_lossy(&bytes).into_owned();
        Self {
            output,
            byte_count,
            cancelled,
        }
    }
}

impl BlockExecutionOutput for TerminalBlockOutput {
    fn get_template_value(&self, key: &str) -> Option<minijinja::Value> {
        match key {
            "output" => Some(minijinja::Value::from(self.output.clone())),
            "byte_count" => Some(minijinja::Value::from(self.byte_count)),
            "cancelled" => Some(minijinja::Value::from(self.cancelled)),
            _ => None,
        }
    }

    fn enumerate_template_keys(&self) -> minijinja::value::Enumerator {
        minijinja::value::Enumerator::Str(&["output", "byte_count", "cancelled"])
    }
}

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

    #[builder(default = 20)]
    pub rows: u16,

    #[builder(default = 120)]
    pub cols: u16,
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
            .rows(
                props
                    .get("rows")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u16)
                    .unwrap_or(20),
            )
            .cols(
                props
                    .get("cols")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u16)
                    .unwrap_or(120),
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
                StreamingBlockOutput::builder()
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
    fn parse_ssh_host(ssh_host: &str) -> (Option<String>, String) {
        if let Some(at_pos) = ssh_host.find('@') {
            let username = ssh_host[..at_pos].to_string();
            let host_part = ssh_host[at_pos + 1..].to_string();
            (Some(username), host_part)
        } else {
            (None, ssh_host.to_string())
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

        let output_accumulator: Arc<RwLock<Vec<u8>>> = Arc::new(RwLock::new(Vec::new()));

        let templated_code = if !self.code.is_empty() {
            context.context_resolver.resolve_template(&self.code)?
        } else {
            String::new()
        };

        let uses_output_vars = templated_code.contains("ATUIN_OUTPUT_VARS");

        let ssh_host = context.context_resolver.ssh_host().cloned();
        let fs_var_handle: Option<crate::context::fs_var::FsVarHandle>;
        let remote_var_path: Option<String>;

        if let Some(ref host) = ssh_host {
            fs_var_handle = None;

            if uses_output_vars {
                let (username, hostname) = Self::parse_ssh_host(host);
                let ssh_pool = context
                    .ssh_pool
                    .clone()
                    .ok_or("SSH pool not available in execution context")?;

                let remote_path = ssh_pool
                    .create_temp_file(&hostname, username.as_deref(), "atuin-desktop-vars")
                    .await
                    .map_err(|e| format!("Failed to create remote temp file: {}", e))?;

                remote_var_path = Some(remote_path);
            } else {
                remote_var_path = None;
            }
        } else {
            if uses_output_vars {
                fs_var_handle =
                    Some(crate::context::fs_var::setup().map_err(|e| {
                        format!("Failed to setup temp file for output variables: {}", e)
                    })?);
            } else {
                fs_var_handle = None;
            }
            remote_var_path = None;
        }

        let cancellation_token_clone = cancellation_token.clone();
        let pty: Box<dyn PtyLike + Send> = if let Some(ref host) = ssh_host {
            let (username, hostname) = Self::parse_ssh_host(host);
            let ssh_pool = context
                .ssh_pool
                .clone()
                .ok_or("SSH pool not available in execution context")?;

            let (output_sender, mut output_receiver) = tokio::sync::mpsc::channel(100);
            let hostname_clone = hostname.clone();
            let username_clone = username.clone();
            let pty_id_str = self.id.to_string();
            let ssh_pool_clone = ssh_pool.clone();
            let remote_path_clone = remote_var_path.clone();

            let initial_cols = self.cols;
            let initial_rows = self.rows;
            let ssh_result = tokio::select! {
                result = ssh_pool_clone.open_pty(
                    &hostname_clone,
                    username_clone.as_deref(),
                    &pty_id_str,
                    output_sender.clone(),
                    initial_cols,
                    initial_rows,
                ) => {
                    result.map_err(|e| format!("Failed to open SSH PTY: {}", e))
                }
                _ = &mut cancel_rx => {
                    let _ = ssh_pool_clone.close_pty(&pty_id_str).await;
                    let _ = context.block_cancelled().await;
                    if let Some(ref path) = remote_path_clone {
                        let _ = ssh_pool_clone.delete_file(&hostname_clone, username_clone.as_deref(), path).await;
                    }
                    return Err("SSH PTY connection cancelled".into());
                }
            };

            let (pty_tx, resize_tx) = ssh_result?;

            // Forward SSH output to binary channel and accumulate
            let context_clone = context.clone();
            let block_id = self.id;
            let output_accumulator_clone = output_accumulator.clone();
            tokio::spawn(async move {
                while let Some(output) = output_receiver.recv().await {
                    let bytes = output.as_bytes().to_vec();

                    // Accumulate output
                    output_accumulator_clone
                        .write()
                        .await
                        .extend_from_slice(&bytes);

                    let _ = context_clone
                        .send_output(
                            StreamingBlockOutput::builder()
                                .block_id(block_id)
                                .binary(bytes)
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
            let mut env_vars = context.context_resolver.env_vars().clone();

            // Add ATUIN_OUTPUT_VARS to environment if we have a handle
            if let Some(ref handle) = fs_var_handle {
                let var_path = handle.path().to_string_lossy().to_string();
                env_vars.insert("ATUIN_OUTPUT_VARS".to_string(), var_path);
            }

            let pty = Pty::open(
                self.rows,
                self.cols,
                Some(cwd.to_string()),
                env_vars,
                metadata.clone(),
                None,
            )
            .await
            .map_err(|e| format!("Failed to open local PTY: {}", e))?;

            // Clone reader before moving PTY
            let reader = pty.reader.clone();

            // Spawn reader task for local PTY
            let context_clone = context.clone();
            let block_id = self.id;

            let cancellation_token_clone = cancellation_token_clone.clone();
            let output_accumulator_clone = output_accumulator.clone();
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
                            let bytes = buf[..n].to_vec();

                            // Accumulate output
                            output_accumulator_clone
                                .write()
                                .await
                                .extend_from_slice(&bytes);

                            // Send raw binary data
                            let _ = context_clone
                                .send_output(
                                    StreamingBlockOutput::builder()
                                        .block_id(block_id)
                                        .binary(bytes)
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

        if let Some(ref remote_path) = remote_var_path {
            let export_cmd = format!("export ATUIN_OUTPUT_VARS='{}'\n", remote_path);
            if let Err(e) = pty_store.write_pty(self.id, export_cmd.into()).await {
                tracing::warn!("Failed to write export command to SSH PTY: {}", e);
            }
        }

        if !templated_code.is_empty() {
            let command = if templated_code.ends_with('\n') {
                templated_code.clone()
            } else {
                format!("{}\n", templated_code)
            };

            if let Err(e) = pty_store.write_pty(self.id, command.into()).await {
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

        // Read variables from temp file (local) or remote file (SSH)
        if let Some(handle) = fs_var_handle {
            // Local terminal
            match crate::context::fs_var::finalize(handle).await {
                Ok(vars) if !vars.is_empty() => {
                    let block_id = self.id;
                    let _ = context
                        .update_active_context(block_id, move |ctx| {
                            for (key, value) in vars.into_iter() {
                                ctx.add_var(key, value, "(terminal variable output)".to_string());
                            }
                        })
                        .await;
                }
                Ok(_) => {
                    // No variables written, that's fine
                }
                Err(e) => {
                    tracing::warn!("Failed to read terminal output variables: {}", e);
                }
            }
        } else if let (Some(ref host), Some(ref remote_path)) = (&ssh_host, &remote_var_path) {
            let (username, hostname) = Self::parse_ssh_host(host);
            let ssh_pool = context.ssh_pool.clone().unwrap();

            match ssh_pool
                .read_file(&hostname, username.as_deref(), remote_path)
                .await
            {
                Ok(contents) => {
                    let vars = crate::context::fs_var::parse_vars(&contents);
                    if !vars.is_empty() {
                        let block_id = self.id;
                        let _ = context
                            .update_active_context(block_id, move |ctx| {
                                for (key, value) in vars.into_iter() {
                                    ctx.add_var(
                                        key,
                                        value,
                                        "(terminal variable output)".to_string(),
                                    );
                                }
                            })
                            .await;
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to read remote terminal output variables: {}", e);
                }
            }

            let _ = ssh_pool
                .delete_file(&hostname, username.as_deref(), remote_path)
                .await;
        }

        // Remove PTY from store (this will also kill it)
        let _ = pty_store.remove_pty(self.id).await;
        let _ = context
            .emit_gc_event(GCEvent::PtyClosed { pty_id: self.id })
            .await;

        // Determine if the terminal was cancelled (still running = cancelled by user)
        let was_cancelled = *context.handle().status.read().await == ExecutionStatus::Running;

        // Store accumulated output for template access
        let accumulated_bytes = output_accumulator.read().await.clone();
        let terminal_output = TerminalBlockOutput::new(accumulated_bytes, was_cancelled);
        let _ = context.set_block_output(terminal_output).await;

        // If the block is still running, cancel it.
        // This will happen when the user manually cancels the block execution.
        if was_cancelled {
            let _ = context.block_cancelled().await;
        }
        Ok(true)
    }
}
