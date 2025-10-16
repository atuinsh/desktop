use async_trait::async_trait;
use std::process::Stdio;
use std::sync::Arc;
use tauri::ipc::Channel;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::runtime::blocks::handler::{
    BlockErrorData, BlockFinishedData, BlockHandler, BlockLifecycleEvent, BlockOutput,
    CancellationToken, ExecutionContext, ExecutionHandle, ExecutionStatus,
};
use crate::runtime::blocks::script::Script;
use crate::runtime::events::GCEvent;
use crate::runtime::workflow::event::WorkflowEvent;

use crate::templates::template_with_context;
use tokio::sync::{mpsc, oneshot};

pub struct ScriptHandler;

#[async_trait]
impl BlockHandler for ScriptHandler {
    type Block = Script;

    fn block_type(&self) -> &'static str {
        "script"
    }

    fn output_variable(&self, block: &Self::Block) -> Option<String> {
        block.output_variable.clone()
    }

    async fn execute(
        &self,
        script: Script,
        context: ExecutionContext,
        event_sender: broadcast::Sender<WorkflowEvent>,
        output_channel: Option<Channel<BlockOutput>>,
    ) -> Result<ExecutionHandle, Box<dyn std::error::Error + Send + Sync>> {
        let handle = ExecutionHandle {
            id: Uuid::new_v4(),
            block_id: script.id,
            cancellation_token: CancellationToken::new(),
            status: Arc::new(RwLock::new(ExecutionStatus::Running)),
            output_variable: script.output_variable.clone(),
        };

        let script_clone = script.clone();
        let context_clone = context.clone();
        let handle_clone = handle.clone();
        let event_sender_clone = event_sender.clone();

        let output_channel_clone = output_channel.clone();
        let runbook_id = context.runbook_id.to_string();
        let output_storage = context.output_storage.clone();

        tokio::spawn(async move {
            // Emit BlockStarted event via Grand Central
            if let Some(event_bus) = &context_clone.event_bus {
                let _ = event_bus
                    .emit(GCEvent::BlockStarted {
                        block_id: script_clone.id,
                        runbook_id: context_clone.runbook_id,
                    })
                    .await;
            }

            let (exit_code, captured_output) = Self::run_script(
                &script_clone,
                context_clone.clone(),
                handle_clone.cancellation_token.clone(),
                event_sender_clone,
                output_channel_clone,
            )
            .await;

            // Determine status based on exit code
            let status = match exit_code {
                Ok(0) => {
                    let output = captured_output.trim().to_string();

                    // Store output variable if successful and we have an output variable
                    if let (Some(var_name), Some(storage)) =
                        (&handle_clone.output_variable, &output_storage)
                    {
                        storage
                            .write()
                            .await
                            .entry(runbook_id.clone())
                            .or_insert_with(std::collections::HashMap::new)
                            .insert(var_name.clone(), output.clone());
                    }

                    // Emit BlockFinished event via Grand Central
                    if let Some(event_bus) = &context_clone.event_bus {
                        let _ = event_bus
                            .emit(GCEvent::BlockFinished {
                                block_id: script_clone.id,
                                runbook_id: context_clone.runbook_id,
                                success: true,
                            })
                            .await;
                    }

                    ExecutionStatus::Success(output)
                }
                Ok(code) => {
                    // Emit BlockFailed event via Grand Central
                    if let Some(event_bus) = &context_clone.event_bus {
                        let _ = event_bus
                            .emit(GCEvent::BlockFailed {
                                block_id: script_clone.id,
                                runbook_id: context_clone.runbook_id,
                                error: format!("Process exited with code {}", code),
                            })
                            .await;
                    }

                    ExecutionStatus::Failed(format!("Process exited with code {}", code))
                }
                Err(e) => {
                    // Emit BlockFailed event via Grand Central
                    if let Some(event_bus) = &context_clone.event_bus {
                        let _ = event_bus
                            .emit(GCEvent::BlockFailed {
                                block_id: script_clone.id,
                                runbook_id: context_clone.runbook_id,
                                error: e.to_string(),
                            })
                            .await;
                    }

                    ExecutionStatus::Failed(e.to_string())
                }
            };

            *handle_clone.status.write().await = status.clone();
        });

        Ok(handle)
    }
}

impl ScriptHandler {
    /// Parse SSH host string to extract username and hostname
    /// Supports formats: "user@host", "host", "user@host:port"
    fn parse_ssh_host(ssh_host: &str) -> (Option<String>, String) {
        if let Some(at_pos) = ssh_host.find('@') {
            let username = ssh_host[..at_pos].to_string();
            let host_part = &ssh_host[at_pos + 1..];
            // Remove port if present (we don't use it for the SSH pool)
            let hostname = if let Some(colon_pos) = host_part.find(':') {
                host_part[..colon_pos].to_string()
            } else {
                host_part.to_string()
            };
            (Some(username), hostname)
        } else {
            // No username specified, just hostname (possibly with port)
            let hostname = if let Some(colon_pos) = ssh_host.find(':') {
                ssh_host[..colon_pos].to_string()
            } else {
                ssh_host.to_string()
            };
            (None, hostname)
        }
    }

    /// Template script code using the Minijinja template system
    async fn template_script_code(
        code: &str,
        context: &ExecutionContext,
        script_id: Uuid,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let block_id_str = script_id.to_string();
        let rendered = template_with_context(
            code,
            &context.variables,
            &context.document,
            Some(&block_id_str),
            None,
        )?;
        Ok(rendered)
    }

    /// Execute script via SSH using the SSH pool
    async fn execute_ssh_script(
        script: &Script,
        code: &str,
        ssh_host: &str,
        context: ExecutionContext,
        cancellation_token: CancellationToken,
        event_sender: broadcast::Sender<WorkflowEvent>,
        output_channel: Option<Channel<BlockOutput>>,
    ) -> (
        Result<i32, Box<dyn std::error::Error + Send + Sync>>,
        String,
    ) {
        // Send start event
        let _ = event_sender.send(WorkflowEvent::BlockStarted { id: script.id });

        // Send started lifecycle event to output channel
        if let Some(ref ch) = output_channel {
            let _ = ch.send(BlockOutput {
                stdout: None,
                stderr: None,
                binary: None,
                object: None,
                lifecycle: Some(BlockLifecycleEvent::Started),
            });
        }

        // Parse SSH host to extract username and hostname
        let (username, hostname) = Self::parse_ssh_host(ssh_host);

        // Get SSH pool from context
        let ssh_pool = match context.ssh_pool {
            Some(pool) => pool,
            None => {
                let error_msg = "SSH pool not available in execution context";
                let _ = event_sender.send(WorkflowEvent::BlockFinished { id: script.id });
                if let Some(ref ch) = output_channel {
                    let _ = ch.send(BlockOutput {
                        stdout: None,
                        stderr: None,
                        binary: None,
                        object: None,
                        lifecycle: Some(BlockLifecycleEvent::Error(BlockErrorData {
                            message: error_msg.to_string(),
                        })),
                    });
                }
                return (Err(error_msg.into()), String::new());
            }
        };

        // Create unique channel ID for this execution
        let channel_id = script.id.to_string();

        // Create channels for SSH communication
        let (output_sender, mut output_receiver) = mpsc::channel::<String>(100);
        let (result_tx, result_rx) = oneshot::channel::<()>();

        // Capture output for return value
        let captured_output = Arc::new(RwLock::new(String::new()));
        let captured_output_clone = captured_output.clone();

        // Start SSH execution
        let exec_result = ssh_pool
            .exec(
                &hostname,
                username.as_deref(),
                &script.interpreter,
                code,
                &channel_id,
                output_sender,
                result_tx,
            )
            .await;

        if let Err(e) = exec_result {
            let error_msg = format!("Failed to start SSH execution: {}", e);
            let _ = event_sender.send(WorkflowEvent::BlockFinished { id: script.id });
            if let Some(ref ch) = output_channel {
                let _ = ch.send(BlockOutput {
                    stdout: None,
                    stderr: None,
                    binary: None,
                    object: None,
                    lifecycle: Some(BlockLifecycleEvent::Error(BlockErrorData {
                        message: error_msg.clone(),
                    })),
                });
            }
            return (Err(error_msg.into()), String::new());
        }

        // Handle output streaming and cancellation
        let cancellation_receiver = cancellation_token.take_receiver();
        let output_channel_clone = output_channel.clone();
        let script_id = script.id;
        let ssh_pool_clone = ssh_pool.clone();
        let channel_id_clone = channel_id.clone();

        // Spawn task to handle output streaming
        tokio::spawn(async move {
            while let Some(line) = output_receiver.recv().await {
                // Send to output channel
                if let Some(ref ch) = output_channel_clone {
                    let _ = ch.send(BlockOutput {
                        stdout: Some(line.clone()),
                        stderr: None,
                        lifecycle: None,
                        binary: None,
                        object: None,
                    });
                }

                // Capture for return value
                let mut captured = captured_output_clone.write().await;
                captured.push_str(&line);
            }
        });

        // Wait for completion or cancellation
        let exit_code = if let Some(cancel_rx) = cancellation_receiver {
            tokio::select! {
                _ = cancel_rx => {
                    // Cancel SSH execution
                    let _ = ssh_pool_clone.exec_cancel(&channel_id_clone).await;
                    let captured = captured_output.read().await.clone();

                    // Emit BlockCancelled event via Grand Central
                    if let Some(event_bus) = &context.event_bus {
                        let _ = event_bus.emit(GCEvent::BlockCancelled {
                            block_id: script_id,
                            runbook_id: context.runbook_id,
                        }).await;
                    }

                    // Send completion events
                    let _ = event_sender.send(WorkflowEvent::BlockFinished { id: script_id });
                    if let Some(ref ch) = output_channel {
                        let _ = ch.send(BlockOutput {
                            stdout: None,
                            stderr: None,
                            binary: None,
                object: None,
                lifecycle: Some(BlockLifecycleEvent::Cancelled),
                        });
                    }
                    return (Err("SSH script execution cancelled".into()), captured);
                }
                _ = result_rx => {
                    // SSH execution completed
                    0 // SSH pool doesn't provide exit codes yet, assume success
                }
            }
        } else {
            // No cancellation receiver, just wait for completion
            let _ = result_rx.await;
            0 // SSH pool doesn't provide exit codes yet, assume success
        };

        // Send completion events
        let _ = event_sender.send(WorkflowEvent::BlockFinished { id: script.id });
        if let Some(ref ch) = output_channel {
            let _ = ch.send(BlockOutput {
                stdout: None,
                stderr: None,
                binary: None,
                object: None,
                lifecycle: Some(BlockLifecycleEvent::Finished(BlockFinishedData {
                    exit_code: Some(exit_code),
                    success: exit_code == 0,
                })),
            });
        }

        // Return result
        let captured = captured_output.read().await.clone();
        (Ok(exit_code), captured)
    }

    async fn run_script(
        script: &Script,
        context: ExecutionContext,
        cancellation_token: CancellationToken,
        event_sender: broadcast::Sender<WorkflowEvent>,
        output_channel: Option<Channel<BlockOutput>>,
    ) -> (
        Result<i32, Box<dyn std::error::Error + Send + Sync>>,
        String,
    ) {
        // Send start event
        let _ = event_sender.send(WorkflowEvent::BlockStarted { id: script.id });

        // Send started lifecycle event to output channel
        if let Some(ref ch) = output_channel {
            let _ = ch.send(BlockOutput {
                stdout: None,
                stderr: None,
                binary: None,
                object: None,
                lifecycle: Some(BlockLifecycleEvent::Started),
            });
        }

        // Template the script code using Minijinja
        let code = Self::template_script_code(&script.code, &context, script.id)
            .await
            .unwrap_or_else(|e| {
                eprintln!("Template error in script {}: {}", script.id, e);
                script.code.clone() // Fallback to original code
            });

        // Execute either via SSH pool or locally
        if let Some(ssh_host) = context.ssh_host.clone() {
            // SSH execution using the SSH pool
            return Self::execute_ssh_script(
                script,
                &code,
                &ssh_host,
                context,
                cancellation_token,
                event_sender,
                output_channel,
            )
            .await;
        }

        // Local execution
        let mut cmd = Command::new(&script.interpreter);
        cmd.arg("-c");
        cmd.arg(&code);
        cmd.current_dir(&context.cwd);
        cmd.envs(&context.env);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.stdin(Stdio::null());

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                // Send completion event on spawn error
                let _ = event_sender.send(WorkflowEvent::BlockFinished { id: script.id });
                // Send error lifecycle event
                if let Some(ref ch) = output_channel {
                    let _ = ch.send(BlockOutput {
                        stdout: None,
                        stderr: None,
                        binary: None,
                        object: None,
                        lifecycle: Some(BlockLifecycleEvent::Error(BlockErrorData {
                            message: format!("Failed to spawn process: {}", e),
                        })),
                    });
                }
                return (Err(e.into()), String::new());
            }
        };
        let pid = child.id();

        // Capture stdout
        let captured_output = Arc::new(RwLock::new(String::new()));

        if let Some(stdout) = child.stdout.take() {
            let channel = output_channel.clone();
            let capture = captured_output.clone();

            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();
                while let Ok(n) = reader.read_line(&mut line).await {
                    if n == 0 {
                        break;
                    }
                    if let Some(ref ch) = channel {
                        let _ = ch.send(BlockOutput {
                            stdout: Some(line.clone()),
                            stderr: None,
                            lifecycle: None,
                            binary: None,
                            object: None,
                        });
                    }
                    // Capture output
                    let mut captured = capture.write().await;
                    captured.push_str(&line);
                    line.clear();
                }
            });
        }

        // Stream stderr
        if let Some(stderr) = child.stderr.take() {
            let channel = output_channel.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                while let Ok(n) = reader.read_line(&mut line).await {
                    if n == 0 {
                        break;
                    }
                    if let Some(ref ch) = channel {
                        let _ = ch.send(BlockOutput {
                            stdout: None,
                            stderr: Some(line.clone()),
                            lifecycle: None,
                            binary: None,
                            object: None,
                        });
                    }
                    line.clear();
                }
            });
        }

        // Wait for completion or cancellation
        let cancellation_receiver = cancellation_token.take_receiver();
        let exit_code = if let Some(cancel_rx) = cancellation_receiver {
            tokio::select! {
                _ = cancel_rx => {
                    // Kill the process
                    if let Some(pid) = pid {
                        #[cfg(unix)]
                        {
                            use nix::sys::signal::{self, Signal};
                            use nix::unistd::Pid;
                            let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
                        }
                        #[cfg(windows)]
                        {
                            let _ = child.kill().await;
                        }
                    }
                    let captured = captured_output.read().await.clone();

                    // Emit BlockCancelled event via Grand Central
                    if let Some(event_bus) = &context.event_bus {
                        let _ = event_bus.emit(GCEvent::BlockCancelled {
                            block_id: script.id,
                            runbook_id: context.runbook_id,
                        }).await;
                    }

                    // Send completion event on cancellation
                    let _ = event_sender.send(WorkflowEvent::BlockFinished { id: script.id });
                    // Send cancelled lifecycle event
                    if let Some(ref ch) = output_channel {
                        let _ = ch.send(BlockOutput {
                            stdout: None,
                            stderr: None,
                            binary: None,
                object: None,
                lifecycle: Some(BlockLifecycleEvent::Cancelled),
                        });
                    }
                    return (Err("Script execution cancelled".into()), captured);
                }
                result = child.wait() => {
                    match result {
                        Ok(status) => status.code().unwrap_or(-1),
                        Err(e) => {
                            let captured = captured_output.read().await.clone();
                            // Send completion event on process wait error
                            let _ = event_sender.send(WorkflowEvent::BlockFinished { id: script.id });
                            // Send error lifecycle event
                            if let Some(ref ch) = output_channel {
                                let _ = ch.send(BlockOutput {
                                    stdout: None,
                                    stderr: None,
                                    binary: None,
                object: None,
                lifecycle: Some(BlockLifecycleEvent::Error(BlockErrorData {
                                        message: format!("Failed to wait for process: {e}")
                                    })),
                                });
                            }
                            return (Err(format!("Failed to wait for process: {e}").into()), captured);
                        }
                    }
                }
            }
        } else {
            // No cancellation receiver available, just wait for completion
            match child.wait().await {
                Ok(status) => status.code().unwrap_or(-1),
                Err(e) => {
                    let captured = captured_output.read().await.clone();
                    // Send completion event on process wait error
                    let _ = event_sender.send(WorkflowEvent::BlockFinished { id: script.id });
                    // Send error lifecycle event
                    if let Some(ref ch) = output_channel {
                        let _ = ch.send(BlockOutput {
                            stdout: None,
                            stderr: None,
                            binary: None,
                            object: None,
                            lifecycle: Some(BlockLifecycleEvent::Error(BlockErrorData {
                                message: format!("Failed to wait for process: {}", e),
                            })),
                        });
                    }
                    return (
                        Err(format!("Failed to wait for process: {}", e).into()),
                        captured,
                    );
                }
            }
        };

        // Send completion event
        let _ = event_sender.send(WorkflowEvent::BlockFinished { id: script.id });

        // Send finished lifecycle event
        if let Some(ref ch) = output_channel {
            let _ = ch.send(BlockOutput {
                stdout: None,
                stderr: None,
                binary: None,
                object: None,
                lifecycle: Some(BlockLifecycleEvent::Finished(BlockFinishedData {
                    exit_code: Some(exit_code),
                    success: exit_code == 0,
                })),
            });
        }

        // Return exit code and captured output
        let captured = captured_output.read().await.clone();
        (Ok(exit_code), captured)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::blocks::script::Script;
    use crate::runtime::events::MemoryEventBus;
    use std::collections::HashMap;
    use tokio::time::{timeout, Duration};

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
        ExecutionContext {
            runbook_id: Uuid::new_v4(),
            cwd: std::env::temp_dir().to_string_lossy().to_string(),
            env: HashMap::new(),
            variables: HashMap::new(),
            ssh_host: None,
            document: Vec::new(),
            ssh_pool: None, // Tests don't need SSH pool unless specifically testing SSH
            output_storage: None, // Tests can add this when needed
            pty_store: None, // Tests don't need PTY store
            event_bus: None,
        }
    }

    fn create_test_context_with_event_bus(event_bus: Arc<MemoryEventBus>) -> ExecutionContext {
        ExecutionContext {
            runbook_id: Uuid::new_v4(),
            cwd: std::env::temp_dir().to_string_lossy().to_string(),
            env: HashMap::new(),
            variables: HashMap::new(),
            ssh_host: None,
            document: Vec::new(),
            ssh_pool: None,
            output_storage: None,
            pty_store: None,
            event_bus: Some(event_bus),
        }
    }

    #[test]
    fn test_handler_block_type() {
        let handler = ScriptHandler;
        assert_eq!(handler.block_type(), "script");
    }

    #[test]
    fn test_output_variable_extraction() {
        let handler = ScriptHandler;

        let script_with_output = Script::builder()
            .id(Uuid::new_v4())
            .name("Test")
            .code("echo test")
            .interpreter("bash")
            .output_variable(Some("result".to_string()))
            .build();

        let script_without_output = Script::builder()
            .id(Uuid::new_v4())
            .name("Test")
            .code("echo test")
            .interpreter("bash")
            .output_variable(None)
            .build();

        assert_eq!(
            handler.output_variable(&script_with_output),
            Some("result".to_string())
        );
        assert_eq!(handler.output_variable(&script_without_output), None);
    }

    #[tokio::test]
    async fn test_successful_script_execution() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Test simple echo command
        let (exit_code, output) = ScriptHandler::run_script(
            &create_test_script("echo 'Hello, World!'", "bash"),
            create_test_context(),
            CancellationToken::new(),
            _tx,
            None,
        )
        .await;

        assert!(exit_code.is_ok());
        assert_eq!(exit_code.unwrap(), 0);
        assert!(output.contains("Hello, World!"));
    }

    #[tokio::test]
    async fn test_failed_script_execution() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Test command that should fail
        let (exit_code, _output) = ScriptHandler::run_script(
            &create_test_script("exit 1", "bash"),
            create_test_context(),
            CancellationToken::new(),
            _tx,
            None,
        )
        .await;

        assert!(exit_code.is_ok());
        assert_eq!(exit_code.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_command_not_found() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Test non-existent command
        let (exit_code, _output) = ScriptHandler::run_script(
            &create_test_script("nonexistent_command_12345", "bash"),
            create_test_context(),
            CancellationToken::new(),
            _tx,
            None,
        )
        .await;

        // Should fail with non-zero exit code
        assert!(exit_code.is_ok());
        assert_ne!(exit_code.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_variable_substitution() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let mut context = create_test_context();
        context
            .variables
            .insert("TEST_VAR".to_string(), "test_value".to_string());
        context
            .variables
            .insert("ANOTHER_VAR".to_string(), "another_value".to_string());

        // Test Minijinja template syntax
        let (exit_code, output) = ScriptHandler::run_script(
            &create_test_script(
                "echo '{{ var.TEST_VAR }} and {{ var.ANOTHER_VAR }}'",
                "bash",
            ),
            context,
            CancellationToken::new(),
            _tx,
            None,
        )
        .await;

        assert!(exit_code.is_ok());
        assert_eq!(exit_code.unwrap(), 0);
        assert!(output.contains("test_value"));
        assert!(output.contains("another_value"));
    }

    #[tokio::test]
    async fn test_variable_substitution_missing_vars() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // No variables in context, should render as empty or error gracefully
        let (exit_code, output) = ScriptHandler::run_script(
            &create_test_script(
                "echo '{{ var.MISSING_VAR | default(\"default_value\") }}'",
                "bash",
            ),
            create_test_context(),
            CancellationToken::new(),
            _tx,
            None,
        )
        .await;

        assert!(exit_code.is_ok());
        assert_eq!(exit_code.unwrap(), 0);
        assert!(output.contains("default_value"));
    }

    #[tokio::test]
    async fn test_environment_variables() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let mut context = create_test_context();
        context
            .env
            .insert("TEST_ENV_VAR".to_string(), "env_value".to_string());

        let (exit_code, output) = ScriptHandler::run_script(
            &create_test_script("echo $TEST_ENV_VAR", "bash"),
            context,
            CancellationToken::new(),
            _tx,
            None,
        )
        .await;

        assert!(exit_code.is_ok());
        assert_eq!(exit_code.unwrap(), 0);
        assert!(output.contains("env_value"));
    }

    #[tokio::test]
    async fn test_working_directory() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let mut context = create_test_context();
        context.cwd = "/".to_string();

        let (exit_code, output) = ScriptHandler::run_script(
            &create_test_script("pwd", "sh"),
            context,
            CancellationToken::new(),
            _tx,
            None,
        )
        .await;

        assert!(exit_code.is_ok());
        assert_eq!(exit_code.unwrap(), 0);
        assert!(output.trim() == "/", "got output: {output:?}");
    }

    #[tokio::test]
    async fn test_different_interpreters() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);
        let context = create_test_context();

        // Test bash
        let (exit_code, _output) = ScriptHandler::run_script(
            &create_test_script("echo 'bash test'", "bash"),
            context.clone(),
            CancellationToken::new(),
            _tx.clone(),
            None,
        )
        .await;
        assert!(exit_code.is_ok());
        assert_eq!(exit_code.unwrap(), 0);

        // Test sh
        let (exit_code, _output) = ScriptHandler::run_script(
            &create_test_script("echo 'sh test'", "sh"),
            context.clone(),
            CancellationToken::new(),
            _tx.clone(),
            None,
        )
        .await;
        assert!(exit_code.is_ok());
        assert_eq!(exit_code.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_multiline_script() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let multiline_script = "echo \"Line 1\"\necho \"Line 2\"\necho \"Line 3\"";

        let (exit_code, output) = ScriptHandler::run_script(
            &create_test_script(multiline_script, "bash"),
            create_test_context(),
            CancellationToken::new(),
            _tx,
            None,
        )
        .await;

        assert!(exit_code.is_ok());
        assert_eq!(exit_code.unwrap(), 0);
        assert!(output.contains("Line 1"));
        assert!(output.contains("Line 2"));
        assert!(output.contains("Line 3"));
    }

    #[tokio::test]
    async fn test_script_with_stderr() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Script that writes to both stdout and stderr
        let (exit_code, _output) = ScriptHandler::run_script(
            &create_test_script("echo 'stdout'; echo 'stderr' >&2", "bash"),
            create_test_context(),
            CancellationToken::new(),
            _tx,
            None,
        )
        .await;

        assert!(exit_code.is_ok());
        assert_eq!(exit_code.unwrap(), 0);
        // Note: stderr is handled separately in the actual implementation
    }

    #[tokio::test]
    async fn test_cancellation_token_creation() {
        let token = CancellationToken::new();

        // Should be able to take receiver once
        let receiver = token.take_receiver();
        assert!(receiver.is_some());

        // Second attempt should return None
        let receiver2 = token.take_receiver();
        assert!(receiver2.is_none());

        // Cancel should not panic
        token.cancel();
    }

    #[tokio::test]
    async fn test_script_cancellation() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);
        let token = CancellationToken::new();

        // Start a long-running script
        let script = create_test_script("sleep 10", "bash");
        let script_future =
            ScriptHandler::run_script(&script, create_test_context(), token.clone(), _tx, None);

        // Cancel after a short delay
        let cancel_future = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            token.cancel();
        };

        // Run both futures concurrently
        let (result, _) = tokio::join!(script_future, cancel_future);

        // Should be cancelled (error result)
        assert!(result.0.is_err());
        assert!(result.0.unwrap_err().to_string().contains("cancelled"));
    }

    #[tokio::test]
    async fn test_large_output() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Generate a large amount of output
        let (exit_code, output) = ScriptHandler::run_script(
            &create_test_script("for i in {1..100}; do echo \"Line $i\"; done", "bash"),
            create_test_context(),
            CancellationToken::new(),
            _tx,
            None,
        )
        .await;

        assert!(exit_code.is_ok());
        assert_eq!(exit_code.unwrap(), 0);
        assert!(output.contains("Line 1"));
        assert!(output.contains("Line 100"));
        // Should have captured all 100 lines
        assert_eq!(output.lines().count(), 100);
    }

    #[tokio::test]
    async fn test_script_timeout_handling() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Test that we can timeout a script execution
        let script = create_test_script("sleep 5", "bash");
        let script_future = ScriptHandler::run_script(
            &script,
            create_test_context(),
            CancellationToken::new(),
            _tx,
            None,
        );

        // Timeout after 1 second
        let result = timeout(Duration::from_millis(100), script_future).await;

        // Should timeout
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_template_with_document_context() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Create a context with document and variables
        let mut context = create_test_context();
        context
            .variables
            .insert("TEST_VAR".to_string(), "from_variable".to_string());

        // Add a simple document with named blocks
        context.document = vec![
            serde_json::json!({
                "id": "block1",
                "type": "paragraph",
                "props": { "name": "first_block" },
                "content": [{"type": "text", "text": "First block content"}]
            }),
            serde_json::json!({
                "id": "block2",
                "type": "script",
                "props": { "code": "echo test" },
                "content": []
            }),
        ];

        // Test template with both variable and document context
        let (exit_code, output) = ScriptHandler::run_script(
            &create_test_script("echo 'Variable: {{ var.TEST_VAR }}, First block: {{ doc.named.first_block.content }}'", "bash"),
            context,
            CancellationToken::new(),
            _tx,
            None,
        )
        .await;

        assert!(exit_code.is_ok());
        assert_eq!(exit_code.unwrap(), 0);
        assert!(output.contains("from_variable"), "got output: {output:?}");
        assert!(
            output.contains("First block content"),
            "got output: {output:?}"
        );
    }

    #[tokio::test]
    async fn test_special_characters_in_script() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Test script with special characters
        let script_with_special_chars = r#"echo "Special chars: !@#$%^&*()[]{}|;':\",./<>?""#;

        let (exit_code, output) = ScriptHandler::run_script(
            &create_test_script(script_with_special_chars, "bash"),
            create_test_context(),
            CancellationToken::new(),
            _tx,
            None,
        )
        .await;

        assert!(exit_code.is_ok());
        assert_eq!(exit_code.unwrap(), 0);
        assert!(output.contains("Special chars:"));
    }

    #[tokio::test]
    async fn test_empty_script() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let (exit_code, output) = ScriptHandler::run_script(
            &create_test_script("", "bash"),
            create_test_context(),
            CancellationToken::new(),
            _tx,
            None,
        )
        .await;

        assert!(exit_code.is_ok());
        assert_eq!(exit_code.unwrap(), 0);
        assert!(output.is_empty());
    }

    #[tokio::test]
    async fn test_script_with_unicode() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let (exit_code, output) = ScriptHandler::run_script(
            &create_test_script("echo '测试 🚀 émojis'", "bash"),
            create_test_context(),
            CancellationToken::new(),
            _tx,
            None,
        )
        .await;

        assert!(exit_code.is_ok());
        assert_eq!(exit_code.unwrap(), 0);
        assert!(output.contains("测试"), "got output: {output:?}");
        assert!(output.contains("🚀"), "got output {output:?}");
        assert!(output.contains("émojis"), "got output {output:?}");
    }

    // Integration test for SSH execution (would need SSH setup to run)
    #[tokio::test]
    async fn test_ssh_execution_without_pool() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let mut context = create_test_context();
        context.ssh_host = Some("localhost".to_string());
        // ssh_pool is None, so this should fail gracefully

        let (exit_code, output) = ScriptHandler::run_script(
            &create_test_script("echo 'SSH test'", "bash"),
            context,
            CancellationToken::new(),
            _tx,
            None,
        )
        .await;

        // Should fail because SSH pool is not available
        assert!(exit_code.is_err());
        assert!(exit_code
            .unwrap_err()
            .to_string()
            .contains("SSH pool not available"));
        assert!(output.is_empty());
    }

    #[tokio::test]
    async fn test_ssh_host_parsing() {
        // Test various SSH host formats
        assert_eq!(
            ScriptHandler::parse_ssh_host("user@host.com"),
            (Some("user".to_string()), "host.com".to_string())
        );

        assert_eq!(
            ScriptHandler::parse_ssh_host("host.com"),
            (None, "host.com".to_string())
        );

        assert_eq!(
            ScriptHandler::parse_ssh_host("user@host.com:22"),
            (Some("user".to_string()), "host.com".to_string())
        );

        assert_eq!(
            ScriptHandler::parse_ssh_host("host.com:2222"),
            (None, "host.com".to_string())
        );
    }

    #[tokio::test]
    async fn test_script_output_variable_storage() {
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Create output storage
        let output_storage = Arc::new(RwLock::new(
            HashMap::<String, HashMap<String, String>>::new(),
        ));

        // Create context with output storage
        let mut context = create_test_context();
        let runbook_id = context.runbook_id;
        context.output_storage = Some(output_storage.clone());

        // Create script with output variable
        let script = Script::builder()
            .id(Uuid::new_v4())
            .name("Test Script")
            .code("echo 'test output value'")
            .interpreter("bash")
            .output_variable(Some("my_output".to_string()))
            .build();

        // Execute the script using the handler
        let handler = ScriptHandler;
        let handle = handler.execute(script, context, tx, None).await.unwrap();

        // Wait for execution to complete
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Success(output) => {
                    assert_eq!(output.trim(), "test output value");
                    break;
                }
                ExecutionStatus::Failed(e) => panic!("Script failed: {}", e),
                ExecutionStatus::Cancelled => panic!("Script was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }

        // Verify output was stored
        let stored_vars = output_storage.read().await;
        let runbook_vars = stored_vars
            .get(&runbook_id.to_string())
            .expect("Runbook variables should exist");
        assert_eq!(
            runbook_vars
                .get("my_output")
                .expect("Output variable should be stored"),
            "test output value"
        );
    }

    #[tokio::test]
    async fn test_script_without_output_variable() {
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Create output storage
        let output_storage = Arc::new(RwLock::new(
            HashMap::<String, HashMap<String, String>>::new(),
        ));

        // Create context with output storage
        let mut context = create_test_context();
        let runbook_id = context.runbook_id;
        context.output_storage = Some(output_storage.clone());

        // Create script WITHOUT output variable
        let script = Script::builder()
            .id(Uuid::new_v4())
            .name("Test Script")
            .code("echo 'test output'")
            .interpreter("bash")
            .output_variable(None)
            .build();

        // Execute the script using the handler
        let handler = ScriptHandler;
        let handle = handler.execute(script, context, tx, None).await.unwrap();

        // Wait for execution to complete
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Success(_) => break,
                ExecutionStatus::Failed(e) => panic!("Script failed: {}", e),
                ExecutionStatus::Cancelled => panic!("Script was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }

        // Verify nothing was stored
        let stored_vars = output_storage.read().await;
        assert!(stored_vars.get(&runbook_id.to_string()).is_none());
    }

    #[tokio::test]
    async fn test_failed_script_no_output_storage() {
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Create output storage
        let output_storage = Arc::new(RwLock::new(
            HashMap::<String, HashMap<String, String>>::new(),
        ));

        // Create context with output storage
        let mut context = create_test_context();
        let runbook_id = context.runbook_id;
        context.output_storage = Some(output_storage.clone());

        // Create script that will fail
        let script = Script::builder()
            .id(Uuid::new_v4())
            .name("Test Script")
            .code("exit 1")
            .interpreter("bash")
            .output_variable(Some("should_not_store".to_string()))
            .build();

        // Execute the script using the handler
        let handler = ScriptHandler;
        let handle = handler.execute(script, context, tx, None).await.unwrap();

        // Wait for execution to complete
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break,
                ExecutionStatus::Success(_) => panic!("Script should have failed"),
                ExecutionStatus::Cancelled => panic!("Script was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }

        // Verify nothing was stored
        let stored_vars = output_storage.read().await;
        assert!(stored_vars.get(&runbook_id.to_string()).is_none());
    }

    #[tokio::test]
    async fn test_grand_central_events_successful_script() {
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Create memory event bus
        let event_bus = Arc::new(MemoryEventBus::new());
        let context = create_test_context_with_event_bus(event_bus.clone());
        let runbook_id = context.runbook_id;

        // Create and execute script
        let script = create_test_script("echo 'test'", "bash");
        let script_id = script.id;

        let handler = ScriptHandler;
        let handle = handler.execute(script, context, tx, None).await.unwrap();

        // Wait for execution to complete
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Success(_) => break,
                ExecutionStatus::Failed(e) => panic!("Script failed: {}", e),
                ExecutionStatus::Cancelled => panic!("Script was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }

        // Verify events were emitted
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
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Create memory event bus
        let event_bus = Arc::new(MemoryEventBus::new());
        let context = create_test_context_with_event_bus(event_bus.clone());
        let runbook_id = context.runbook_id;

        // Create script that will fail
        let script = create_test_script("exit 1", "bash");
        let script_id = script.id;

        let handler = ScriptHandler;
        let handle = handler.execute(script, context, tx, None).await.unwrap();

        // Wait for execution to complete
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(_) => break,
                ExecutionStatus::Success(_) => panic!("Script should have failed"),
                ExecutionStatus::Cancelled => panic!("Script was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }

        // Verify events were emitted
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
                assert_eq!(*block_id, script_id);
                assert_eq!(*rb_id, runbook_id);
                assert!(error.contains("Process exited with code 1"));
            }
            _ => panic!("Expected BlockFailed event, got: {:?}", events[1]),
        }
    }

    #[tokio::test]
    async fn test_grand_central_events_cancelled_script() {
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Create memory event bus
        let event_bus = Arc::new(MemoryEventBus::new());
        let context = create_test_context_with_event_bus(event_bus.clone());
        let runbook_id = context.runbook_id;

        // Create long-running script
        let script = create_test_script("sleep 10", "bash");
        let script_id = script.id;

        let handler = ScriptHandler;
        let handle = handler.execute(script, context, tx, None).await.unwrap();

        // Cancel after a short delay
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        handle.cancellation_token.cancel();

        // Wait for cancellation to complete
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle.status.read().await.clone();
            match status {
                ExecutionStatus::Failed(e) if e.contains("cancelled") => break,
                ExecutionStatus::Success(_) => panic!("Script should have been cancelled"),
                ExecutionStatus::Cancelled => break,
                ExecutionStatus::Running => continue,
                ExecutionStatus::Failed(_e) => break, // May fail due to cancellation
            }
        }

        // Verify events were emitted
        let events = event_bus.events();
        assert!(
            events.len() >= 2,
            "Expected at least 2 events, got: {}",
            events.len()
        );

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

        // Check final event (could be BlockCancelled or BlockFinished depending on timing)
        let last_event = events.last().unwrap();
        match last_event {
            GCEvent::BlockCancelled {
                block_id,
                runbook_id: rb_id,
            } => {
                assert_eq!(*block_id, script_id);
                assert_eq!(*rb_id, runbook_id);
            }
            GCEvent::BlockFinished {
                block_id,
                runbook_id: rb_id,
                success: _,
            } => {
                // Script may finish before cancellation takes effect
                assert_eq!(*block_id, script_id);
                assert_eq!(*rb_id, runbook_id);
            }
            GCEvent::BlockFailed {
                block_id,
                runbook_id: rb_id,
                error: _,
            } => {
                // Script may fail due to cancellation
                assert_eq!(*block_id, script_id);
                assert_eq!(*rb_id, runbook_id);
            }
            _ => panic!(
                "Expected BlockCancelled, BlockFinished, or BlockFailed event, got: {:?}",
                last_event
            ),
        }
    }
}
