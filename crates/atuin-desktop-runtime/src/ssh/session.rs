// Handle making SSH connections. Do not manage or pool them, just handle the actual plumbing.
// This is essentially a wrapper around the russh crate.

use bytes::Bytes;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::{mpsc::Sender, oneshot};
use tokio::time::timeout;

use eyre::Result;
use russh::client::Handle;
use russh::*;
use russh_config::*;

use crate::context::{DocumentSshConfig, SshIdentityKeyConfig};
use crate::ssh::SshPoolHandle;

/// Result of executing a simple command on the remote system
#[derive(Debug, Clone)]
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// An ssh session, wrapping the underlying russh with async-safe primitives
pub struct Session {
    session: Handle<Client>,
    ssh_config: SshConfig,
}

/// SSH connection configuration resolved from SSH config
#[derive(Debug, Clone)]
pub struct SshConfig {
    pub hostname: String,
    pub port: u16,
    pub username: Option<String>,
    pub identity_files: Vec<PathBuf>,
    pub proxy_command: Option<String>,
    pub proxy_jump: Option<String>,
    pub identity_agent: Option<String>,
}

/// Authentication methods
pub enum Authentication {
    Key(PathBuf),
    Password(String, String),
}

pub enum OutputLine {
    Stdout(String),
    Stderr(String),
}

impl OutputLine {
    pub fn inner(&self) -> &str {
        match self {
            Self::Stdout(text) => text,
            Self::Stderr(text) => text,
        }
    }

    pub fn is_stdout(&self) -> bool {
        matches!(self, Self::Stdout(_))
    }

    pub fn is_stderr(&self) -> bool {
        matches!(self, Self::Stderr(_))
    }
}

/// SSH client implementation for russh
pub struct Client;

impl russh::client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        // For now, accept all server keys
        // In production, you'd want to implement proper host key verification
        Ok(true)
    }
}

impl Session {
    /// Execute a simple command and capture its output
    /// This opens a new channel, runs the command through a shell, and returns stdout, stderr, and exit code
    /// Used for simple utility commands like mktemp, cat, rm
    pub(crate) async fn exec_and_capture(&self, command: &str) -> Result<CommandResult> {
        let mut channel = self.session.channel_open_session().await?;

        // Run through shell to ensure proper PATH and environment
        let shell_command = format!("sh -c '{}'", command.replace('\'', "'\"'\"'"));
        channel.exec(true, shell_command.as_str()).await?;

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code: Option<i32> = None;
        let mut got_eof = false;

        loop {
            let Some(msg) = channel.wait().await else {
                break;
            };

            match msg {
                ChannelMsg::Data { data } => {
                    if let Ok(data_str) = std::str::from_utf8(&data) {
                        stdout.push_str(data_str);
                    }
                }
                ChannelMsg::ExtendedData { data, ext: 1 } => {
                    // stderr
                    if let Ok(data_str) = std::str::from_utf8(&data) {
                        stderr.push_str(data_str);
                    }
                }
                ChannelMsg::ExitStatus { exit_status } => {
                    exit_code = Some(exit_status as i32);
                    // If we already got EOF, we can break now
                    if got_eof {
                        break;
                    }
                }
                ChannelMsg::Eof => {
                    got_eof = true;
                    // If we already got exit status, we can break now
                    if exit_code.is_some() {
                        break;
                    }
                }
                ChannelMsg::Close => {
                    break;
                }
                _ => {}
            }
        }

        let _ = channel.close().await;

        Ok(CommandResult {
            stdout,
            stderr,
            exit_code: exit_code.unwrap_or(0),
        })
    }

    /// Create a temporary file on the remote system
    /// Returns the path to the created file
    pub async fn create_temp_file(&self, prefix: &str) -> Result<String> {
        // Use more portable mktemp syntax without file extension
        let result = self
            .exec_and_capture(&format!("mktemp /tmp/{}-XXXXXXXX", prefix))
            .await?;

        if result.exit_code != 0 {
            return Err(eyre::eyre!(
                "Failed to create temp file (exit code {}): stdout='{}' stderr='{}'",
                result.exit_code,
                result.stdout.trim(),
                result.stderr.trim()
            ));
        }

        let path = result.stdout.trim().to_string();

        // Validate that we got a non-empty path
        if path.is_empty() {
            return Err(eyre::eyre!(
                "mktemp returned empty path: stdout='{}' stderr='{}' exit_code={}",
                result.stdout,
                result.stderr,
                result.exit_code
            ));
        }

        tracing::debug!("Created remote temp file: {}", path);
        Ok(path)
    }

    /// Read the contents of a file on the remote system
    pub async fn read_file(&self, path: &str) -> Result<String> {
        // Escape the path for shell safety
        let escaped_path = path.replace('\'', "'\"'\"'");
        let result = self
            .exec_and_capture(&format!("cat '{}'", escaped_path))
            .await?;

        if result.exit_code != 0 {
            return Err(eyre::eyre!(
                "Failed to read file: {} {}",
                result.stdout.trim(),
                result.stderr.trim()
            ));
        }

        Ok(result.stdout)
    }

    /// Delete a file on the remote system
    /// Uses rm -f so it doesn't fail if the file doesn't exist
    pub async fn delete_file(&self, path: &str) -> Result<()> {
        // Escape the path for shell safety
        let escaped_path = path.replace('\'', "'\"'\"'");
        let result = self
            .exec_and_capture(&format!("rm -f '{}'", escaped_path))
            .await?;

        // rm -f returns 0 even if file doesn't exist, but log if there's an error
        if result.exit_code != 0 {
            tracing::warn!(
                "Failed to delete file {}: {} {}",
                path,
                result.stdout.trim(),
                result.stderr.trim()
            );
        }

        Ok(())
    }

    /// Send a keepalive to test if the SSH connection is still active and responsive
    /// Uses a lightweight exec command that actually tests network connectivity
    pub async fn send_keepalive(&self) -> bool {
        const KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(5);

        let keepalive_check = async {
            let mut channel = self.session.channel_open_session().await.ok()?;
            channel.exec(true, "true").await.ok()?;

            let mut code = None;

            loop {
                // There's an event available on the session channel
                let Some(msg) = channel.wait().await else {
                    break;
                };
                if let ChannelMsg::ExitStatus { exit_status } = msg {
                    code = Some(exit_status);
                }
            }

            let _ = channel.close().await;
            code
        };

        match timeout(KEEPALIVE_TIMEOUT, keepalive_check).await {
            Ok(Some(_)) => true,
            Ok(None) => false,
            Err(_) => {
                tracing::debug!("SSH keepalive timed out");
                false
            }
        }
    }

    /// Parse IdentityAgent from SSH config manually (since russh-config doesn't support it)
    fn parse_identity_agent(host: &str) -> Option<String> {
        Self::parse_identity_agent_from_path(host, &dirs::home_dir()?.join(".ssh").join("config"))
    }

    /// Helper function to parse IdentityAgent from a specific config file path
    fn parse_identity_agent_from_path(host: &str, config_path: &std::path::Path) -> Option<String> {
        if !config_path.exists() {
            return None;
        }

        let content = std::fs::read_to_string(config_path).ok()?;
        let mut current_host_matches = false;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(host_line) = line
                .strip_prefix("Host ")
                .or_else(|| line.strip_prefix("host "))
            {
                // Check if this host section matches our target using glob patterns
                let hosts: Vec<&str> = host_line.split_whitespace().collect();
                current_host_matches = hosts.iter().any(|pattern| {
                    if pattern == &"*" {
                        true
                    } else if pattern.contains('*') || pattern.contains('?') {
                        // Use glob pattern matching
                        match glob::Pattern::new(pattern) {
                            Ok(glob_pattern) => glob_pattern.matches(host),
                            Err(_) => false,
                        }
                    } else {
                        pattern == &host
                    }
                });
            } else if current_host_matches {
                // Parse IdentityAgent under the matching host
                if let Some((key, value)) = line.split_once(' ').or_else(|| line.split_once('\t')) {
                    let key = key.trim().to_lowercase();
                    let value = value.trim().trim_matches('"');

                    if key == "identityagent" {
                        // Expand ~ to home directory
                        if let Some(pref) = value.strip_prefix("~/") {
                            if let Some(home) = dirs::home_dir() {
                                return Some(home.join(pref).to_string_lossy().to_string());
                            }
                        }
                        return Some(value.to_string());
                    }
                }
            }
        }

        None
    }

    /// Parse user@hostname:port format and extract components
    fn parse_host_string(input: &str) -> (Option<String>, String, Option<u16>) {
        // Handle user@host:port, user@host, host:port, or just host
        let (user_part, host_part) = if let Some(at_pos) = input.find('@') {
            let user = input[..at_pos].to_string();
            let host_part = &input[at_pos + 1..];
            (Some(user), host_part)
        } else {
            (None, input)
        };

        // Now parse host:port from the host part
        let (hostname, port) = if let Some(colon_pos) = host_part.rfind(':') {
            let host = host_part[..colon_pos].to_string();
            let port_str = &host_part[colon_pos + 1..];

            // Try to parse port as number
            match port_str.parse::<u16>() {
                Ok(port) => (host, Some(port)),
                Err(_) => {
                    // If port parsing fails, treat the whole thing as hostname
                    (host_part.to_string(), None)
                }
            }
        } else {
            (host_part.to_string(), None)
        };

        (user_part, hostname, port)
    }

    /// Resolve SSH configuration for a host using ~/.ssh/config with russh-config
    pub fn resolve_ssh_config(host: &str) -> SshConfig {
        // Parse the input to extract user, hostname, and port
        let (input_user, hostname, input_port) = Self::parse_host_string(host);

        let default_config = SshConfig {
            hostname: hostname.clone(),
            port: input_port.unwrap_or(22),
            username: input_user.clone(),
            identity_files: vec![],
            proxy_command: None,
            proxy_jump: None,
            identity_agent: None,
        };

        // Try to read SSH config using russh-config
        let config_path = dirs::home_dir().map(|home| home.join(".ssh").join("config"));

        if let Some(_path) = config_path {
            // Use russh-config to resolve host settings with glob pattern support
            // Pass only the hostname part to russh-config
            match parse_home(&hostname) {
                Ok(config) => {
                    let hostname = if config.host_name.is_empty() {
                        hostname.clone()
                    } else {
                        config.host_name
                    };
                    // Use input port if specified, otherwise use config port, otherwise default
                    let port = input_port.unwrap_or(config.port);
                    // Use input username if specified, otherwise use config username
                    let username = input_user.or({
                        if config.user.is_empty() {
                            None
                        } else {
                            Some(config.user)
                        }
                    });

                    // Collect identity files from config
                    let mut identity_files = Vec::new();
                    if let Some(identity_file) = config.identity_file {
                        if let Some(home) = dirs::home_dir() {
                            let path = if let Some(pref) = identity_file.strip_prefix("~/") {
                                home.join(pref)
                            } else if identity_file.starts_with('/') {
                                PathBuf::from(identity_file)
                            } else {
                                home.join(".ssh").join(identity_file)
                            };
                            if path.exists() {
                                identity_files.push(path);
                            }
                        }
                    }

                    let proxy_command = config.proxy_command.clone();
                    let proxy_jump = config.proxy_jump.clone();

                    // Parse IdentityAgent manually since russh-config doesn't support it
                    let identity_agent = Self::parse_identity_agent(&hostname);

                    tracing::debug!(
                        "Resolved SSH config for {host}: hostname={hostname}, port={port}, username={username:?}, identity_files={identity_files:?}, proxy_command={proxy_command:?}, proxy_jump={proxy_jump:?}"
                    );

                    return SshConfig {
                        hostname,
                        port,
                        username,
                        identity_files,
                        proxy_command,
                        proxy_jump,
                        identity_agent,
                    };
                }
                Err(e) => {
                    tracing::warn!("Failed to parse SSH config: {e}");
                }
            }
        }

        tracing::debug!("No SSH config found for {host}, using defaults");
        default_config
    }

    /// Open a new SSH session to the given host, and connect
    pub async fn open(host: &str) -> Result<Self> {
        let ssh_config = Self::resolve_ssh_config(host);

        let config = russh::client::Config::default();
        let sh = Client;

        // Parse the hostname for proxy connections
        let (_, hostname, _) = Self::parse_host_string(host);

        // Handle ProxyCommand and ProxyJump
        let session = if ssh_config.proxy_command.is_some() || ssh_config.proxy_jump.is_some() {
            tracing::debug!(
                "Using proxy for connection to {} (proxy_command: {:?}, proxy_jump: {:?})",
                host,
                ssh_config.proxy_command,
                ssh_config.proxy_jump
            );

            // Use russh-config's stream method to handle proxying
            match parse_home(&hostname) {
                Ok(parsed_config) => {
                    let stream = parsed_config.stream().await?;
                    russh::client::connect_stream(Arc::new(config), stream, sh).await?
                }
                Err(e) => {
                    tracing::warn!("Failed to create proxy stream: {e}");
                    // Fallback to direct connection
                    let address = format!("{}:{}", ssh_config.hostname, ssh_config.port);
                    tracing::debug!("Falling back to direct connection: {address}");
                    russh::client::connect(Arc::new(config), address.as_str(), sh).await?
                }
            }
        } else {
            // Direct connection
            let address = format!("{}:{}", ssh_config.hostname, ssh_config.port);
            tracing::debug!("Connecting directly to: {address}");
            russh::client::connect(Arc::new(config), address.as_str(), sh).await?
        };

        Ok(Session {
            session,
            ssh_config,
        })
    }

    /// Open a new SSH session with optional configuration overrides from block settings.
    /// Block settings take precedence over SSH config file.
    pub async fn open_with_config(host: &str, config_override: Option<&DocumentSshConfig>) -> Result<Self> {
        let mut ssh_config = Self::resolve_ssh_config(host);

        // Apply block-level overrides if provided
        if let Some(override_cfg) = config_override {
            if let Some(ref user) = override_cfg.user {
                if !user.is_empty() {
                    tracing::debug!("Overriding username from block settings: {}", user);
                    ssh_config.username = Some(user.clone());
                }
            }
            if let Some(ref hostname) = override_cfg.hostname {
                if !hostname.is_empty() {
                    tracing::debug!("Overriding hostname from block settings: {}", hostname);
                    ssh_config.hostname = hostname.clone();
                }
            }
            if let Some(port) = override_cfg.port {
                if port > 0 {
                    tracing::debug!("Overriding port from block settings: {}", port);
                    ssh_config.port = port;
                }
            }
        }

        let config = russh::client::Config::default();
        let sh = Client;

        // Handle ProxyCommand and ProxyJump
        let session = if ssh_config.proxy_command.is_some() || ssh_config.proxy_jump.is_some() {
            tracing::debug!(
                "Using proxy for connection to {} (proxy_command: {:?}, proxy_jump: {:?})",
                host,
                ssh_config.proxy_command,
                ssh_config.proxy_jump
            );

            match parse_home(&ssh_config.hostname) {
                Ok(parsed_config) => {
                    let stream = parsed_config.stream().await?;
                    russh::client::connect_stream(Arc::new(config), stream, sh).await?
                }
                Err(e) => {
                    tracing::warn!("Failed to create proxy stream: {e}");
                    let address = format!("{}:{}", ssh_config.hostname, ssh_config.port);
                    tracing::debug!("Falling back to direct connection: {address}");
                    russh::client::connect(Arc::new(config), address.as_str(), sh).await?
                }
            }
        } else {
            let address = format!("{}:{}", ssh_config.hostname, ssh_config.port);
            tracing::debug!("Connecting directly to: {address}");
            russh::client::connect(Arc::new(config), address.as_str(), sh).await?
        };

        Ok(Session {
            session,
            ssh_config,
        })
    }

    /// Password authentication
    pub async fn password_auth(&mut self, username: &str, password: &str) -> Result<()> {
        let auth_res = self
            .session
            .authenticate_password(username, password)
            .await?;

        if !matches!(auth_res, russh::client::AuthResult::Success) {
            return Err(eyre::eyre!("Password authentication failed"));
        }

        Ok(())
    }

    /// Get default SSH private keys in the same order as the ssh command
    ///
    /// This matches the exact behavior of OpenSSH, which tries these specific key files
    /// in this exact order (not scanning the entire ~/.ssh directory).
    ///
    /// Order matches ssh's default (as of OpenSSH 8.x+):
    /// 1. id_rsa
    /// 2. id_ecdsa
    /// 3. id_ecdsa_sk (FIDO/U2F security key)
    /// 4. id_ed25519
    /// 5. id_ed25519_sk (FIDO/U2F security key)
    /// 6. id_xmss
    /// 7. id_dsa
    fn default_ssh_keys() -> Vec<PathBuf> {
        let Some(home) = dirs::home_dir() else {
            return vec![];
        };

        let ssh_dir = home.join(".ssh");

        // Try keys in the exact order that ssh does
        vec![
            ssh_dir.join("id_rsa"),
            ssh_dir.join("id_ecdsa"),
            ssh_dir.join("id_ecdsa_sk"),
            ssh_dir.join("id_ed25519"),
            ssh_dir.join("id_ed25519_sk"),
            ssh_dir.join("id_xmss"),
            ssh_dir.join("id_dsa"),
        ]
        .into_iter()
        .filter(|path| path.exists())
        .collect()
    }

    /// Public key authentication
    pub async fn key_auth(&mut self, username: &str, key_path: PathBuf) -> Result<()> {
        tracing::info!(
            "Attempting public key authentication with {}",
            key_path.display()
        );

        let key_pair = match russh::keys::load_secret_key(&key_path, None) {
            Ok(kp) => kp,
            Err(e) => {
                tracing::warn!("Failed to load key {}: {e}", key_path.display());
                return Err(e.into());
            }
        };

        tracing::debug!("Key loaded successfully, authenticating...");

        // Query the server for the best RSA hash algorithm it supports
        // This ensures compatibility with both modern (SHA-256/SHA-512) and legacy (SHA-1) servers
        let best_hash = self.session.best_supported_rsa_hash().await?.flatten();
        let key_with_alg = russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key_pair), best_hash);

        let auth_res = self
            .session
            .authenticate_publickey(username, key_with_alg)
            .await?;

        match auth_res {
            russh::client::AuthResult::Success => {
                tracing::info!("✓ Authentication successful with {}", key_path.display());
                Ok(())
            }
            russh::client::AuthResult::Failure {
                remaining_methods,
                partial_success,
            } => {
                tracing::warn!(
                    "Server rejected key {} (remaining methods: {:?}, partial: {})",
                    key_path.display(),
                    remaining_methods,
                    partial_success
                );
                Err(eyre::eyre!(
                    "Public key authentication failed: server rejected key"
                ))
            }
        }
    }

    pub async fn agent_auth(&mut self, username: &str) -> Result<bool> {
        tracing::info!("Attempting SSH agent authentication for {username}");

        // Try to connect to SSH agent, using custom IdentityAgent if specified
        let agent_result = if let Some(ref identity_agent) = self.ssh_config.identity_agent {
            tracing::info!("Using custom IdentityAgent: {identity_agent}");
            // Connect to custom agent socket
            russh::keys::agent::client::AgentClient::connect_uds(identity_agent).await
        } else {
            tracing::info!("Using default SSH agent from environment");
            // Use default SSH agent from environment
            russh::keys::agent::client::AgentClient::connect_env().await
        };

        match agent_result {
            Ok(mut agent) => match agent.request_identities().await {
                Ok(keys) => {
                    tracing::info!("SSH agent has {} keys available", keys.len());
                    for (i, key) in keys.iter().enumerate() {
                        tracing::debug!("Trying SSH agent key #{}", i + 1);
                        match self
                            .session
                            .authenticate_publickey_with(username, key.clone(), None, &mut agent)
                            .await
                        {
                            Ok(russh::client::AuthResult::Success) => {
                                tracing::info!(
                                    "✓ Successfully authenticated with SSH agent key #{}",
                                    i + 1
                                );
                                return Ok(true);
                            }
                            Ok(_) => {
                                tracing::debug!("SSH agent key #{} rejected by server", i + 1);
                                continue;
                            }
                            Err(e) => {
                                tracing::debug!("Error trying SSH agent key #{}: {e:?}", i + 1);
                                continue;
                            }
                        }
                    }
                    tracing::info!("No SSH agent keys worked for authentication");
                    Ok(false)
                }
                Err(e) => {
                    tracing::info!("Failed to request identities from SSH agent: {e}");
                    Ok(false)
                }
            },
            Err(e) => {
                tracing::info!("Cannot connect to SSH agent: {e}");
                Ok(false)
            }
        }
    }

    /// Authenticate the session. If a username is provided, use it for authentication - otherwise we will use SSH config or the current user
    ///
    /// The authentication order matches the ssh command:
    /// 1. SSH Agent authentication
    /// 2. SSH config identity files
    /// 3. Default SSH keys (id_rsa, id_ecdsa, id_ecdsa_sk, id_ed25519, id_ed25519_sk, id_xmss, id_dsa)
    /// 4. Provided authentication method (password or key)
    pub async fn authenticate(
        &mut self,
        auth: Option<Authentication>,
        username: Option<&str>,
    ) -> Result<()> {
        // Clone values we need before any mutable borrows
        let config_username = self.ssh_config.username.clone();
        let identity_files = self.ssh_config.identity_files.clone();
        let current_user = whoami::username();

        // Use provided username, or SSH config username, or default to current user
        let username = username
            .or(config_username.as_deref())
            .unwrap_or(&current_user);

        tracing::info!(
            "Starting SSH authentication for {username}@{}",
            self.ssh_config.hostname
        );
        tracing::debug!("SSH config identity files: {identity_files:?}");

        let default_keys = Self::default_ssh_keys();
        tracing::debug!("Available default SSH keys: {default_keys:?}");

        // 1. attempt ssh agent auth
        tracing::info!("Step 1/4: Trying SSH agent authentication");
        if self.agent_auth(username).await? {
            tracing::info!("✓ SSH authentication successful with agent");
            return Ok(());
        }
        tracing::info!("✗ SSH agent authentication failed or unavailable");

        // 2. Try SSH config identity files
        tracing::info!(
            "Step 2/4: Trying SSH config identity files ({} files)",
            identity_files.len()
        );
        for identity_file in &identity_files {
            if let Ok(()) = self.key_auth(username, identity_file.clone()).await {
                return Ok(());
            }
        }

        // 3. Try default SSH keys if not already tried via config
        tracing::info!(
            "Step 3/4: Trying default SSH keys ({} keys found)",
            default_keys.len()
        );
        for key_path in &default_keys {
            // Skip if this key was already tried from config
            if identity_files.contains(key_path) {
                tracing::debug!(
                    "Skipping {} (already tried via SSH config)",
                    key_path.display()
                );
                continue;
            }

            match self.key_auth(username, key_path.clone()).await {
                Ok(()) => {
                    return Ok(());
                }
                Err(e) => {
                    tracing::debug!("Default SSH key failed: {e}");
                }
            }
        }

        // 4. whatever the user provided
        tracing::info!("Step 4/4: Trying explicitly provided authentication");
        match auth {
            Some(Authentication::Password(_user, password)) => {
                tracing::info!("Trying password authentication");
                self.password_auth(username, &password).await?;
                return Ok(());
            }
            Some(Authentication::Key(key_path)) => {
                tracing::info!("Trying explicitly provided key: {}", key_path.display());
                self.key_auth(username, key_path).await?;
                return Ok(());
            }
            None => {
                tracing::warn!("All SSH authentication methods exhausted");
                tracing::warn!(
                    "Tried: SSH agent, {} config keys, {} default keys",
                    identity_files.len(),
                    default_keys.len()
                );
            }
        }

        Err(eyre::eyre!("All SSH authentication methods exhausted"))
    }

    /// Authenticate with optional block-provided identity key configuration.
    /// If an identity key is provided from block settings, it is tried FIRST before other methods.
    ///
    /// Authentication order when identity_key_config is provided:
    /// 0. Block-provided identity key (FIRST - overrides everything)
    /// 1. SSH Agent authentication
    /// 2. SSH config identity files
    /// 3. Default SSH keys
    /// 4. Provided authentication method (password or key)
    pub async fn authenticate_with_config(
        &mut self,
        auth: Option<Authentication>,
        username: Option<&str>,
        identity_key_config: Option<&SshIdentityKeyConfig>,
    ) -> Result<()> {
        // Clone values we need before any mutable borrows
        let config_username = self.ssh_config.username.clone();
        let current_user = whoami::username();

        // Use provided username, or SSH config username, or default to current user
        let username = username
            .or(config_username.as_deref())
            .unwrap_or(&current_user);

        tracing::info!(
            "Starting SSH authentication for {username}@{}",
            self.ssh_config.hostname
        );

        // Step 0: Try block-provided identity key FIRST (overrides everything)
        // If an explicit key is configured and fails, we do NOT fall back to agent/defaults
        tracing::debug!("authenticate_with_config called with identity_key_config: {:?}", identity_key_config);
        if let Some(key_config) = identity_key_config {
            match key_config {
                SshIdentityKeyConfig::None => {
                    tracing::debug!("Block identity key config is SshIdentityKeyConfig::None, using defaults");
                }
                SshIdentityKeyConfig::Paste { content } => {
                    tracing::info!("Step 0: Trying block-provided pasted key");
                    match self.key_auth_from_content(username, content).await {
                        Ok(()) => {
                            tracing::info!("✓ SSH authentication successful with pasted key");
                            return Ok(());
                        }
                        Err(e) => {
                            // Explicit key was configured but failed - do not fall back
                            return Err(eyre::eyre!(
                                "Authentication failed with configured identity key: {e}"
                            ));
                        }
                    }
                }
                SshIdentityKeyConfig::Path { path } => {
                    tracing::info!("Step 0: Trying block-provided key path: {}", path);
                    match self.key_auth(username, PathBuf::from(path)).await {
                        Ok(()) => {
                            tracing::info!("✓ SSH authentication successful with key: {}", path);
                            return Ok(());
                        }
                        Err(e) => {
                            // Explicit key was configured but failed - do not fall back
                            return Err(eyre::eyre!(
                                "Authentication failed with configured identity key '{}': {e}",
                                path
                            ));
                        }
                    }
                }
            }
        }

        // Continue with normal authentication flow (only reached if no explicit key configured)
        self.authenticate(auth, Some(username)).await
    }

    /// Authenticate using a key from pasted content
    async fn key_auth_from_content(&mut self, username: &str, key_content: &str) -> Result<()> {
        tracing::debug!("Attempting authentication with pasted key content");

        let key_pair = russh::keys::decode_secret_key(key_content, None)
            .map_err(|e| eyre::eyre!("Failed to decode pasted key: {e}"))?;

        // Query the server for the best RSA hash algorithm it supports
        let best_hash = self.session.best_supported_rsa_hash().await?.flatten();
        let key_with_alg = russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key_pair), best_hash);

        let auth_res = self
            .session
            .authenticate_publickey(username, key_with_alg)
            .await?;

        match auth_res {
            russh::client::AuthResult::Success => {
                tracing::info!("✓ Pasted key authentication successful");
                Ok(())
            }
            russh::client::AuthResult::Failure {
                remaining_methods,
                partial_success,
            } => {
                tracing::warn!(
                    "Server rejected pasted key (remaining methods: {:?}, partial: {})",
                    remaining_methods,
                    partial_success
                );
                Err(eyre::eyre!("Pasted key authentication failed: server rejected key"))
            }
        }
    }

    pub async fn disconnect(&self) -> Result<()> {
        self.session
            .disconnect(Disconnect::HostNotAllowedToConnect, "", "")
            .await?;
        Ok(())
    }

    /// Determine the correct flag for passing code to the interpreter
    fn get_interpreter_flag(interpreter: &str) -> Option<&'static str> {
        let interpreter = Self::get_program_name(interpreter);

        match interpreter {
            "ruby" | "node" | "nodejs" | "perl" | "lua" => Some("-e"),
            "php" => Some("-r"),
            "bash" | "sh" | "zsh" | "fish" => Some("-c"),
            s if s.starts_with("python") => Some("-c"),
            _ => None,
        }
    }

    fn get_program_name(path: &str) -> &str {
        std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path)
    }

    fn has_flag(args: &[&str], char_flag: char) -> bool {
        args.iter().any(|arg| {
            if arg.starts_with("--") {
                false
            } else if arg.starts_with('-') {
                arg.chars().any(|c| c == char_flag)
            } else {
                false
            }
        })
    }

    /// Open a new SSH channel and execute a command
    #[allow(clippy::too_many_arguments)]
    pub async fn exec(
        &self,
        handle: SshPoolHandle,
        channel_id: String,
        output_stream: Sender<OutputLine>,
        mut cancel_rx: oneshot::Receiver<()>,
        interpreter: &str,
        command: &str,
    ) -> Result<()> {
        // For now, let's simplify this and just execute the command directly
        // without creating files on the remote
        let mut channel = self.session.channel_open_session().await?;

        // Create the actual command to execute
        // Parse interpreter string into program and args
        let parts: Vec<&str> = interpreter.split_whitespace().collect();
        let program = parts.first().unwrap_or(&interpreter);
        let args = if parts.len() > 1 { &parts[1..] } else { &[] };

        let program_name = Self::get_program_name(program);
        let mut full_command_parts = Vec::new();
        full_command_parts.push(program.to_string());

        let mut final_args: Vec<String> = args.iter().map(|s| s.to_string()).collect();

        // For shells, ensure we run as a login shell if no other login args are present
        // This ensures environment variables (like from .bash_profile) are loaded
        if ["bash", "zsh", "sh", "fish"].contains(&program_name)
            && !Self::has_flag(args, 'l')
            && !args.contains(&"--login")
        {
            final_args.insert(0, "-l".to_string());
        }

        full_command_parts.extend(final_args);

        // Add interpreter flag if not already present
        if let Some(flag) = Self::get_interpreter_flag(program) {
            // Get the char flag (e.g. 'c' from "-c")
            if let Some(char_flag) = flag.chars().last() {
                if !Self::has_flag(args, char_flag) {
                    full_command_parts.push(flag.to_string());
                }
            }
        }

        full_command_parts.push(format!("'{}'", command.replace('\'', "'\"'\"'")));

        let full_command = full_command_parts.join(" ");

        tracing::debug!("Executing command on remote: {full_command}");

        let channel_id_clone = channel_id.clone();
        let output_stream_clone = output_stream.clone();

        tokio::task::spawn(async move {
            if let Err(e) = channel.exec(true, full_command.as_str()).await {
                tracing::error!("Failed to execute command: {e}");
                let _ = output_stream_clone
                    .send(OutputLine::Stderr(e.to_string()))
                    .await;
                return;
            }

            let mut line_buffer = String::new();
            let mut stderr_line_buffer = String::new();

            loop {
                tokio::select! {
                    // Check if we've been asked to cancel
                    _ = &mut cancel_rx => {
                        tracing::debug!("SSH command execution cancelled");
                        break;
                    }

                    // Wait for channel messages
                    msg = channel.wait() => {
                        let Some(msg) = msg else {
                            break;
                        };

                        match msg {
                            ChannelMsg::Data { data } => {
                                tracing::trace!("Handling SSH Data message for stdout");
                                if let Ok(data_str) = std::str::from_utf8(&data) {
                                    line_buffer.push_str(data_str);

                                    // Process complete lines
                                    while let Some(pos) = line_buffer.find('\n') {
                                        let line = line_buffer[..pos].to_string();
                                        line_buffer = line_buffer[pos + 1..].to_string();

                                        if output_stream_clone.send(OutputLine::Stdout(line)).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                            }
                            ChannelMsg::ExtendedData { data, ext: 1 } => {
                                tracing::trace!("Handling SSH ExtendedData message for stderr");
                                // stderr
                                if let Ok(data_str) = std::str::from_utf8(&data) {
                                    stderr_line_buffer.push_str(data_str);

                                    // Process complete lines
                                    while let Some(pos) = stderr_line_buffer.find('\n') {
                                        let line = stderr_line_buffer[..pos].to_string();
                                        stderr_line_buffer = stderr_line_buffer[pos + 1..].to_string();

                                        if output_stream_clone.send(OutputLine::Stderr(line)).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                            }
                            ChannelMsg::ExitStatus { .. } => {
                                tracing::trace!("Handling SSH ExitStatus message");
                                break;
                            }
                            ChannelMsg::Eof => {
                                tracing::trace!("Handling SSH EOF message");
                                break;
                            }
                            ChannelMsg::Close => {
                                tracing::trace!("Handling SSH Close message");
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }

            // Send any remaining data
            if !line_buffer.is_empty() {
                let _ = output_stream_clone
                    .send(OutputLine::Stdout(line_buffer))
                    .await;
            }
            if !stderr_line_buffer.is_empty() {
                let _ = output_stream_clone
                    .send(OutputLine::Stderr(stderr_line_buffer))
                    .await;
            }

            tracing::debug!("Sending exec finished for channel {channel_id_clone}");
            let _ = handle.exec_finished(&channel_id_clone).await;
        });

        Ok(())
    }

    /// Open a PTY session
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    pub async fn open_pty(
        &self,
        _channel_id: String,
        width: u16,
        height: u16,
        mut resize_stream: mpsc::Receiver<(u16, u16)>,
        mut input_stream: mpsc::Receiver<Bytes>,
        output_stream: Sender<String>,
        mut cancel_rx: oneshot::Receiver<()>,
    ) -> Result<()> {
        const SSH_OPERATION_TIMEOUT: Duration = Duration::from_secs(10);

        let mut channel = timeout(SSH_OPERATION_TIMEOUT, self.session.channel_open_session())
            .await
            .map_err(|_| eyre::eyre!("Timeout opening SSH channel for PTY"))??;

        // Request PTY
        timeout(
            SSH_OPERATION_TIMEOUT,
            channel.request_pty(
                true,
                "xterm-256color",
                width as u32,
                height as u32,
                0,
                0,
                &[],
            ),
        )
        .await
        .map_err(|_| eyre::eyre!("Timeout requesting PTY"))??;

        // Start shell
        timeout(SSH_OPERATION_TIMEOUT, channel.request_shell(true))
            .await
            .map_err(|_| eyre::eyre!("Timeout starting shell"))??;

        tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    // Check if we've been asked to cancel
                    _ = &mut cancel_rx => {
                        tracing::debug!("SSH PTY session cancelled");
                        break;
                    }

                    resize = resize_stream.recv() => {
                        match resize {
                            Some((rows, cols)) => {
                                let _ = channel.window_change(cols as u32, rows as u32, 0, 0).await;
                            }
                            None => {
                                tracing::debug!("SSH resize stream closed");
                                break;
                            }
                        }
                    }

                    // Try to read from input stream
                    input_result = input_stream.recv() => {
                        match input_result {
                            Some(input) => {
                                let cursor = std::io::Cursor::new(input.as_ref());
                                if let Err(e) = channel.data(cursor).await {
                                    tracing::error!("Failed to write to channel: {e}");
                                    break;
                                }
                            }
                            None => {
                                tracing::debug!("SSH input stream closed");
                                break;
                            }
                        }
                    }

                    // Wait for channel messages
                    msg = channel.wait() => {
                        let Some(msg) = msg else {
                            break;
                        };

                        match msg {
                            ChannelMsg::Data { data } => {
                                if let Err(e) = output_stream.send(String::from_utf8_lossy(&data).to_string()).await {
                                    tracing::error!("Failed to send output to stream: {e}");
                                    break;
                                }
                            }
                            ChannelMsg::Close => {
                                tracing::debug!("SSH channel closed");
                                break;
                            }
                            ChannelMsg::Eof => {
                                tracing::debug!("SSH channel EOF");
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }

            // Clean up
            let _ = channel.eof().await;
            let _ = channel.close().await;
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_ssh_config(content: &str) -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let ssh_dir = temp_dir.path().join(".ssh");
        fs::create_dir_all(&ssh_dir).unwrap();

        let config_path = ssh_dir.join("config");
        fs::write(&config_path, content).unwrap();

        temp_dir
    }

    #[test]
    fn test_parse_host_string_host_only() {
        let (user, host, port) = Session::parse_host_string("example.com");
        assert_eq!(user, None);
        assert_eq!(host, "example.com");
        assert_eq!(port, None);
    }

    #[test]
    fn test_parse_host_string_with_port() {
        let (user, host, port) = Session::parse_host_string("example.com:2222");
        assert_eq!(user, None);
        assert_eq!(host, "example.com");
        assert_eq!(port, Some(2222));
    }

    #[test]
    fn test_parse_host_string_with_user() {
        let (user, host, port) = Session::parse_host_string("alice@example.com");
        assert_eq!(user, Some("alice".to_string()));
        assert_eq!(host, "example.com");
        assert_eq!(port, None);
    }

    #[test]
    fn test_parse_host_string_full_format() {
        let (user, host, port) = Session::parse_host_string("alice@example.com:2222");
        assert_eq!(user, Some("alice".to_string()));
        assert_eq!(host, "example.com");
        assert_eq!(port, Some(2222));
    }

    #[test]
    fn test_parse_host_string_invalid_port() {
        let (user, host, port) = Session::parse_host_string("example.com:invalid");
        assert_eq!(user, None);
        assert_eq!(host, "example.com:invalid");
        assert_eq!(port, None);
    }

    #[test]
    fn test_parse_host_string_ipv6() {
        let (user, host, port) = Session::parse_host_string("[2001:db8::1]:2222");
        assert_eq!(user, None);
        assert_eq!(host, "[2001:db8::1]");
        assert_eq!(port, Some(2222));
    }

    #[test]
    fn test_parse_identity_agent_not_found() {
        let temp_dir = create_test_ssh_config("");
        let config_path = temp_dir.path().join(".ssh").join("config");
        let result = Session::parse_identity_agent_from_path("nonexistent", &config_path);
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_identity_agent_basic() {
        let config_content = r#"
Host example.com
    IdentityAgent ~/.ssh/agent.sock
"#;
        let temp_dir = create_test_ssh_config(config_content);
        let config_path = temp_dir.path().join(".ssh").join("config");

        let home_dir = dirs::home_dir().unwrap();
        let expected = home_dir
            .join(".ssh/agent.sock")
            .to_string_lossy()
            .to_string();

        let result = Session::parse_identity_agent_from_path("example.com", &config_path);
        assert_eq!(result, Some(expected));
    }

    #[test]
    fn test_parse_identity_agent_glob_pattern() {
        let config_content = r#"
Host *.example.com
    IdentityAgent /tmp/custom-agent.sock
"#;
        let temp_dir = create_test_ssh_config(config_content);
        let config_path = temp_dir.path().join(".ssh").join("config");

        let result = Session::parse_identity_agent_from_path("server.example.com", &config_path);
        assert_eq!(result, Some("/tmp/custom-agent.sock".to_string()));
    }

    #[test]
    fn test_parse_identity_agent_wildcard() {
        let config_content = r#"
Host *
    IdentityAgent ~/.ssh/default-agent.sock
"#;
        let temp_dir = create_test_ssh_config(config_content);
        let config_path = temp_dir.path().join(".ssh").join("config");

        let home_dir = dirs::home_dir().unwrap();
        let expected = home_dir
            .join(".ssh/default-agent.sock")
            .to_string_lossy()
            .to_string();

        let result = Session::parse_identity_agent_from_path("any-host", &config_path);
        assert_eq!(result, Some(expected));
    }

    #[test]
    fn test_parse_identity_agent_no_match() {
        let config_content = r#"
Host other.com
    IdentityAgent ~/.ssh/agent.sock
"#;
        let temp_dir = create_test_ssh_config(config_content);
        let config_path = temp_dir.path().join(".ssh").join("config");

        let result = Session::parse_identity_agent_from_path("example.com", &config_path);
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_identity_agent_case_insensitive() {
        let config_content = r#"
Host example.com
    identityagent ~/.ssh/agent.sock
"#;
        let temp_dir = create_test_ssh_config(config_content);
        let config_path = temp_dir.path().join(".ssh").join("config");

        let home_dir = dirs::home_dir().unwrap();
        let expected = home_dir
            .join(".ssh/agent.sock")
            .to_string_lossy()
            .to_string();

        let result = Session::parse_identity_agent_from_path("example.com", &config_path);
        assert_eq!(result, Some(expected));
    }

    #[test]
    fn test_resolve_ssh_config_defaults() {
        // Test with a host that's unlikely to be in any real SSH config
        let config = Session::resolve_ssh_config("test-nonexistent-host-12345.invalid");
        assert_eq!(config.hostname, "test-nonexistent-host-12345.invalid");
        assert_eq!(config.port, 22);
        // Note: username might be set from global SSH config, so we don't assert None
        assert!(config.identity_files.is_empty());
        assert_eq!(config.proxy_command, None);
        assert_eq!(config.proxy_jump, None);
        assert_eq!(config.identity_agent, None);
    }

    #[test]
    fn test_resolve_ssh_config_with_input_port() {
        let config = Session::resolve_ssh_config("test-nonexistent-host-12345.invalid:2222");
        assert_eq!(config.hostname, "test-nonexistent-host-12345.invalid");
        assert_eq!(config.port, 2222);
        // Note: username might be set from global SSH config, so we don't assert None
    }

    #[test]
    fn test_resolve_ssh_config_with_input_user() {
        let config = Session::resolve_ssh_config("alice@test-nonexistent-host-12345.invalid");
        assert_eq!(config.hostname, "test-nonexistent-host-12345.invalid");
        assert_eq!(config.port, 22);
        assert_eq!(config.username, Some("alice".to_string()));
    }

    #[test]
    fn test_resolve_ssh_config_full_input() {
        let config = Session::resolve_ssh_config("alice@test-nonexistent-host-12345.invalid:2222");
        assert_eq!(config.hostname, "test-nonexistent-host-12345.invalid");
        assert_eq!(config.port, 2222);
        assert_eq!(config.username, Some("alice".to_string()));
    }

    #[test]
    fn test_default_ssh_keys() {
        let keys = Session::default_ssh_keys();

        // Keys should match ssh command's exact order
        let expected_order = vec![
            "id_rsa",
            "id_ecdsa",
            "id_ecdsa_sk",
            "id_ed25519",
            "id_ed25519_sk",
            "id_xmss",
            "id_dsa",
        ];

        for key_path in keys.iter() {
            let filename = key_path.file_name().unwrap().to_str().unwrap();
            assert!(
                expected_order.contains(&filename),
                "Unexpected key file: {filename}"
            );

            // Verify the key exists (since we filtered for existing keys)
            assert!(key_path.exists());
        }

        // Verify all returned keys are in ~/.ssh directory
        if let Some(home) = dirs::home_dir() {
            let ssh_dir = home.join(".ssh");
            for key_path in &keys {
                assert!(key_path.starts_with(&ssh_dir));
            }
        }

        // Verify order is preserved (keys should be in the same order as expected_order)
        for (i, key_path) in keys.iter().enumerate() {
            if i > 0 {
                let curr_name = key_path.file_name().unwrap().to_str().unwrap();
                let prev_name = keys[i - 1].file_name().unwrap().to_str().unwrap();

                let curr_pos = expected_order.iter().position(|&k| k == curr_name).unwrap();
                let prev_pos = expected_order.iter().position(|&k| k == prev_name).unwrap();

                assert!(
                    curr_pos > prev_pos,
                    "Keys not in ssh order: {prev_name} should come before {curr_name}"
                );
            }
        }
    }
}
