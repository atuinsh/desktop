use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot, RwLock};
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::blocks::{Block, BlockBehavior};
use crate::context::{BlockExecutionOutput, DocumentVar};
use crate::execution::{
    BlockOutput, CancellationToken, ExecutionContext, ExecutionHandle, ExecutionStatus,
};

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
                    .map(|s| s.to_string()),
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct ScriptOutput {
    pub exit_code: i32,
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
        log::trace!("Executing script block {id}", id = self.id);

        log::trace!(
            "Script block {id} execution handle created; ID = {handle_id}",
            id = self.id,
            handle_id = context.handle().id
        );

        let context_clone = context.clone();
        tokio::spawn(async move {
            let (exit_code, captured_output) = self
                .run_script(context.clone(), context.cancellation_token())
                .await;

            log::trace!(
                "Script block {id} execution completed; Exit code = {exit_code}",
                id = self.id,
                exit_code = exit_code
                    .as_ref()
                    .map(|c| c.to_string())
                    .unwrap_or("(none)".to_string())
            );

            // Determine status based on exit code
            match exit_code {
                Ok(0) => {
                    let output = captured_output.trim().to_string();

                    // Store output variable as DocumentVar in context
                    if let Some(var_name) = &self.output_variable {
                        let block_id = self.id;
                        let var_name_clone = var_name.clone();
                        let output_clone = output.clone();

                        let _ = context
                            .update_active_context(block_id, move |ctx| {
                                log::trace!(
                                    "Storing output variable {var_name_clone} for script block {block_id}",
                                    var_name_clone = var_name_clone,
                                    block_id = block_id
                                );
                                ctx.insert(DocumentVar::new(var_name_clone, output_clone, "(script output)".to_string()));
                            })
                            .await;
                    }

                    // Store execution output in context
                    let block_id = self.id;
                    let _ = context
                        .update_active_context(block_id, move |ctx| {
                            ctx.insert(BlockExecutionOutput {
                                exit_code: Some(0),
                                stdout: Some(output),
                                stderr: None,
                            });
                        })
                        .await;

                    ExecutionStatus::Success
                }
                Ok(code) => {
                    // Store execution output in context (failed)
                    let block_id = self.id;
                    let captured_clone = captured_output.clone();
                    let _ = context
                        .update_active_context(block_id, move |ctx| {
                            ctx.insert(BlockExecutionOutput {
                                exit_code: Some(code),
                                stdout: Some(captured_clone),
                                stderr: None,
                            });
                        })
                        .await;

                    ExecutionStatus::Failed(format!("Process exited with code {}", code))
                }
                Err(e) => ExecutionStatus::Failed(e.to_string()),
            };
        });

        Ok(Some(context_clone.handle()))
    }
}

impl Script {
    /// Parse SSH host string to extract username and hostname
    fn parse_ssh_host(ssh_host: &str) -> (Option<String>, String) {
        if let Some(at_pos) = ssh_host.find('@') {
            let username = ssh_host[..at_pos].to_string();
            let host_part = &ssh_host[at_pos + 1..];
            let hostname = if let Some(colon_pos) = host_part.find(':') {
                host_part[..colon_pos].to_string()
            } else {
                host_part.to_string()
            };
            (Some(username), hostname)
        } else {
            let hostname = if let Some(colon_pos) = ssh_host.find(':') {
                ssh_host[..colon_pos].to_string()
            } else {
                ssh_host.to_string()
            };
            (None, hostname)
        }
    }

    async fn run_script(
        &self,
        context: ExecutionContext,
        cancellation_token: CancellationToken,
    ) -> (
        Result<i32, Box<dyn std::error::Error + Send + Sync>>,
        String,
    ) {
        // Send started lifecycle event to output channel
        log::trace!(
            "Sending started lifecycle event to output channel for script block {id}",
            id = self.id
        );

        let _ = context.block_started().await;

        // Template the script code
        let code = context
            .context_resolver
            .resolve_template(&self.code)
            .unwrap_or_else(|e| {
                log::warn!("Templating error in script {id}: {e}", id = self.id, e = e);
                self.code.clone()
            });

        // Check if SSH execution is needed
        let ssh_host = context.context_resolver.ssh_host().cloned();
        if let Some(ssh_host) = ssh_host {
            log::trace!(
                "Executing SSH script for script block {id} with SSH host {ssh_host}",
                id = self.id,
                ssh_host = ssh_host
            );

            return self
                .execute_ssh_script(&code, &ssh_host, context, cancellation_token)
                .await;
        }

        // Local execution
        let mut cwd = context.context_resolver.cwd().to_string();
        if cwd.is_empty() {
            cwd = std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        }
        let env_vars = context.context_resolver.env_vars();

        let mut cmd = Command::new(&self.interpreter);
        cmd.arg("-c");
        cmd.arg(&code);
        cmd.current_dir(&cwd);
        cmd.envs(env_vars);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.stdin(Stdio::null());

        #[cfg(unix)]
        {
            cmd.process_group(0);
        }

        log::trace!("Spawning process for script block {id}", id = self.id,);

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                return (Err(e.into()), String::new());
            }
        };
        let pid = child.id();

        let captured_output = Arc::new(RwLock::new(String::new()));

        // Capture stdout
        if let Some(stdout) = child.stdout.take() {
            let context_clone = context.clone();
            let capture = captured_output.clone();
            let block_id = self.id;

            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();
                while let Ok(n) = reader.read_line(&mut line).await {
                    if n == 0 {
                        break;
                    }
                    log::trace!(
                        "Sending stdout line to output channel for script block {id}",
                        id = block_id
                    );

                    let _ = context_clone
                        .send_output(
                            BlockOutput::builder()
                                .block_id(block_id)
                                .stdout(line.clone())
                                .build(),
                        )
                        .await;
                    let mut captured = capture.write().await;
                    captured.push_str(&line);
                    line.clear();
                }
            });
        }

        // Stream stderr
        if let Some(stderr) = child.stderr.take() {
            let context_clone = context.clone();
            let block_id = self.id;

            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                while let Ok(n) = reader.read_line(&mut line).await {
                    if n == 0 {
                        break;
                    }
                    log::trace!(
                        "Sending stderr line to output channel for script block {id}",
                        id = block_id
                    );

                    let _ = context_clone
                        .send_output(
                            BlockOutput::builder()
                                .block_id(block_id)
                                .stderr(line.clone())
                                .build(),
                        )
                        .await;
                    line.clear();
                }
            });
        }

        // Wait for completion or cancellation
        let cancellation_receiver = cancellation_token.take_receiver();
        let exit_code = if let Some(cancel_rx) = cancellation_receiver {
            tokio::select! {
                _ = cancel_rx => {
                    log::trace!("Process for script block {id} cancelled", id = self.id);

                    // Kill the process
                    if let Some(pid) = pid {
                        #[cfg(unix)]
                        {
                            use nix::sys::signal::{self, Signal};
                            use nix::unistd::Pid;
                            log::trace!("Sending SIGTERM to process {pid}", pid = pid);
                            // Send SIGTERM to the process group
                            let _ = signal::kill(Pid::from_raw(-(pid as i32)), Signal::SIGTERM);
                        }
                        #[cfg(windows)]
                        {
                            let _ = child.kill().await;
                        }
                    }
                    let captured = captured_output.read().await.clone();

                    let _ = context.block_cancelled().await;

                    return (Err("Script execution cancelled".into()), captured);
                }
                result = child.wait() => {
                    match result {
                        Ok(status) => status.code().unwrap_or(-1),
                        Err(e) => {
                            let captured = captured_output.read().await.clone();
                            let _ = context.block_failed(format!("Failed to wait for process: {}", e)).await;
                            return (Err(format!("Failed to wait for process: {}", e).into()), captured);
                        }
                    }
                }
            }
        } else {
            match child.wait().await {
                Ok(status) => status.code().unwrap_or(-1),
                Err(e) => {
                    let captured = captured_output.read().await.clone();
                    let _ = context
                        .block_failed(format!("Failed to wait for process: {}", e))
                        .await;
                    return (
                        Err(format!("Failed to wait for process: {}", e).into()),
                        captured,
                    );
                }
            }
        };

        if exit_code == 0 {
            let _ = context
                .block_finished(Some(exit_code), exit_code == 0)
                .await;
        } else {
            let _ = context
                .block_failed(format!("Script exited with code {}", exit_code))
                .await;
        }

        let captured = captured_output.read().await.clone();
        (Ok(exit_code), captured)
    }

    async fn execute_ssh_script(
        &self,
        code: &str,
        ssh_host: &str,
        context: ExecutionContext,
        cancellation_token: CancellationToken,
    ) -> (
        Result<i32, Box<dyn std::error::Error + Send + Sync>>,
        String,
    ) {
        let (username, hostname) = Self::parse_ssh_host(ssh_host);

        let ssh_pool = match &context.ssh_pool {
            Some(pool) => pool,
            None => {
                let error_msg = "SSH pool not available in execution context";
                let _ = context.block_failed(error_msg.to_string()).await;
                return (Err(error_msg.into()), String::new());
            }
        };

        let channel_id = self.id.to_string();
        let (output_sender, mut output_receiver) = mpsc::channel::<String>(100);
        let (result_tx, result_rx) = oneshot::channel::<()>();

        let captured_output = Arc::new(RwLock::new(String::new()));
        let captured_output_clone = captured_output.clone();

        let exec_result = ssh_pool
            .exec(
                &hostname,
                username.as_deref(),
                &self.interpreter,
                code,
                &channel_id,
                output_sender,
                result_tx,
            )
            .await;

        if let Err(e) = exec_result {
            let error_msg = format!("Failed to start SSH execution: {}", e);
            let _ = context.block_failed(error_msg.to_string()).await;
            return (Err(error_msg.into()), String::new());
        }

        let cancellation_receiver = cancellation_token.take_receiver();
        let context_clone = context.clone();
        let block_id = self.id;
        let ssh_pool_clone = ssh_pool.clone();
        let channel_id_clone = channel_id.clone();

        tokio::spawn(async move {
            while let Some(line) = output_receiver.recv().await {
                let _ = context_clone
                    .send_output(
                        BlockOutput::builder()
                            .block_id(block_id)
                            .stdout(line.clone())
                            .build(),
                    )
                    .await;
                let mut captured = captured_output_clone.write().await;
                captured.push_str(&line);
            }
        });

        let exit_code = if let Some(cancel_rx) = cancellation_receiver {
            tokio::select! {
                _ = cancel_rx => {
                    let _ = ssh_pool_clone.exec_cancel(&channel_id_clone).await;
                    let captured = captured_output.read().await.clone();

                    let _ = context.block_cancelled().await;
                    return (Err("SSH script execution cancelled".into()), captured);
                }
                _ = result_rx => {
                    0
                }
            }
        } else {
            let _ = result_rx.await;
            0
        };

        let _ = context
            .block_finished(Some(exit_code), exit_code == 0)
            .await;

        let captured = captured_output.read().await.clone();
        (Ok(exit_code), captured)
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
        let (event_sender, _event_receiver) = tokio::sync::broadcast::channel(16);

        let block_id = Uuid::new_v4();
        ExecutionContext::builder()
            .block_id(block_id)
            .runbook_id(Uuid::new_v4())
            .document_handle(document_handle)
            .context_resolver(Arc::new(context_resolver))
            .workflow_event_sender(event_sender)
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

        let (event_sender, _event_receiver) = tokio::sync::broadcast::channel(16);

        let block_id = Uuid::new_v4();
        ExecutionContext::builder()
            .block_id(block_id)
            .runbook_id(Uuid::new_v4())
            .document_handle(document_handle)
            .context_resolver(Arc::new(context_resolver))
            .workflow_event_sender(event_sender)
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
        let (event_sender, _event_receiver) = tokio::sync::broadcast::channel(16);

        ExecutionContext::builder()
            .block_id(block_id)
            .runbook_id(Uuid::new_v4())
            .document_handle(document_handle)
            .context_resolver(Arc::new(context_resolver))
            .workflow_event_sender(event_sender)
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
            (Some("user".to_string()), "host.com".to_string())
        );

        assert_eq!(
            Script::parse_ssh_host("host.com:2222"),
            (None, "host.com".to_string())
        );
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
