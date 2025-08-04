use async_trait::async_trait;
use std::process::Stdio;
use tauri::{ipc::Channel, AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;
use std::sync::Arc;

use crate::runtime::blocks::handler::{BlockHandler, BlockOutput, BlockLifecycleEvent, CancellationToken, ExecutionContext, ExecutionHandle, ExecutionStatus};
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
            if let (ExecutionStatus::Success(output), Some(var_name)) = (&status, &handle_clone.output_variable) {
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
    ) -> (Result<i32, Box<dyn std::error::Error + Send + Sync>>, String) {
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
                            message: format!("Failed to spawn process: {}", e) 
                        }),
                    });
                }
                return (Err(e.into()), String::new())
            },
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
                                        message: format!("Failed to wait for process: {}", e) 
                                    }),
                                });
                            }
                            return (Err(format!("Failed to wait for process: {}", e).into()), captured);
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
                                message: format!("Failed to wait for process: {}", e) 
                            }),
                        });
                    }
                    return (Err(format!("Failed to wait for process: {}", e).into()), captured);
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
                    success: exit_code == 0 
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

    #[tokio::test]
    async fn test_script_execution() {
        let (tx, _rx) = broadcast::channel(16);
        let handler = ScriptHandler;

        let script = Script::builder()
            .id(Uuid::new_v4())
            .name("Test Script")
            .code("echo 'Hello, World!'")
            .interpreter("bash")
            .output_variable(None)
            .build();

        let context = ExecutionContext::default();

        // Note: AppHandle not available in tests, so this test would need to be adjusted
        // let handle = handler.execute(script, context, tx, None, app_handle).await.unwrap();

        // Wait a bit for execution
        // tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // let status = handle.status.read().await;
        // match &*status {
        //     ExecutionStatus::Success(output) => {
        //         // Test would check output here
        //     }
        //     _ => panic!("Expected success status"),
        // }
    }
}