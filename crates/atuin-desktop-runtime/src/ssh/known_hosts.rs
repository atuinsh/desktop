//! SSH host key verification and known_hosts management
//!
//! This module provides:
//! - Reading from both system (`~/.ssh/known_hosts`) and app-specific known_hosts files
//! - Writing only to the app-specific file
//! - Interactive host key verification via the `HostKeyVerifier` trait
//! - Integration with the UI prompting system via ExecutionContext

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use async_trait::async_trait;
use russh::keys::PublicKey;
use tokio::sync::{mpsc, oneshot};

use crate::client::{ClientPrompt, PromptIcon, PromptOption, PromptOptionColor, PromptOptionVariant};
use crate::execution::ExecutionContext;

/// Result of checking a host key against known_hosts files
#[derive(Debug, Clone)]
pub enum HostKeyStatus {
    /// Key matches a known key (from either file)
    Known,
    /// Host not found in any known_hosts file
    Unknown {
        fingerprint: String,
        key_type: String,
    },
    /// Key has changed from what we have on file (potential MITM!)
    Changed {
        old_fingerprint: String,
        new_fingerprint: String,
        key_type: String,
        /// Which file contains the old key
        source: KnownHostsSource,
    },
}

/// Source of a known host key
#[derive(Debug, Clone)]
pub enum KnownHostsSource {
    /// System file: ~/.ssh/known_hosts
    System,
    /// App-specific file
    App,
}

impl std::fmt::Display for KnownHostsSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KnownHostsSource::System => write!(f, "~/.ssh/known_hosts"),
            KnownHostsSource::App => write!(f, "application settings"),
        }
    }
}

/// Error type for host key verification
#[derive(Debug, thiserror::Error)]
pub enum HostKeyError {
    #[error("Host key verification rejected by user")]
    Rejected,
    #[error("Host key has changed - connection refused for security")]
    KeyChanged,
    #[error("Failed to prompt user: {0}")]
    PromptFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Known hosts error: {0}")]
    KnownHosts(String),
}

/// Async trait for host key verification
///
/// Implementations can verify host keys against known_hosts files
/// and optionally prompt the user for unknown or changed keys.
#[async_trait]
pub trait HostKeyVerifier: Send + Sync {
    /// Verify a host key, potentially prompting the user
    ///
    /// Returns `Ok(true)` to accept the key, `Ok(false)` to reject,
    /// or `Err` for verification errors.
    async fn verify(
        &self,
        host: &str,
        port: u16,
        pubkey: &PublicKey,
    ) -> Result<bool, HostKeyError>;
}

/// A request to prompt the user about a host key
pub struct HostKeyPromptRequest {
    pub host: String,
    pub port: u16,
    pub status: HostKeyStatus,
    pub pubkey: PublicKey,
    pub response_tx: oneshot::Sender<HostKeyPromptResponse>,
}

/// User's response to a host key prompt
#[derive(Debug, Clone, Copy)]
pub enum HostKeyPromptResponse {
    /// Accept the key for this connection only
    Accept,
    /// Accept and save the key for future connections
    AcceptAndSave,
    /// Reject the connection
    Reject,
}

/// Manages known_hosts file operations
///
/// Reads from both system and app-specific files, but only writes to app-specific.
#[derive(Clone)]
pub struct KnownHostsManager {
    /// System known_hosts file (read-only): ~/.ssh/known_hosts
    system_path: PathBuf,
    /// App-specific known_hosts file (read/write)
    app_path: PathBuf,
}

impl Default for KnownHostsManager {
    fn default() -> Self {
        Self::new()
    }
}

impl KnownHostsManager {
    /// Create a new KnownHostsManager with default paths
    pub fn new() -> Self {
        let home = dirs::home_dir().expect("No home directory found");
        Self {
            system_path: home.join(".ssh").join("known_hosts"),
            app_path: home
                .join(".config")
                .join("atuin-desktop")
                .join("known_hosts"),
        }
    }

    /// Check if a host key is known
    ///
    /// Checks app-specific file first, then system file.
    pub fn check(&self, host: &str, port: u16, pubkey: &PublicKey) -> HostKeyStatus {
        let fingerprint = Self::compute_fingerprint(pubkey);
        let key_type = Self::get_key_type(pubkey);

        // Check app-specific file first
        if self.app_path.exists() {
            match self.check_file(&self.app_path, host, port, pubkey) {
                CheckResult::Known => return HostKeyStatus::Known,
                CheckResult::Changed(old_fp) => {
                    return HostKeyStatus::Changed {
                        old_fingerprint: old_fp,
                        new_fingerprint: fingerprint,
                        key_type,
                        source: KnownHostsSource::App,
                    };
                }
                CheckResult::NotFound => {} // Continue to system file
            }
        }

        // Check system file
        if self.system_path.exists() {
            match self.check_file(&self.system_path, host, port, pubkey) {
                CheckResult::Known => return HostKeyStatus::Known,
                CheckResult::Changed(old_fp) => {
                    return HostKeyStatus::Changed {
                        old_fingerprint: old_fp,
                        new_fingerprint: fingerprint,
                        key_type,
                        source: KnownHostsSource::System,
                    };
                }
                CheckResult::NotFound => {} // Not found anywhere
            }
        }

        // Host not found in either file
        HostKeyStatus::Unknown { fingerprint, key_type }
    }

    /// Learn a new host key (write to app-specific file only)
    pub fn learn(&self, host: &str, port: u16, pubkey: &PublicKey) -> Result<(), HostKeyError> {
        // Ensure parent directory exists
        if let Some(parent) = self.app_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Remove any existing entry for this host first (in case of key change)
        if self.app_path.exists() {
            self.remove_host_entry(host, port)?;
        }

        // Format the host entry (use [host]:port for non-standard ports)
        let host_entry = if port == 22 {
            host.to_string()
        } else {
            format!("[{}]:{}", host, port)
        };

        // Get the key in OpenSSH format (e.g., "ssh-ed25519 AAAAC3...")
        let key_openssh = pubkey
            .to_openssh()
            .map_err(|e| HostKeyError::KnownHosts(format!("Failed to encode key: {}", e)))?;

        // Write the entry: "hostname key-type base64-key"
        let line = format!("{} {}\n", host_entry, key_openssh);

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.app_path)?;

        file.write_all(line.as_bytes())?;

        tracing::info!(
            "Saved host key for {}:{} to {}",
            host,
            port,
            self.app_path.display()
        );

        Ok(())
    }

    /// Check a specific known_hosts file
    fn check_file(&self, path: &PathBuf, host: &str, port: u16, pubkey: &PublicKey) -> CheckResult {
        // Read the file contents
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return CheckResult::NotFound,
        };

        // Format what we're looking for
        let host_pattern = if port == 22 {
            host.to_string()
        } else {
            format!("[{}]:{}", host, port)
        };

        // Also check for bare hostname in case port is 22
        let bare_host = host.to_string();

        let mut found_host = false;
        let mut found_matching_key = false;
        let mut old_fingerprint = None;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse the line: "hostname key-type base64-key [comment]"
            let parts: Vec<&str> = line.splitn(3, ' ').collect();
            if parts.len() < 2 {
                continue;
            }

            let line_host = parts[0];

            // Check if this line matches our host
            let matches = line_host == host_pattern
                || (port == 22 && line_host == bare_host)
                || Self::host_matches_pattern(line_host, host, port);

            if !matches {
                continue;
            }

            found_host = true;

            // Parse the key from the line (parts[1] and parts[2] if present)
            let key_str = if parts.len() >= 3 {
                format!("{} {}", parts[1], parts[2].split_whitespace().next().unwrap_or(""))
            } else {
                parts[1].to_string()
            };

            // Try to parse the known key
            if let Ok(known_key) = key_str.parse::<PublicKey>() {
                if &known_key == pubkey {
                    found_matching_key = true;
                    break;
                }
                // Store fingerprint of first non-matching key for "changed" case
                if old_fingerprint.is_none() {
                    old_fingerprint = Some(Self::compute_fingerprint(&known_key));
                }
            }
        }

        if found_matching_key {
            CheckResult::Known
        } else if found_host {
            CheckResult::Changed(old_fingerprint.unwrap_or_else(|| "unknown".to_string()))
        } else {
            CheckResult::NotFound
        }
    }

    /// Check if a line's host pattern matches the target host
    fn host_matches_pattern(pattern: &str, host: &str, port: u16) -> bool {
        // Handle hashed hosts (|1|base64|base64) - we can't match these without the salt
        if pattern.starts_with("|1|") {
            return false;
        }

        // Handle [host]:port format
        if let Some(stripped) = pattern.strip_prefix('[') {
            if let Some(bracket_pos) = stripped.find(']') {
                let pattern_host = &stripped[..bracket_pos];
                let rest = &stripped[bracket_pos + 1..];
                if let Some(port_str) = rest.strip_prefix(':') {
                    if let Ok(pattern_port) = port_str.parse::<u16>() {
                        return pattern_host == host && pattern_port == port;
                    }
                }
            }
        }

        // Simple hostname match (only valid for port 22)
        port == 22 && pattern == host
    }

    /// Remove an existing host entry from the app-specific file
    fn remove_host_entry(&self, host: &str, port: u16) -> Result<(), HostKeyError> {
        if !self.app_path.exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(&self.app_path)?;
        let host_pattern = if port == 22 {
            host.to_string()
        } else {
            format!("[{}]:{}", host, port)
        };

        let filtered: Vec<&str> = content
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    return true; // Keep empty lines and comments
                }
                // Check if line starts with our host pattern
                !trimmed.starts_with(&host_pattern)
            })
            .collect();

        std::fs::write(&self.app_path, filtered.join("\n") + "\n")?;
        Ok(())
    }

    /// Compute SHA256 fingerprint of a public key
    pub fn compute_fingerprint(pubkey: &PublicKey) -> String {
        // Use russh's built-in fingerprint method with default SHA-256
        pubkey.fingerprint(Default::default()).to_string()
    }

    /// Get the key type string (e.g., "ssh-ed25519", "ssh-rsa")
    pub fn get_key_type(pubkey: &PublicKey) -> String {
        pubkey.algorithm().to_string()
    }
}

/// Internal result of checking a single file
enum CheckResult {
    Known,
    Changed(String), // old fingerprint
    NotFound,
}

/// Interactive verifier that uses channels to prompt the user
///
/// This verifier checks known_hosts files and sends prompt requests
/// via a channel when user interaction is needed.
pub struct InteractiveHostKeyVerifier {
    known_hosts: KnownHostsManager,
    prompt_tx: mpsc::Sender<HostKeyPromptRequest>,
}

impl InteractiveHostKeyVerifier {
    /// Create a new interactive verifier
    pub fn new(prompt_tx: mpsc::Sender<HostKeyPromptRequest>) -> Self {
        Self {
            known_hosts: KnownHostsManager::new(),
            prompt_tx,
        }
    }
}

#[async_trait]
impl HostKeyVerifier for InteractiveHostKeyVerifier {
    async fn verify(
        &self,
        host: &str,
        port: u16,
        pubkey: &PublicKey,
    ) -> Result<bool, HostKeyError> {
        let status = self.known_hosts.check(host, port, pubkey);

        match status {
            HostKeyStatus::Known => {
                tracing::debug!("Host key for {}:{} is known and trusted", host, port);
                Ok(true)
            }
            HostKeyStatus::Unknown { .. } | HostKeyStatus::Changed { .. } => {
                // Need to prompt user
                let (response_tx, response_rx) = oneshot::channel();

                let request = HostKeyPromptRequest {
                    host: host.to_string(),
                    port,
                    status: status.clone(),
                    pubkey: pubkey.clone(),
                    response_tx,
                };

                self.prompt_tx
                    .send(request)
                    .await
                    .map_err(|_| HostKeyError::PromptFailed("Prompt channel closed".into()))?;

                let response = response_rx
                    .await
                    .map_err(|_| HostKeyError::PromptFailed("No response received".into()))?;

                match response {
                    HostKeyPromptResponse::Accept => {
                        tracing::info!("User accepted host key for {}:{} (not saving)", host, port);
                        Ok(true)
                    }
                    HostKeyPromptResponse::AcceptAndSave => {
                        tracing::info!("User accepted host key for {}:{} (saving)", host, port);
                        if let Err(e) = self.known_hosts.learn(host, port, pubkey) {
                            tracing::warn!("Failed to save host key: {}", e);
                        }
                        Ok(true)
                    }
                    HostKeyPromptResponse::Reject => {
                        tracing::info!("User rejected host key for {}:{}", host, port);
                        Ok(false)
                    }
                }
            }
        }
    }
}

/// A verifier that accepts all keys (INSECURE - for testing only)
#[derive(Clone, Copy)]
pub struct AcceptAllVerifier;

#[async_trait]
impl HostKeyVerifier for AcceptAllVerifier {
    async fn verify(
        &self,
        host: &str,
        port: u16,
        _pubkey: &PublicKey,
    ) -> Result<bool, HostKeyError> {
        tracing::warn!(
            "AcceptAllVerifier: Accepting host key for {}:{} WITHOUT VERIFICATION (INSECURE)",
            host,
            port
        );
        Ok(true)
    }
}

/// Host key verifier that uses ExecutionContext to prompt the user
///
/// This verifier integrates directly with the desktop application's
/// prompt system via ExecutionContext::prompt_client().
#[derive(Clone)]
pub struct ContextHostKeyVerifier {
    known_hosts: KnownHostsManager,
    ctx: ExecutionContext,
}

impl ContextHostKeyVerifier {
    /// Create a new context-based verifier
    pub fn new(ctx: ExecutionContext) -> Self {
        Self {
            known_hosts: KnownHostsManager::new(),
            ctx,
        }
    }

    /// Build a prompt for an unknown host key
    fn build_unknown_prompt(host: &str, port: u16, fingerprint: &str, key_type: &str) -> ClientPrompt {
        let host_display = if port == 22 {
            host.to_string()
        } else {
            format!("{}:{}", host, port)
        };

        ClientPrompt::new(
            "Unknown SSH Host",
            &format!(
                "The authenticity of host '{}' can't be established.\n\n\
                 {} key fingerprint is:\n{}\n\n\
                 Are you sure you want to continue connecting?",
                host_display, key_type, fingerprint
            ),
        )
        .icon(PromptIcon::Question)
        .option(
            PromptOption::new("Yes, remember this host", "accept_save")
                .variant(PromptOptionVariant::Solid)
                .color(PromptOptionColor::Primary),
        )
        .option(
            PromptOption::new("Yes, just this once", "accept")
                .variant(PromptOptionVariant::Light),
        )
        .option(
            PromptOption::new("No, abort connection", "reject")
                .variant(PromptOptionVariant::Light)
                .color(PromptOptionColor::Danger),
        )
    }

    /// Build a warning prompt for a changed host key (potential MITM attack)
    fn build_changed_prompt(
        host: &str,
        port: u16,
        old_fingerprint: &str,
        new_fingerprint: &str,
        key_type: &str,
        source: &KnownHostsSource,
    ) -> ClientPrompt {
        let host_display = if port == 22 {
            host.to_string()
        } else {
            format!("{}:{}", host, port)
        };

        ClientPrompt::new(
            "WARNING: HOST KEY HAS CHANGED!",
            &format!(
                "@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@\n\
                 @    WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!   @\n\
                 @@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@\n\n\
                 IT IS POSSIBLE THAT SOMEONE IS DOING SOMETHING NASTY!\n\
                 Someone could be eavesdropping on you right now (man-in-the-middle attack)!\n\
                 It is also possible that a host key has just been changed.\n\n\
                 Host: {}\n\
                 Key type: {}\n\n\
                 Old fingerprint (from {}):\n{}\n\n\
                 New fingerprint:\n{}\n\n\
                 If you expected this change, you can update the key.\n\
                 Otherwise, you should abort and investigate!",
                host_display, key_type, source, old_fingerprint, new_fingerprint
            ),
        )
        .icon(PromptIcon::Warning)
        .option(
            PromptOption::new("Abort connection (recommended)", "reject")
                .variant(PromptOptionVariant::Solid)
                .color(PromptOptionColor::Danger),
        )
        .option(
            PromptOption::new("Update key and connect", "accept_save")
                .variant(PromptOptionVariant::Light)
                .color(PromptOptionColor::Warning),
        )
        .option(
            PromptOption::new("Connect once (don't update)", "accept")
                .variant(PromptOptionVariant::Light)
                .color(PromptOptionColor::Warning),
        )
    }
}

#[async_trait]
impl HostKeyVerifier for ContextHostKeyVerifier {
    async fn verify(
        &self,
        host: &str,
        port: u16,
        pubkey: &PublicKey,
    ) -> Result<bool, HostKeyError> {
        let status = self.known_hosts.check(host, port, pubkey);

        match &status {
            HostKeyStatus::Known => {
                tracing::debug!("Host key for {}:{} is known and trusted", host, port);
                Ok(true)
            }
            HostKeyStatus::Unknown { fingerprint, key_type } => {
                let prompt = Self::build_unknown_prompt(host, port, fingerprint, key_type);

                let result = self
                    .ctx
                    .prompt_client(prompt)
                    .await
                    .map_err(|e| HostKeyError::PromptFailed(e.to_string()))?;

                match result.button.as_str() {
                    "accept_save" => {
                        tracing::info!("User accepted and saved host key for {}:{}", host, port);
                        if let Err(e) = self.known_hosts.learn(host, port, pubkey) {
                            tracing::warn!("Failed to save host key: {}", e);
                        }
                        Ok(true)
                    }
                    "accept" => {
                        tracing::info!("User accepted host key for {}:{} (not saving)", host, port);
                        Ok(true)
                    }
                    _ => {
                        tracing::info!("User rejected host key for {}:{}", host, port);
                        Ok(false)
                    }
                }
            }
            HostKeyStatus::Changed {
                old_fingerprint,
                new_fingerprint,
                key_type,
                source,
            } => {
                let prompt = Self::build_changed_prompt(
                    host,
                    port,
                    old_fingerprint,
                    new_fingerprint,
                    key_type,
                    source,
                );

                let result = self
                    .ctx
                    .prompt_client(prompt)
                    .await
                    .map_err(|e| HostKeyError::PromptFailed(e.to_string()))?;

                match result.button.as_str() {
                    "accept_save" => {
                        tracing::warn!(
                            "User accepted CHANGED host key for {}:{} - updating saved key",
                            host,
                            port
                        );
                        if let Err(e) = self.known_hosts.learn(host, port, pubkey) {
                            tracing::warn!("Failed to save host key: {}", e);
                        }
                        Ok(true)
                    }
                    "accept" => {
                        tracing::warn!(
                            "User accepted CHANGED host key for {}:{} (not saving)",
                            host,
                            port
                        );
                        Ok(true)
                    }
                    _ => {
                        tracing::info!(
                            "User rejected connection due to changed host key for {}:{}",
                            host,
                            port
                        );
                        Ok(false)
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_hosts_source_display() {
        assert_eq!(format!("{}", KnownHostsSource::System), "~/.ssh/known_hosts");
        assert_eq!(format!("{}", KnownHostsSource::App), "application settings");
    }
}
