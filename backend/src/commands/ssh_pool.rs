use crate::pty::PtyMetadata;
use crate::run::pty::PTY_OPEN_CHANNEL;
use crate::runtime::ssh::session::SshAuth;
use crate::runtime::ssh_pool::SshPty;
use crate::state::AtuinState;

use eyre::Result;
use std::path::PathBuf;
use tauri::Emitter;
use uuid::Uuid;

/// Connect to an SSH host
#[tauri::command]
pub async fn ssh_connect(
    state: tauri::State<'_, AtuinState>,
    host: String,
    username: Option<String>,
    password: Option<String>,
    key_path: Option<String>,
    custom_agent_socket: Option<String>,
) -> Result<(), String> {
    let ssh_pool = state.ssh_pool();

    // Build SshAuth from provided parameters
    let mut ssh_auth = SshAuth::new();
    if let Some(username) = username {
        ssh_auth = ssh_auth.with_username(username);
    }
    if let Some(password) = password {
        ssh_auth = ssh_auth.with_password(password);
    }
    if let Some(key_path) = key_path {
        ssh_auth = ssh_auth.with_key_path(PathBuf::from(key_path));
    }
    if let Some(socket) = custom_agent_socket {
        ssh_auth = ssh_auth.with_custom_agent_socket(socket);
    }

    ssh_pool.connect(&host, ssh_auth).await.map_err(|e| {
        log::error!("Failed to connect to SSH host: {e}");
        e.to_string()
    })?;

    Ok(())
}

/// Disconnect from an SSH host
#[tauri::command]
pub async fn ssh_disconnect(
    state: tauri::State<'_, AtuinState>,
    host: String,
    username: String,
) -> Result<(), String> {
    let ssh_pool = state.ssh_pool();

    ssh_pool
        .disconnect(&host, &username)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// List all active SSH connections
#[tauri::command]
pub async fn ssh_list_connections(
    state: tauri::State<'_, AtuinState>,
) -> Result<Vec<String>, String> {
    let ssh_pool = state.ssh_pool();

    ssh_pool.list_connections().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ssh_exec(
    state: tauri::State<'_, AtuinState>,
    app: tauri::AppHandle,
    host: String,
    username: Option<String>,
    channel: &str,
    command: &str,
    interpreter: &str,
    custom_agent_socket: Option<String>,
) -> Result<String, String> {
    let ssh_pool = state.ssh_pool();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(100);
    let (result_tx, result_rx) = tokio::sync::oneshot::channel();

    // Build SshAuth from provided parameters
    let mut ssh_auth = SshAuth::new();
    if let Some(username) = username {
        ssh_auth = ssh_auth.with_username(username);
    }
    if let Some(socket) = custom_agent_socket {
        ssh_auth = ssh_auth.with_custom_agent_socket(socket);
    }

    // TODO(ellie): refactor the local script executor to work in the same way as the ssh remote does
    // this will allow us to use similar code for both local and remote execution, and have more reliable
    // local execution

    ssh_pool
        .exec(
            &host,
            ssh_auth,
            interpreter,
            command,
            channel,
            sender,
            result_tx,
        )
        .await
        .map_err(|e| e.to_string())?;

    while let Some(line) = receiver.recv().await {
        app.emit(channel, line).unwrap();
    }

    // Wait for the result_rx to be sent, indicating the command has finished
    // emit this to the frontend
    // TODO: use it to communicate the exit code of the command
    let channel = channel.to_string();
    tokio::task::spawn(async move {
        let _ = result_rx.await;
        let channel = format!("ssh_exec_finished:{channel}");
        log::debug!("Sending ssh_exec_finished event to {channel}");
        app.emit(channel.as_str(), "").unwrap();
    });

    Ok(String::new())
}

#[tauri::command]
pub async fn ssh_exec_cancel(
    state: tauri::State<'_, AtuinState>,
    channel: &str,
) -> Result<(), String> {
    let ssh_pool = state.ssh_pool();
    ssh_pool
        .exec_cancel(channel)
        .await
        .map_err(|e| e.to_string())
}

/// Open an interactive SSH PTY session
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn ssh_open_pty(
    state: tauri::State<'_, AtuinState>,
    app: tauri::AppHandle,
    host: &str,
    username: Option<String>,
    channel: &str,
    runbook: &str,
    block: &str,
    width: u16,
    height: u16,
    custom_agent_socket: Option<String>,
) -> Result<(), String> {
    let ssh_pool = state.ssh_pool();

    // Build SshAuth from provided parameters
    let mut ssh_auth = SshAuth::new();
    if let Some(username) = username {
        ssh_auth = ssh_auth.with_username(username);
    }
    if let Some(socket) = custom_agent_socket {
        ssh_auth = ssh_auth.with_custom_agent_socket(socket);
    }

    // Create channels for bidirectional communication
    let (output_sender, mut output_receiver) = tokio::sync::mpsc::channel(100);

    // Start the PTY session
    let pty_tx = ssh_pool
        .open_pty(host, ssh_auth, channel, output_sender, width, height)
        .await
        .map_err(|e| e.to_string())?;

    // Forward output from the PTY to the frontend
    let channel_name = format!("pty-{channel}");
    let app_clone = app.clone();
    tokio::task::spawn(async move {
        while let Some(output) = output_receiver.recv().await {
            if let Err(e) = app_clone.emit(&channel_name, output) {
                log::error!("Failed to emit PTY output: {e}");
                break;
            }
        }

        // When the output channel closes, notify the frontend
        let finished_channel = format!("ssh_pty_finished:{channel_name}");
        if let Err(e) = app_clone.emit(&finished_channel, "") {
            log::error!("Failed to emit PTY finished event: {e}");
        }
    });

    // to fit into the same plumbing as a local pty, we also need to emit PTY_OPEN_CHANNEL
    let nanoseconds_now = time::OffsetDateTime::now_utc().unix_timestamp_nanos();
    let meta = PtyMetadata {
        pid: Uuid::parse_str(channel).unwrap(),
        runbook: Uuid::parse_str(runbook).unwrap(),
        block: block.to_string(),
        created_at: nanoseconds_now as u64,
    };
    let ssh_pty = SshPty {
        tx: pty_tx.0,
        resize_tx: pty_tx.1,
        metadata: meta.clone(),
        ssh_pool: ssh_pool.clone(),
    };
    state
        .pty_store()
        .add_pty(Box::new(ssh_pty))
        .await
        .map_err(|e| e.to_string())?;

    app.emit(PTY_OPEN_CHANNEL, meta)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Send input to an open SSH PTY session
#[tauri::command]
pub async fn ssh_write_pty(
    state: tauri::State<'_, AtuinState>,
    channel: &str,
    input: &str,
) -> Result<(), String> {
    let ssh_pool = state.ssh_pool();
    let bytes = input.as_bytes().to_vec();
    ssh_pool
        .pty_write(channel, bytes.into())
        .await
        .map_err(|e| e.to_string())
}
