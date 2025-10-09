use std::collections::HashMap;
use std::path::Path;
use std::{process::Stdio, sync::Arc};

use atuin_common::utils::uuid_v7;
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::RwLock;

use crate::runtime::blocks::script::ScriptOutput;
use crate::runtime::blocks::Block;
use crate::state::AtuinState;

/// Execute a shell command and stream the output over a channel
/// Unlike a pty, this is not interactive
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ShellProps {
    pub env: Option<HashMap<String, String>>,
    pub cwd: Option<String>,
    pub runbook: Option<String>,
    pub block: Block,
}

#[tauri::command]
pub async fn term_process(pid: u32) -> Result<(), String> {
    nix::sys::signal::kill(Pid::from_raw(pid as i32), nix::sys::signal::SIGTERM)
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn check_binary_exists(path: String) -> Result<bool, String> {
    // Check if the binary exists and is executable
    let path = shellexpand::tilde(&path).to_string();
    let exists = tokio::fs::metadata(&path).await.map(|meta| {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            meta.is_file() && meta.permissions().mode() & 0o111 != 0
        }
        #[cfg(not(unix))]
        {
            meta.is_file()
        }
    });

    Ok(exists.unwrap_or(false))
}

#[tauri::command]
pub async fn shell_exec(
    app: tauri::AppHandle,
    state: State<'_, AtuinState>,
    interpreter: String,
    channel: String,
    command: String,
    props: ShellProps,
) -> Result<u32, String> {
    let block = if let Block::Script(ref script) = props.block {
        script.clone()
    } else {
        return Err("Block is not a script".to_string());
    };

    // Split interpreter string into command and args
    let parts: Vec<&str> = interpreter.split_whitespace().collect();
    let (cmd_name, cmd_args) = parts.split_first().unwrap_or((&"bash", &[]));

    let env = props.env.clone().unwrap_or_default();
    let cwd = props.clone().cwd.unwrap_or(String::from("~"));
    let cwd = shellexpand::tilde(&cwd).to_string();
    let path = Path::new(&cwd);

    let cmd = Command::new(cmd_name)
        .args(cmd_args)
        .arg(command)
        .current_dir(path)
        .envs(env)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn command: {path:?} {e}"))?;

    let cmd = Arc::new(RwLock::new(cmd));
    let nanoseconds_start = time::OffsetDateTime::now_utc().unix_timestamp_nanos();

    let id = uuid_v7();
    state.child_processes.write().await.insert(id, cmd.clone());

    // Spawn a channel for handling multithreaded writes to the output
    let (output_tx, mut output_rx) = tokio::sync::mpsc::channel::<String>(100);

    let output_vars = state.runbook_output_variables.clone();

    // Spawn a task to listen to the output channel and write to the buffer
    let props_clone = props.clone();
    let block_clone = block.clone();
    let app_clone = app.clone();
    let _output_handle = tokio::spawn(async move {
        // Create a string buffer to store the output
        let mut output = String::new();

        while let Some(line) = output_rx.recv().await {
            output.push_str(&line);

            // we're reading by lines, which eats the newline characters. sooooo add them back in :)
            output.push('\n');
        }

        // the above loop stops when the channel is closed
        // so we can set the output variable in the state
        if let Some(runbook) = props_clone.runbook {
            if let Some(output_variable) = block_clone.output_variable {
                if output_variable.is_empty() {
                    return;
                }

                output_vars
                    .write()
                    .await
                    .entry(runbook.clone())
                    .or_insert(HashMap::new())
                    .insert(output_variable.clone(), output);

                let _ = app_clone.emit(
                    "variable-changed",
                    format!("{}:{}", runbook, output_variable),
                );
            }
        }
    });

    // Spawn stdout reader
    let stdout = cmd.write().await.stdout.take().unwrap();
    let app_clone = app.clone();
    let channel_clone = channel.clone();

    let out_output_tx = output_tx.clone();
    let out_handle = tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Some(line) = lines.next_line().await.unwrap() {
            out_output_tx.send(line.clone()).await.unwrap();
            app_clone.emit(channel_clone.as_str(), line).unwrap();
        }
    });

    // Spawn stderr reader
    let stderr = cmd.write().await.stderr.take().unwrap();
    let app_clone = app.clone();
    let channel_clone = channel.clone();

    let err_output_tx = output_tx.clone();
    let err_handle = tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Some(line) = lines.next_line().await.unwrap() {
            err_output_tx.send(line.clone()).await.unwrap();
            app_clone.emit(channel_clone.as_str(), line).unwrap();
        }
    });

    // wait for the command in another task
    let pid = cmd.read().await.id().expect("Command has no id");
    let app_clone = app.clone();

    tauri::async_runtime::spawn(async move {
        let state = app_clone.state::<AtuinState>();
        let output = cmd.write().await.wait().await.unwrap();
        out_handle.abort();
        err_handle.abort();
        drop(output_tx);

        state.child_processes.write().await.remove(&id);

        let nanoseconds_end = time::OffsetDateTime::now_utc().unix_timestamp_nanos();
        let exit = output.code().unwrap_or(1);
        let output = ScriptOutput::builder().exit_code(exit).build();
        let output = serde_json::to_string(&output).unwrap();

        crate::commands::exec_log::log_execution(
            app_clone.clone(),
            state,
            Block::Script(block),
            nanoseconds_start as u64,
            nanoseconds_end as u64,
            output,
        )
        .await
        .unwrap();

        app_clone
            .emit(format!("shell_exec_finished:{pid}").as_str(), "")
            .unwrap();
    });

    Ok(pid)
}

#[tauri::command]
pub async fn shell_exec_sync(
    interpreter: String,
    command: String,
    env: Option<HashMap<String, String>>,
    cwd: Option<String>,
) -> Result<String, String> {
    // Split interpreter string into command and args
    let parts: Vec<&str> = interpreter.split_whitespace().collect();
    let (cmd_name, cmd_args) = parts.split_first().unwrap_or((&"bash", &[]));

    let env = env.clone().unwrap_or_default();
    let cwd = cwd.unwrap_or(String::from("~"));
    let cwd = shellexpand::tilde(&cwd).to_string();
    let path = Path::new(&cwd);

    let output = Command::new(cmd_name)
        .args(cmd_args)
        .arg(command)
        .current_dir(path)
        .envs(env)
        .output()
        .await
        .map_err(|e| format!("Failed to run command: {path:?} {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Combine stdout and stderr
    // TODO(ellie): think of some good examples to test and make a nicer way of combining output
    let mut combined_output = String::new();
    if !stdout.is_empty() {
        combined_output.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !combined_output.is_empty() {
            combined_output.push('\n');
        }
        combined_output.push_str(&stderr);
    }

    Ok(combined_output)
}
