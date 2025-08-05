use async_trait::async_trait;
use std::process::Stdio;
use std::sync::Arc;
use tauri::{ipc::Channel, AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::runtime::blocks::handler::{
    BlockHandler, BlockLifecycleEvent, BlockOutput, CancellationToken, ExecutionContext,
    ExecutionHandle, ExecutionStatus,
};
use crate::runtime::blocks::script::Script;
use crate::runtime::workflow::event::WorkflowEvent;
use crate::state::AtuinState;

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
        app_handle: AppHandle,
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
        let app_handle_clone = app_handle.clone();
        let runbook_id = context.runbook_id.to_string();

        tokio::spawn(async move {
            let (exit_code, captured_output) = Self::run_script(
                &script_clone,
                context_clone,
                handle_clone.cancellation_token.clone(),
                event_sender_clone,
                output_channel_clone,
            )
            .await;

            // Determine status based on exit code
            let status = match exit_code {
                Ok(0) => ExecutionStatus::Success(captured_output.trim().to_string()),
                Ok(code) => ExecutionStatus::Failed(format!("Process exited with code {}", code)),
                Err(e) => ExecutionStatus::Failed(e.to_string()),
            };

            *handle_clone.status.write().await = status.clone();

            // Store output variable if successful
            if let (ExecutionStatus::Success(output), Some(var_name)) =
                (&status, &handle_clone.output_variable)
            {
                if !output.is_empty() {
                    if let Some(state) = app_handle_clone.try_state::<AtuinState>() {
                        state
                            .runbook_output_variables
                            .write()
                            .await
                            .entry(runbook_id)
                            .or_insert_with(std::collections::HashMap::new)
                            .insert(var_name.clone(), output.clone());
                    }
                }
            }
        });

        Ok(handle)
    }
}

impl ScriptHandler {
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
                lifecycle: Some(BlockLifecycleEvent::Started),
            });
        }

        // Template the script code with variables
        let code = if context.variables.is_empty() {
            script.code.clone()
        } else {
            // Simple variable substitution for now
            let mut templated = script.code.clone();
            for (key, value) in &context.variables {
                templated = templated.replace(&format!("${{{}}}", key), value);
                templated = templated.replace(&format!("${}", key), value);
            }
            templated
        };

        // Build the command
        let mut cmd = if let Some(ssh_host) = &context.ssh_host {
            // SSH execution
            let mut ssh_cmd = Command::new("ssh");
            ssh_cmd.arg(ssh_host);
            ssh_cmd.arg(format!("{} -c", script.interpreter));
            ssh_cmd.arg(&code);
            ssh_cmd
        } else {
            // Local execution
            let mut local_cmd = Command::new(&script.interpreter);
            local_cmd.arg("-c");
            local_cmd.arg(&code);
            local_cmd.current_dir(&context.cwd);
            local_cmd.envs(&context.env);
            local_cmd
        };

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
                        lifecycle: Some(BlockLifecycleEvent::Error {
                            message: format!("Failed to spawn process: {}", e),
                        }),
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
                    // Send completion event on cancellation
                    let _ = event_sender.send(WorkflowEvent::BlockFinished { id: script.id });
                    // Send cancelled lifecycle event
                    if let Some(ref ch) = output_channel {
                        let _ = ch.send(BlockOutput {
                            stdout: None,
                            stderr: None,
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
                                    lifecycle: Some(BlockLifecycleEvent::Error {
                                        message: format!("Failed to wait for process: {e}")
                                    }),
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
                            lifecycle: Some(BlockLifecycleEvent::Error {
                                message: format!("Failed to wait for process: {}", e),
                            }),
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
                lifecycle: Some(BlockLifecycleEvent::Finished {
                    exit_code: Some(exit_code),
                    success: exit_code == 0,
                }),
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

        // Test both ${VAR} and $VAR syntax
        let (exit_code, output) = ScriptHandler::run_script(
            &create_test_script("echo '${TEST_VAR} and $ANOTHER_VAR'", "bash"),
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

        // No variables in context, should leave placeholders as-is
        let (exit_code, output) = ScriptHandler::run_script(
            &create_test_script("echo '${MISSING_VAR}'", "bash"),
            create_test_context(),
            CancellationToken::new(),
            _tx,
            None,
        )
        .await;

        assert!(exit_code.is_ok());
        assert_eq!(exit_code.unwrap(), 0);
        assert!(output.contains("${MISSING_VAR}"));
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
        context.cwd = "/tmp".to_string();

        let (exit_code, output) = ScriptHandler::run_script(
            &create_test_script("pwd", "bash"),
            context,
            CancellationToken::new(),
            _tx,
            None,
        )
        .await;

        assert!(exit_code.is_ok());
        assert_eq!(exit_code.unwrap(), 0);
        assert!(output.trim().ends_with("tmp"));
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
        let result = timeout(Duration::from_secs(1), script_future).await;

        // Should timeout
        assert!(result.is_err());
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
        assert!(output.contains("测试"));
        assert!(output.contains("🚀"));
        assert!(output.contains("émojis"));
    }

    // Integration test for SSH execution (would need SSH setup to run)
    #[tokio::test]
    #[ignore] // Ignore by default since it requires SSH setup
    async fn test_ssh_execution() {
        let (_tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        let mut context = create_test_context();
        context.ssh_host = Some("localhost".to_string());

        let (exit_code, output) = ScriptHandler::run_script(
            &create_test_script("echo 'SSH test'", "bash"),
            context,
            CancellationToken::new(),
            _tx,
            None,
        )
        .await;

        // This would only pass if SSH is properly configured
        assert!(exit_code.is_ok());
        assert_eq!(exit_code.unwrap(), 0);
        assert!(output.contains("SSH test"));
    }
}
