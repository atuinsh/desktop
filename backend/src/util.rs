use eyre::Result;
use std::env;
use std::process::Command;

#[cfg(unix)]
use nix::unistd::{getuid, User};

/// Fetches the current user's shell from /etc/passwd
#[cfg(unix)]
pub fn get_user_shell() -> Result<String> {
    let uid = getuid();

    let user = User::from_uid(uid)?
        .ok_or_else(|| eyre::eyre!("Could not find passwd entry for uid {}", uid))?;

    let shell = user
        .shell
        .to_str()
        .ok_or_else(|| eyre::eyre!("Shell path contains invalid UTF-8"))?
        .to_string();

    Ok(shell)
}

/// Loads a login shell's environment and sets it on the current process
/// This function is resilient to different shells (bash, zsh, fish, etc.)
///
/// This function will timeout after 2 seconds to prevent hanging the app startup.
#[cfg(unix)]
pub async fn load_login_shell_environment() -> Result<()> {
    // Add a timeout to prevent hanging during app startup
    let timeout_duration = std::time::Duration::from_secs(2);

    tokio::time::timeout(timeout_duration, load_login_shell_environment_impl())
        .await
        .map_err(|_| eyre::eyre!("Timeout while loading login shell environment"))?
}

#[cfg(unix)]
async fn load_login_shell_environment_impl() -> Result<()> {
    let shell = get_user_shell()?;

    log::info!("Loading environment from login shell: {}", shell);

    // Try different approaches based on the shell type
    let shell_name = std::path::Path::new(&shell)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    // Attempt to capture environment with different flag combinations
    let attempts = match shell_name {
        "fish" => vec![
            // Fish uses -l for login and -c for command
            vec!["-l", "-c", "env"],
            // Fallback without login flag
            vec!["-c", "env"],
        ],
        "bash" | "zsh" | "sh" => vec![
            // Standard POSIX shells use -l for login shell
            vec!["-l", "-c", "env"],
            // Some shells might use --login
            vec!["--login", "-c", "env"],
            // Fallback without login flag
            vec!["-c", "env"],
        ],
        _ => vec![
            // Generic approach for unknown shells
            vec!["-l", "-c", "env"],
            vec!["--login", "-c", "env"],
            vec!["-c", "env"],
        ],
    };

    let mut last_error = None;

    for args in attempts {
        log::debug!("Attempting to capture environment with args: {:?}", args);

        match Command::new(&shell).args(&args).output() {
            Ok(output) if output.status.success() => {
                let env_output = String::from_utf8(output.stdout)?;

                // Parse the environment variables and set them
                // Always update critical variables like PATH, but skip others if already set
                let always_update = ["PATH", "HOME", "SHELL", "USER"];
                let mut count = 0;

                for line in env_output.lines() {
                    if let Some(eq_pos) = line.find('=') {
                        let (key, value) = line.split_at(eq_pos);
                        let value = &value[1..]; // Skip the '=' character

                        // Skip empty keys or keys with invalid characters
                        if !key.is_empty() && key.chars().all(|c| c.is_alphanumeric() || c == '_') {
                            // Always update critical variables, or set if not already present
                            let should_set = always_update.contains(&key) || env::var(key).is_err();

                            if should_set {
                                unsafe {
                                    env::set_var(key, value);
                                }
                                count += 1;
                            }
                        }
                    }
                }

                log::info!(
                    "Loaded {} environment variables from login shell. PATH: {}",
                    count,
                    env::var("PATH").unwrap_or_default()
                );

                return Ok(());
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                log::debug!(
                    "Shell command failed with status {:?}: {}",
                    output.status,
                    stderr
                );
                last_error = Some(format!(
                    "Shell exited with status {:?}: {}",
                    output.status, stderr
                ));
            }
            Err(e) => {
                log::debug!("Failed to execute shell: {}", e);
                last_error = Some(e.to_string());
            }
        }
    }

    eyre::bail!(
        "Failed to capture environment from shell after all attempts. Last error: {}",
        last_error.unwrap_or_else(|| "unknown error".to_string())
    )
}

#[cfg(not(unix))]
pub fn get_user_shell() -> Result<String> {
    eyre::bail!("get_user_shell is only supported on Unix systems");
}

#[cfg(not(unix))]
pub async fn load_login_shell_environment() -> Result<()> {
    eyre::bail!("load_login_shell_environment is only supported on Unix systems");
}
