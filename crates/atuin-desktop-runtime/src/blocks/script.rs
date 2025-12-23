use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::task::JoinHandle;
use ts_rs::TS;
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::blocks::{Block, BlockBehavior};
use crate::context::{fs_var, BlockExecutionOutput, BlockVars};
use crate::execution::{
    CancellationToken, ExecutionContext, ExecutionHandle, ExecutionStatus, StreamingBlockOutput,
};
use crate::ssh::OutputLine as SessionOutputLine;

use super::FromDocument;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Script {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub name: String,

    #[builder(setter(into))]
    pub code: String,

    #[builder(setter(into))]
    pub interpreter: String,

    #[builder(setter(into))]
    pub output_variable: Option<String>,

    #[builder(default = true)]
    pub output_visible: bool,
}

impl FromDocument for Script {
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

        let script = Script::builder()
            .id(id)
            .name(
                props
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Script")
                    .to_string(),
            )
            .code(
                props
                    .get("code")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            )
            .interpreter(
                props
                    .get("interpreter")
                    .and_then(|v| v.as_str())
                    .unwrap_or("bash")
                    .to_string(),
            )
            .output_variable(
                props
                    .get("outputVariable")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .and_then(|s| if s.is_empty() { None } else { Some(s) }),
            )
            .output_visible(
                props
                    .get("outputVisible")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
            )
            .build();

        Ok(script)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OutputLine {
    pub is_stdout: bool,
    pub text: String,
}

impl OutputLine {
    pub fn stdout(text: String) -> Self {
        Self {
            is_stdout: true,
            text,
        }
    }

    pub fn stderr(text: String) -> Self {
        Self {
            is_stdout: false,
            text,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, TypedBuilder, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScriptExecutionOutput {
    pub exit_code: Option<i32>,
    pub output: Vec<OutputLine>,
}

impl ScriptExecutionOutput {
    pub fn stdout(&self) -> Option<String> {
        if self.output.is_empty() {
            return None;
        }

        Some(
            self.output
                .iter()
                .filter(|line| line.is_stdout)
                .map(|line| line.text.clone())
                .collect::<Vec<String>>()
                .join(""),
        )
    }

    pub fn stderr(&self) -> Option<String> {
        if self.output.is_empty() {
            return None;
        }

        Some(
            self.output
                .iter()
                .filter(|line| !line.is_stdout)
                .map(|line| line.text.clone())
                .collect::<Vec<String>>()
                .join(""),
        )
    }

    pub fn combined_out(&self) -> Option<String> {
        if self.output.is_empty() {
            return None;
        }

        Some(
            self.output
                .iter()
                .map(|line| line.text.clone())
                .collect::<Vec<String>>()
                .join(""),
        )
    }
}

impl BlockExecutionOutput for ScriptExecutionOutput {
    fn get_template_value(&self, key: &str) -> Option<minijinja::Value> {
        match key {
            "exit_code" => Some(minijinja::Value::from_serialize(self.exit_code)),
            "stdout" => Some(minijinja::Value::from_serialize(self.stdout())),
            "stderr" => Some(minijinja::Value::from_serialize(self.stderr())),
            "combined" => Some(minijinja::Value::from_serialize(self.combined_out())),
            _ => None,
        }
    }

    fn enumerate_template_keys(&self) -> minijinja::value::Enumerator {
        minijinja::value::Enumerator::Str(&["exit_code", "stdout", "stderr", "combined"])
    }
}

#[async_trait]
impl BlockBehavior for Script {
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::Script(self)
    }

    async fn execute(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        tracing::trace!("Executing script block {id}", id = self.id);

        tracing::trace!(
            "Script block {id} execution handle created; ID = {handle_id}",
            id = self.id,
            handle_id = context.handle().id
        );

        let var_name = match self.output_variable {
            Some(ref v) => match context.context_resolver.resolve_template(v) {
                Ok(resolved) => Some(resolved),
                Err(e) => return Err(Box::new(e)),
            },
            None => None,
        };

        let context_clone = context.clone();
        tokio::spawn(async move {
            let (exit_code, captured_lines, vars) = self
                .run_script(context.clone(), context.cancellation_token())
                .await;

            tracing::trace!(
                "Script block {id} execution completed; Exit code = {exit_code}",
                id = self.id,
                exit_code = exit_code
                    .as_ref()
                    .map(|c| c.to_string())
                    .unwrap_or("(none)".to_string())
            );

            // Determine status based on exit code
            // IMPORTANT: We must call update_active_context BEFORE block_finished/block_failed
            // to avoid a race condition. The non-interactive executor is fast enough that it
            // will move to the next block immediately after receiving the Finished event,
            // before the context has been updated.
            match exit_code {
                Ok(0) => {
                    if let Some(vars) = vars {
                        let _ = context
                            .update_active_context(self.id, move |ctx| {
                                for (key, value) in vars.into_iter() {
                                    ctx.add_var(key, value, "(script variable output)".to_string());
                                }
                            })
                            .await;
                    }

                    if let Some(var_name) = var_name {
                        let stdout = captured_lines
                            .iter()
                            .filter(|line| line.is_stdout)
                            .map(|line| line.text.clone())
                            .collect::<Vec<String>>()
                            .join("\n");

                        let _ = context
                            .update_active_context(self.id, move |ctx| {
                                tracing::trace!(
                                    "Storing output variable {var_name} for script block {block_id}",
                                    var_name = var_name,
                                    block_id = self.id
                                );
                                ctx.add_var(var_name, stdout, "(script output)".to_string());
                            })
                            .await;
                    }

                    // Store execution output in context
                    let _ = context
                        .set_block_output(ScriptExecutionOutput {
                            exit_code: Some(0),
                            output: captured_lines,
                        })
                        .await;

                    // Signal completion AFTER context is updated
                    let _ = context.block_finished(Some(0), true).await;

                    ExecutionStatus::Success
                }
                Ok(code) => {
                    // Store execution output in context (failed)
                    let _ = context
                        .set_block_output(ScriptExecutionOutput {
                            exit_code: Some(code),
                            output: captured_lines,
                        })
                        .await;

                    // Signal failure AFTER context is updated
                    let _ = context
                        .block_failed(format!("Script exited with code {}", code))
                        .await;

                    ExecutionStatus::Failed(format!("Process exited with code {}", code))
                }
                Err(e) => {
                    let _ = context
                        .set_block_output(ScriptExecutionOutput {
                            exit_code: None,
                            output: Vec::new(),
                        })
                        .await;
                    let _ = context.block_failed(e.to_string()).await;
                    ExecutionStatus::Failed(e.to_string())
                }
            };
        });

        Ok(Some(context_clone.handle()))
    }
}

impl Script {
    fn parse_ssh_host(ssh_host: &str) -> (Option<String>, String) {
        if let Some(at_pos) = ssh_host.find('@') {
            let username = ssh_host[..at_pos].to_string();
            let host_part = ssh_host[at_pos + 1..].to_string();
            (Some(username), host_part)
        } else {
            (None, ssh_host.to_string())
        }
    }

    /// Determine the correct flag for passing code to the interpreter
    fn get_interpreter_flag(interpreter: &str) -> Option<&'static str> {
        let interpreter = Self::get_program_name(interpreter);

        match interpreter {
            "ruby" | "node" | "nodejs" | "perl" | "lua" => Some("-e"),
            "php" => Some("-r"),
            "bash" | "sh" | "zsh" | "fish" => Some("-c"),
            s if s.starts_with("python") => Some("-c"),
            _ => None,
        }
    }

    fn get_program_name(path: &str) -> &str {
        std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path)
    }

    fn has_flag(args: &[&str], char_flag: char) -> bool {
        args.iter().any(|arg| {
            if arg.starts_with("--") {
                false
            } else if arg.starts_with('-') {
                arg.chars().any(|c| c == char_flag)
            } else {
                false
            }
        })
    }

    async fn run_script(
        &self,
        context: ExecutionContext,
        cancellation_token: CancellationToken,
    ) -> (
        Result<i32, Box<dyn std::error::Error + Send + Sync>>,
        Vec<OutputLine>,
        Option<HashMap<String, String>>,
    ) {
        // Send started lifecycle event to output channel
        tracing::trace!(
            "Sending started lifecycle event to output channel for script block {id}",
            id = self.id
        );

        let _ = context.block_started().await;

        // Template the script code
        let code = context
            .context_resolver
            .resolve_template(&self.code)
            .unwrap_or_else(|e| {
                tracing::warn!("Templating error in script {id}: {e}", id = self.id, e = e);
                self.code.clone()
            });

        // Check if SSH execution is needed
        let ssh_host = context.context_resolver.ssh_host().cloned();
        if let Some(ssh_host) = ssh_host {
            tracing::trace!(
                "Executing SSH script for script block {id} with SSH host {ssh_host}",
                id = self.id,
                ssh_host = ssh_host
            );

            return self
                .execute_ssh_script(&code, &ssh_host, context, cancellation_token)
                .await;
        }

        // Local execution
        let cwd = context.context_resolver.cwd().to_string();
        let env_vars = context.context_resolver.env_vars();

        // Parse interpreter string into program and args
        let parts: Vec<&str> = self.interpreter.split_whitespace().collect();
        let binding = self.interpreter.as_str();
        let program = parts.first().copied().unwrap_or(binding);
        let args = if parts.len() > 1 { &parts[1..] } else { &[] };

        let program_name = Self::get_program_name(program);
        let mut final_args: Vec<String> = args.iter().map(|s| s.to_string()).collect();

        // For shells, ensure we run as a login shell if no other login args are present
        // This ensures environment variables (like from .bash_profile) are loaded
        if ["bash", "zsh", "sh", "fish"].contains(&program_name)
            && !Self::has_flag(args, 'l')
            && !args.contains(&"--login")
        {
            final_args.insert(0, "-l".to_string());
        }

        let mut cmd = Command::new(program);

        // Add interpreter flag if not already present
        if let Some(flag) = Self::get_interpreter_flag(program) {
            // Get the char flag (e.g. 'c' from "-c")
            if let Some(char_flag) = flag.chars().last() {
                if !Self::has_flag(args, char_flag) {
                    final_args.push(flag.to_string());
                }
            }
        }

        let fs_var = fs_var::setup();
        if let Err(e) = fs_var {
            let _ = context
                .block_failed(format!(
                    "Failed to setup temporary file for output variables: {}",
                    e
                ))
                .await;
            return (Err(e), Vec::new(), None);
        }
        let fs_var = fs_var.unwrap();

        cmd.args(final_args);
        cmd.arg(&code);
        cmd.current_dir(&cwd);
        cmd.envs(env_vars);
        cmd.env("ATUIN_OUTPUT_VARS", fs_var.path().as_os_str());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.stdin(Stdio::null());

        #[cfg(unix)]
        {
            cmd.process_group(0);
        }

        tracing::trace!("Spawning process for script block {id}", id = self.id,);

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                let _ = context
                    .block_failed(format!("Failed to spawn process: {}", e))
                    .await;
                return (Err(e.into()), Vec::new(), None);
            }
        };
        let pid = child.id();

        let captured_output = Arc::new(RwLock::new(Vec::new()));

        let mut stdout_task: Option<JoinHandle<()>> = None;
        let mut stderr_task: Option<JoinHandle<()>> = None;

        // Capture stdout
        if let Some(stdout) = child.stdout.take() {
            let context_clone = context.clone();
            let capture_stdout = captured_output.clone();
            let block_id = self.id;

            stdout_task = Some(tokio::spawn(async move {
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();
                while let Ok(n) = reader.read_line(&mut line).await {
                    if n == 0 {
                        break;
                    }
                    tracing::trace!(
                        "Sending stdout line to output channel for script block {id}",
                        id = block_id
                    );

                    let _ = context_clone
                        .send_output(
                            StreamingBlockOutput::builder()
                                .block_id(block_id)
                                .stdout(line.clone())
                                .build(),
                        )
                        .await;
                    let mut captured = capture_stdout.write().await;
                    captured.push(OutputLine::stdout(line.clone()));
                    line.clear();
                }
            }));
        }

        // Stream stderr
        if let Some(stderr) = child.stderr.take() {
            let context_clone = context.clone();
            let capture_stderr = captured_output.clone();
            let block_id = self.id;

            stderr_task = Some(tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                while let Ok(n) = reader.read_line(&mut line).await {
                    if n == 0 {
                        break;
                    }
                    tracing::trace!(
                        "Sending stderr line to output channel for script block {id}",
                        id = block_id
                    );

                    let _ = context_clone
                        .send_output(
                            StreamingBlockOutput::builder()
                                .block_id(block_id)
                                .stderr(line.clone())
                                .build(),
                        )
                        .await;
                    let mut captured = capture_stderr.write().await;
                    captured.push(OutputLine::stderr(line.clone()));
                    line.clear();
                }
            }));
        }

        // Wait for completion or cancellation
        let cancellation_receiver = cancellation_token.take_receiver();
        let exit_code = if let Some(cancel_rx) = cancellation_receiver {
            tokio::select! {
                _ = cancel_rx => {
                    tracing::trace!("Process for script block {id} cancelled", id = self.id);

                    // Kill the process
                    if let Some(pid) = pid {
                        #[cfg(unix)]
                        {
                            use nix::sys::signal::{self, Signal};
                            use nix::unistd::Pid;
                            tracing::trace!("Sending SIGTERM to process {pid}", pid = pid);
                            // Send SIGTERM to the process group
                            let _ = signal::kill(Pid::from_raw(-(pid as i32)), Signal::SIGTERM);
                        }
                        #[cfg(windows)]
                        {
                            let _ = child.kill().await;
                        }
                    }

                    if let Some(stdout_task) = stdout_task {
                        tracing::trace!("Waiting for stdout reader to finish");
                        let _ = stdout_task.await;
                    }
                    if let Some(stderr_task) = stderr_task {
                        tracing::trace!("Waiting for stderr reader to finish");
                        let _ = stderr_task.await;
                    }

                    tracing::trace!("Reading captured output");
                    let captured = captured_output.read().await.clone();

                    let _ = context.block_cancelled().await;

                    return (Err("Script execution cancelled".into()), captured, None);
                }
                result = child.wait() => {
                    match result {
                        Ok(status) => status.code().unwrap_or(-1),
                        Err(e) => {
                            if let Some(stdout_task) = stdout_task {
                                tracing::trace!("Waiting for stdout reader to finish");
                                let _ = stdout_task.await;
                            }
                            if let Some(stderr_task) = stderr_task {
                                tracing::trace!("Waiting for stderr reader to finish");
                                let _ = stderr_task.await;
                            }

                            tracing::trace!("Reading captured output");
                            let captured = captured_output.read().await.clone();
                            let _ = context.block_failed(format!("Failed to wait for process: {}", e)).await;
                            return (Err(format!("Failed to wait for process: {}", e).into()), captured, None);
                        }
                    }
                }
            }
        } else {
            match child.wait().await {
                Ok(status) => status.code().unwrap_or(-1),
                Err(e) => {
                    if let Some(stdout_task) = stdout_task {
                        tracing::trace!("Waiting for stdout reader to finish");
                        let _ = stdout_task.await;
                    }
                    if let Some(stderr_task) = stderr_task {
                        tracing::trace!("Waiting for stderr reader to finish");
                        let _ = stderr_task.await;
                    }

                    tracing::trace!("Reading captured output");
                    let captured = captured_output.read().await.clone();
                    let _ = context
                        .block_failed(format!("Failed to wait for process: {}", e))
                        .await;
                    return (
                        Err(format!("Failed to wait for process: {}", e).into()),
                        captured,
                        None,
                    );
                }
            }
        };

        if let Some(stdout_task) = stdout_task {
            let _ = stdout_task.await;
        }
        if let Some(stderr_task) = stderr_task {
            let _ = stderr_task.await;
        }

        tracing::trace!("Reading captured output");
        let captured = captured_output.read().await.clone();
        if let Ok(vars) = fs_var::finalize(fs_var).await {
            (Ok(exit_code), captured, Some(vars))
        } else {
            (
                Err("Failed to finalize temporary file for output variables".into()),
                captured,
                None,
            )
        }
    }

    async fn execute_ssh_script(
        &self,
        code: &str,
        ssh_host: &str,
        context: ExecutionContext,
        cancellation_token: CancellationToken,
    ) -> (
        Result<i32, Box<dyn std::error::Error + Send + Sync>>,
        Vec<OutputLine>,
        Option<HashMap<String, String>>,
    ) {
        let (username, hostname) = Self::parse_ssh_host(ssh_host);

        let ssh_pool = match &context.ssh_pool {
            Some(pool) => pool,
            None => {
                let error_msg = "SSH pool not available in execution context";
                let _ = context.block_failed(error_msg.to_string()).await;
                return (Err(error_msg.into()), Vec::new(), None);
            }
        };

        let uses_output_vars = code.contains("ATUIN_OUTPUT_VARS");

        let remote_temp_path: Option<String> = if uses_output_vars {
            match ssh_pool
                .create_temp_file(&hostname, username.as_deref(), "atuin-desktop-vars")
                .await
            {
                Ok(path) => Some(path),
                Err(e) => {
                    let error_msg = format!("Failed to create remote temp file: {}", e);
                    let _ = context.block_failed(error_msg.clone()).await;
                    return (Err(error_msg.into()), Vec::new(), None);
                }
            }
        } else {
            None
        };

        let code_to_run = if let Some(ref path) = remote_temp_path {
            format!("export ATUIN_OUTPUT_VARS='{}'\n{}", path, code)
        } else {
            code.to_string()
        };

        let channel_id = self.id.to_string();
        let (output_sender, mut output_receiver) = mpsc::channel::<SessionOutputLine>(100);
        let (result_tx, result_rx) = oneshot::channel::<()>();

        let captured_output = Arc::new(RwLock::new(Vec::new()));
        let captured_output_clone = captured_output.clone();

        let mut cancel_rx = match cancellation_token.take_receiver() {
            Some(rx) => rx,
            None => {
                let error_msg = "Cancellation receiver already taken";
                let _ = context.block_failed(error_msg.to_string()).await;
                if let Some(ref path) = remote_temp_path {
                    let _ = ssh_pool
                        .delete_file(&hostname, username.as_deref(), path)
                        .await;
                }
                return (Err(error_msg.into()), Vec::new(), None);
            }
        };

        let exec_result = tokio::select! {
            result = ssh_pool.exec(
                &hostname,
                username.as_deref(),
                &self.interpreter,
                &code_to_run,
                &channel_id,
                output_sender,
                result_tx,
            ) => {
                result
            }
            _ = &mut cancel_rx => {
                tracing::trace!("Sending cancel to SSH execution for channel {channel_id}");
                let _ = ssh_pool.exec_cancel(&channel_id).await;
                let _ = context.block_cancelled().await;
                if let Some(ref path) = remote_temp_path {
                    let _ = ssh_pool.delete_file(&hostname, username.as_deref(), path).await;
                }
                return (Err("SSH script execution cancelled before start".into()), Vec::new(), None);
            }
        };

        if let Err(e) = exec_result {
            let error_msg = format!("Failed to start SSH execution: {}", e);
            let _ = context.block_failed(error_msg.to_string()).await;
            if let Some(ref path) = remote_temp_path {
                let _ = ssh_pool.delete_file(&hostname, username.as_deref(), path).await;
            }
            return (Err(error_msg.into()), Vec::new(), None);
        }
        let context_clone = context.clone();
        let block_id = self.id;
        let ssh_pool_clone = ssh_pool.clone();
        let channel_id_clone = channel_id.clone();

        let output_task = tokio::spawn(async move {
            while let Some(line) = output_receiver.recv().await {
                let mut text = line.inner().to_string();

                if !text.ends_with('\n') {
                    text.push('\n');
                }

                let streaming_output = if line.is_stdout() {
                    StreamingBlockOutput::builder()
                        .block_id(block_id)
                        .stdout(text.clone())
                        .build()
                } else {
                    StreamingBlockOutput::builder()
                        .block_id(block_id)
                        .stderr(text.clone())
                        .build()
                };

                let _ = context_clone.send_output(streaming_output).await;
                let mut captured = captured_output_clone.write().await;
                if line.is_stdout() {
                    captured.push(OutputLine::stdout(text));
                } else {
                    captured.push(OutputLine::stderr(text));
                }
            }
        });

        let exit_code = tokio::select! {
            _ = cancel_rx => {
                let _ = ssh_pool_clone.exec_cancel(&channel_id_clone).await;
                let _ = output_task.await;
                let captured = captured_output.read().await.clone();

                let _ = context.block_cancelled().await;
                if let Some(ref path) = remote_temp_path {
                    let _ = ssh_pool.delete_file(&hostname, username.as_deref(), path).await;
                }
                return (Err("SSH script execution cancelled".into()), captured, None);
            }
            _ = result_rx => {
                0
            }
        };

        let _ = output_task.await;
        let captured = captured_output.read().await.clone();

        let vars = if let Some(ref path) = remote_temp_path {
            match ssh_pool
                .read_file(&hostname, username.as_deref(), path)
                .await
            {
                Ok(contents) => Some(fs_var::parse_vars(&contents)),
                Err(e) => {
                    tracing::warn!("Failed to read remote temp file for variables: {}", e);
                    None
                }
            }
        } else {
            None
        };

        if let Some(ref path) = remote_temp_path {
            let _ = ssh_pool
                .delete_file(&hostname, username.as_deref(), path)
                .await;
        }

        (Ok(exit_code), captured, vars)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ContextResolver;
    use crate::document::actor::DocumentCommand;
    use crate::document::DocumentHandle;
    use crate::events::MemoryEventBus;
    use crate::execution::ExecutionStatus;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    fn create_test_script(code: &str, interpreter: &str) -> Script {
        Script::builder()
            .id(Uuid::new_v4())
            .name("Test Script")
            .code(code)
            .interpreter(interpreter)
            .output_variable(None)
            .build()
    }

    fn create_test_context() -> ExecutionContext {
        let (tx, _rx) = mpsc::unbounded_channel::<DocumentCommand>();
        let document_handle = DocumentHandle::from_raw(
            "test-runbook".to_string(),
            tx,
            Arc::new(MemoryEventBus::new()),
        );
        let context_resolver = ContextResolver::new();

        let block_id = Uuid::new_v4();
        ExecutionContext::builder()
            .block_id(block_id)
            .runbook_id(Uuid::new_v4())
            .document_handle(document_handle)
            .context_resolver(Arc::new(context_resolver))
            .handle(ExecutionHandle::new(block_id))
            .build()
    }

    fn create_test_context_with_vars(vars: Vec<(&str, &str)>) -> ExecutionContext {
        let (tx, _rx) = mpsc::unbounded_channel::<DocumentCommand>();
        let document_handle = DocumentHandle::from_raw(
            "test-runbook".to_string(),
            tx,
            Arc::new(MemoryEventBus::new()),
        );

        let vars_map: HashMap<String, String> = vars
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        let context_resolver = ContextResolver::with_vars(vars_map);

        let block_id = Uuid::new_v4();
        ExecutionContext::builder()
            .block_id(block_id)
            .runbook_id(Uuid::new_v4())
            .document_handle(document_handle)
            .context_resolver(Arc::new(context_resolver))
            .handle(ExecutionHandle::new(block_id))
            .build()
    }

    fn create_test_context_with_event_bus(
        block_id: Uuid,
        event_bus: Arc<MemoryEventBus>,
    ) -> ExecutionContext {
        let (tx, _rx) = mpsc::unbounded_channel::<DocumentCommand>();
        let document_handle =
            DocumentHandle::from_raw("test-runbook".to_string(), tx, event_bus.clone());
        let context_resolver = ContextResolver::new();

        ExecutionContext::builder()
            .block_id(block_id)
            .runbook_id(Uuid::new_v4())
            .document_handle(document_handle)
            .context_resolver(Arc::new(context_resolver))
            .gc_event_bus(event_bus)
            .handle(ExecutionHandle::new(block_id))
            .build()
    }

    #[tokio::test]
    async fn test_successful_script_execution() {
        let script = create_test_script("echo 'Hello, World!'", "bash");
        let context = create_test_context();

        let handle = script.execute(context).await.unwrap().unwrap();

        // Wait for execution to complete
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Success => {
                    break;
                }
                ExecutionStatus::Failed(e) => panic!("Script failed: {}", e),
                ExecutionStatus::Cancelled => panic!("Script was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_failed_script_execution() {
        let script = create_test_script("exit 1", "bash");
        let context = create_test_context();

        let handle = script.execute(context).await.unwrap().unwrap();

        // Wait for execution to complete
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(msg) => {
                    assert!(msg.contains("Script exited with code 1"));
                    break;
                }
                ExecutionStatus::Success => panic!("Script should have failed"),
                ExecutionStatus::Cancelled => panic!("Script was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_variable_substitution() {
        let vars = vec![("TEST_VAR", "test_value"), ("ANOTHER_VAR", "another_value")];
        let context = create_test_context_with_vars(vars);

        let script = create_test_script(
            "echo '{{ var.TEST_VAR }} and {{ var.ANOTHER_VAR }}'",
            "bash",
        );

        let handle = script.execute(context).await.unwrap().unwrap();

        // Wait for completion
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Success => {
                    break;
                }
                ExecutionStatus::Failed(e) => panic!("Script failed: {}", e),
                ExecutionStatus::Cancelled => panic!("Script was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_script_cancellation() {
        let script = create_test_script("sleep 10", "bash");
        let context = create_test_context();

        let handle = script.execute(context).await.unwrap().unwrap();

        // Cancel after a short delay
        tokio::time::sleep(Duration::from_millis(100)).await;
        handle.cancellation_token.cancel();

        // Wait for cancellation to complete
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(e) if e.contains("cancelled") => break,
                ExecutionStatus::Success => panic!("Script should have been cancelled"),
                ExecutionStatus::Cancelled => break,
                ExecutionStatus::Running => continue,
                ExecutionStatus::Failed(_) => break, // May fail due to cancellation
            }
        }
    }

    #[tokio::test]
    async fn test_multiline_script() {
        let multiline_script = "echo \"Line 1\"\necho \"Line 2\"\necho \"Line 3\"";
        let script = create_test_script(multiline_script, "bash");
        let context = create_test_context();

        let handle = script.execute(context).await.unwrap().unwrap();

        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Success => {
                    break;
                }
                ExecutionStatus::Failed(e) => panic!("Script failed: {}", e),
                ExecutionStatus::Cancelled => panic!("Script was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }
    }

    #[tokio::test]
    async fn test_atuin_output_vars_env_is_set() {
        // Verify that the ATUIN_OUTPUT_VARS environment variable is set and points to a writable file
        let script_code = r#"
if [ -z "$ATUIN_OUTPUT_VARS" ]; then
    echo "ATUIN_OUTPUT_VARS not set" >&2
    exit 1
fi

# Verify we can write to the file
echo "TEST_VAR=test_value" >> "$ATUIN_OUTPUT_VARS"

# Output the path so we can see it worked
echo "Successfully wrote to $ATUIN_OUTPUT_VARS"
"#;
        let script = create_test_script(script_code, "bash");
        let context = create_test_context();

        let handle = script.execute(context).await.unwrap().unwrap();

        // Wait for execution to complete
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Success => break,
                ExecutionStatus::Failed(e) => panic!("Script failed: {}", e),
                ExecutionStatus::Cancelled => panic!("Script was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }

        // If we got here, the script succeeded, which means:
        // 1. ATUIN_OUTPUT_VARS was set
        // 2. The file was writable
        // 3. The fs_var integration is working
    }

    #[tokio::test]
    async fn test_ssh_host_parsing() {
        assert_eq!(
            Script::parse_ssh_host("user@host.com"),
            (Some("user".to_string()), "host.com".to_string())
        );

        assert_eq!(
            Script::parse_ssh_host("host.com"),
            (None, "host.com".to_string())
        );

        assert_eq!(
            Script::parse_ssh_host("user@host.com:22"),
            (Some("user".to_string()), "host.com:22".to_string())
        );

        assert_eq!(
            Script::parse_ssh_host("host.com:2222"),
            (None, "host.com:2222".to_string())
        );
    }

    #[tokio::test]
    async fn test_interpreter_flag_logic() {
        assert_eq!(Script::get_interpreter_flag("ruby"), Some("-e"));
        assert_eq!(Script::get_interpreter_flag("node"), Some("-e"));
        assert_eq!(Script::get_interpreter_flag("php"), Some("-r"));
        assert_eq!(Script::get_interpreter_flag("bash"), Some("-c"));
        assert_eq!(Script::get_interpreter_flag("/usr/bin/ruby"), Some("-e"));
        assert_eq!(
            Script::get_interpreter_flag("/usr/local/bin/python3"),
            Some("-c")
        );
        assert_eq!(Script::get_interpreter_flag("python3.10"), Some("-c"));
        assert_eq!(Script::get_interpreter_flag("awk"), None);
        assert_eq!(Script::get_interpreter_flag("my-custom-tool"), None);
    }

    #[tokio::test]
    async fn test_grand_central_events_successful_script() {
        let event_bus = Arc::new(MemoryEventBus::new());
        let script = create_test_script("echo 'test'", "bash");
        let script_id = script.id;
        let context = create_test_context_with_event_bus(script.id, event_bus.clone());
        let runbook_id = context.runbook_id;

        let handle = script.execute(context).await.unwrap().unwrap();

        // Wait for execution to complete
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Success => break,
                ExecutionStatus::Failed(e) => panic!("Script failed: {}", e),
                ExecutionStatus::Cancelled => panic!("Script was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }

        // Verify events were emitted
        use crate::events::GCEvent;
        let events = event_bus.events();
        assert_eq!(events.len(), 2);

        // Check BlockStarted event
        match &events[0] {
            GCEvent::BlockStarted {
                block_id,
                runbook_id: rb_id,
            } => {
                assert_eq!(*block_id, script_id);
                assert_eq!(*rb_id, runbook_id);
            }
            _ => panic!("Expected BlockStarted event, got: {:?}", events[0]),
        }

        // Check BlockFinished event
        match &events[1] {
            GCEvent::BlockFinished {
                block_id,
                runbook_id: rb_id,
                success,
            } => {
                assert_eq!(*block_id, script_id);
                assert_eq!(*rb_id, runbook_id);
                assert_eq!(*success, true);
            }
            _ => panic!("Expected BlockFinished event, got: {:?}", events[1]),
        }
    }

    #[tokio::test]
    async fn test_grand_central_events_failed_script() {
        let event_bus = Arc::new(MemoryEventBus::new());
        let script = create_test_script("exit 1", "bash");
        let script_id = script.id;
        let context = create_test_context_with_event_bus(script_id, event_bus.clone());
        let runbook_id = context.runbook_id;

        let handle = script.execute(context).await.unwrap().unwrap();

        // Wait for execution to complete
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break,
                ExecutionStatus::Success => panic!("Script should have failed"),
                ExecutionStatus::Cancelled => panic!("Script was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }

        // Verify events were emitted
        use crate::events::GCEvent;
        let events = event_bus.events();
        assert_eq!(events.len(), 2);

        // Check BlockStarted event
        match &events[0] {
            GCEvent::BlockStarted {
                block_id,
                runbook_id: rb_id,
            } => {
                assert_eq!(*block_id, script_id);
                assert_eq!(*rb_id, runbook_id);
            }
            _ => panic!("Expected BlockStarted event, got: {:?}", events[0]),
        }

        // Check BlockFailed event
        match &events[1] {
            GCEvent::BlockFailed {
                block_id,
                runbook_id: rb_id,
                error,
            } => {
                println!("BlockFailed event: {:?}", error);
                assert_eq!(*block_id, script_id);
                assert_eq!(*rb_id, runbook_id);
                assert!(error.contains("Script exited with code 1"));
            }
            _ => panic!("Expected BlockFailed event, got: {:?}", events[1]),
        }
    }
}
