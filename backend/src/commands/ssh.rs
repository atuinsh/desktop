use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Information about an SSH key found in ~/.ssh
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshKeyInfo {
    /// Filename (e.g., "id_ed25519")
    pub name: String,
    /// Full path to the key file
    pub path: String,
    /// Detected key type if available (e.g., "ed25519", "rsa")
    pub key_type: Option<String>,
    /// Whether this is a standard SSH key name
    pub is_standard: bool,
}

/// List SSH private keys available in ~/.ssh
#[tauri::command]
pub async fn list_ssh_keys() -> Result<Vec<SshKeyInfo>, String> {
    let ssh_dir = dirs::home_dir()
        .ok_or("Cannot determine home directory")?
        .join(".ssh");

    if !ssh_dir.exists() {
        return Ok(vec![]);
    }

    let standard_names = [
        "id_rsa",
        "id_ecdsa",
        "id_ecdsa_sk",
        "id_ed25519",
        "id_ed25519_sk",
        "id_xmss",
        "id_dsa",
    ];

    let mut keys: Vec<SshKeyInfo> = vec![];

    let entries = fs::read_dir(&ssh_dir).map_err(|e| format!("Failed to read ~/.ssh: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();

        // Skip public keys, known_hosts, config, and other non-key files
        if name.ends_with(".pub")
            || name == "known_hosts"
            || name == "known_hosts.old"
            || name == "config"
            || name == "authorized_keys"
            || name == "environment"
            || name.starts_with(".")
        {
            continue;
        }

        // Only process regular files
        if !path.is_file() {
            continue;
        }

        let is_standard = standard_names.contains(&name.as_str());
        let key_type = detect_key_type(&path);

        // Only include if it looks like a private key
        if key_type.is_some() || is_standard {
            keys.push(SshKeyInfo {
                name,
                path: path.to_string_lossy().to_string(),
                key_type,
                is_standard,
            });
        }
    }

    // Sort: standard keys first, then alphabetically
    keys.sort_by(|a, b| match (a.is_standard, b.is_standard) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });

    Ok(keys)
}

/// Detect the key type from the file content
fn detect_key_type(path: &PathBuf) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let first_line = content.lines().next()?;

    // Check for common private key headers
    if first_line.contains("RSA PRIVATE KEY") {
        Some("rsa".to_string())
    } else if first_line.contains("EC PRIVATE KEY") || first_line.contains("ECDSA") {
        Some("ecdsa".to_string())
    } else if first_line.contains("OPENSSH PRIVATE KEY") {
        // OpenSSH format - need to look deeper or just return generic
        Some("openssh".to_string())
    } else if first_line.contains("DSA PRIVATE KEY") {
        Some("dsa".to_string())
    } else if first_line.starts_with("-----BEGIN") && first_line.contains("PRIVATE KEY") {
        Some("unknown".to_string())
    } else {
        None
    }
}
