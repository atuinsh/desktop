use std::collections::HashMap;
use std::path::Path;

use nix::unistd::Pid;
use tauri::State;
use tokio::process::Command;
use uuid::Uuid;

use crate::state::AtuinState;

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
pub async fn shell_exec_sync(
    state: State<'_, AtuinState>,
    interpreter: String,
    command: String,
    env: Option<HashMap<String, String>>,
    cwd: Option<String>,
    runbook_id: String,
    block_id: String,
) -> Result<String, String> {
    let documents = state.documents.read().await;
    let document = documents.get(&runbook_id).ok_or("Document not found")?;

    let mut workspace_context = HashMap::new();
    let workspace_root = if let Some(workspace_manager) = state.workspaces.lock().await.as_ref() {
        workspace_manager
            .workspace_root(&runbook_id)
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };
    workspace_context.insert("root".to_string(), workspace_root.to_string());

    let mut extra_template_context = HashMap::new();
    extra_template_context.insert("workspace".to_string(), workspace_context);

    let context_resolver = document
        .build_context_resolver(
            Uuid::parse_str(&block_id).map_err(|e| format!("Invalid block ID: {}", e))?,
            None,
        )
        .await
        .map_err(|e| format!("Failed to build context resolver: {}", e))?;

    let command = context_resolver
        .resolve_template(&command)
        .map_err(|e| format!("Failed to resolve template: {}", e))?;

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
