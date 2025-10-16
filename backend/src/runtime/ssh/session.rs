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

use crate::runtime::ssh_pool::SshPoolHandle;

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
    /// Send a keepalive to test if the SSH connection is still active and responsive
    /// Uses a lightweight exec command that actually tests network connectivity
    pub async fn send_keepalive(&self) -> bool {
        const KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(5);

        let keepalive_check = async {
            let mut channel = self.session.channel_open_session().await.ok()?;
            channel.exec(true, "true").await.ok()?;

            let mut success = false;
            while let Some(msg) = channel.wait().await {
                match msg {
                    ChannelMsg::ExitStatus { .. } => {
                        success = true;
                        break;
                    }
                    ChannelMsg::Close => break,
                    ChannelMsg::Eof => break,
                    _ => continue,
                }
            }

            let _ = channel.close().await;
            Some(success)
        };

        match timeout(KEEPALIVE_TIMEOUT, keepalive_check).await {
            Ok(Some(success)) => success,
            Ok(None) => false,
            Err(_) => {
                log::debug!("SSH keepalive timed out");
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
    fn resolve_ssh_config(host: &str) -> SshConfig {
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

                    log::debug!("Resolved SSH config for {host}: hostname={hostname}, port={port}, username={username:?}, identity_files={identity_files:?}, proxy_command={proxy_command:?}, proxy_jump={proxy_jump:?}");

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
                    log::warn!("Failed to parse SSH config: {e}");
                }
            }
        }

        log::debug!("No SSH config found for {host}, using defaults");
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
            log::debug!(
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
                    log::warn!("Failed to create proxy stream: {e}");
                    // Fallback to direct connection
                    let address = format!("{}:{}", ssh_config.hostname, ssh_config.port);
                    log::debug!("Falling back to direct connection: {address}");
                    russh::client::connect(Arc::new(config), address.as_str(), sh).await?
                }
            }
        } else {
            // Direct connection
            let address = format!("{}:{}", ssh_config.hostname, ssh_config.port);
            log::debug!("Connecting directly to: {address}");
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

    /// Public key authentication
    pub async fn key_auth(&mut self, username: &str, key_path: PathBuf) -> Result<()> {
        let key_pair = russh::keys::load_secret_key(&key_path, None)?;

        let auth_res = self
            .session
            .authenticate_publickey(
                username,
                russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key_pair), None),
            )
            .await?;

        if !matches!(auth_res, russh::client::AuthResult::Success) {
            return Err(eyre::eyre!("Public key authentication failed"));
        }

        Ok(())
    }

    pub async fn agent_auth(&mut self, username: &str) -> Result<bool> {
        // Try to connect to SSH agent, using custom IdentityAgent if specified
        let agent_result = if let Some(ref identity_agent) = self.ssh_config.identity_agent {
            log::debug!("Using custom IdentityAgent: {identity_agent}");
            // Connect to custom agent socket
            russh::keys::agent::client::AgentClient::connect_uds(identity_agent)
                .await
                .map_err(|_| russh::Error::NotAuthenticated)
        } else {
            // Use default SSH agent from environment
            russh::keys::agent::client::AgentClient::connect_env()
                .await
                .map_err(|_| russh::Error::NotAuthenticated)
        };

        match agent_result {
            Ok(mut agent) => match agent.request_identities().await {
                Ok(keys) => {
                    log::debug!("Found {} keys in SSH agent", keys.len());
                    for key in keys {
                        match self
                            .session
                            .authenticate_publickey_with(username, key.clone(), None, &mut agent)
                            .await
                        {
                            Ok(russh::client::AuthResult::Success) => {
                                log::debug!("Successfully authenticated with SSH agent key");
                                return Ok(true);
                            }
                            Ok(_) => {
                                log::debug!("SSH agent key rejected by server");
                                continue;
                            }
                            Err(e) => {
                                log::debug!("Error trying SSH agent key: {e:?}");
                                continue;
                            }
                        }
                    }
                    log::debug!("No SSH agent keys worked for authentication");
                    Ok(false)
                }
                Err(e) => {
                    log::debug!("Failed to request identities from SSH agent: {e}");
                    Ok(false)
                }
            },
            Err(e) => {
                log::debug!("Failed to connect to SSH agent: {e}");
                Ok(false)
            }
        }
    }

    /// Authenticate the session. If a username is provided, use it for authentication - otherwise we will use SSH config or "root"
    ///
    /// The authentication order is:
    /// 1. SSH Agent authentication
    /// 2. SSH config identity files
    /// 3. Provided authentication method (password or key)
    pub async fn authenticate(
        &mut self,
        auth: Option<Authentication>,
        username: Option<&str>,
    ) -> Result<()> {
        // Clone values we need before any mutable borrows
        let config_username = self.ssh_config.username.clone();
        let identity_files = self.ssh_config.identity_files.clone();

        // Use provided username, or SSH config username, or default to "root"
        let username = username.or(config_username.as_deref()).unwrap_or("root");

        log::debug!("SSH authentication as {username} (config username: {config_username:?})");

        // 1. attempt ssh agent auth
        if self.agent_auth(username).await? {
            log::debug!("SSH authentication successful with agent");
            return Ok(());
        }

        // 2. Try SSH config identity files
        for identity_file in &identity_files {
            log::debug!("Trying SSH config identity file: {identity_file:?}");
            if let Ok(()) = self.key_auth(username, identity_file.clone()).await {
                log::debug!(
                    "SSH authentication successful with config identity file: {identity_file:?}"
                );
                return Ok(());
            }
        }

        // 3. whatever the user provided
        match auth {
            Some(Authentication::Password(_user, password)) => {
                self.password_auth(username, &password).await?
            }
            Some(Authentication::Key(key_path)) => self.key_auth(username, key_path).await?,
            None => {
                // If no explicit auth method is provided and previous methods failed,
                // log the attempt but don't fail
                log::debug!("No explicit authentication method provided, tried SSH agent and config identity files");
            }
        }

        Ok(())
    }

    pub async fn disconnect(&self) -> Result<()> {
        self.session
            .disconnect(Disconnect::HostNotAllowedToConnect, "", "")
            .await?;
        Ok(())
    }

    /// Open a new SSH channel and execute a command
    #[allow(clippy::too_many_arguments)]
    pub async fn exec(
        &self,
        handle: SshPoolHandle,
        channel_id: String,
        output_stream: Sender<String>,
        mut cancel_rx: oneshot::Receiver<()>,
        interpreter: &str,
        command: &str,
    ) -> Result<()> {
        // For now, let's simplify this and just execute the command directly
        // without creating files on the remote
        let mut channel = self.session.channel_open_session().await?;

        // Create the actual command to execute
        let full_command = format!("{} -c '{}'", interpreter, command.replace('\'', "'\"'\"'"));

        log::debug!("Executing command on remote: {full_command}");

        let channel_id_clone = channel_id.clone();
        let output_stream_clone = output_stream.clone();

        tokio::task::spawn(async move {
            if let Err(e) = channel.exec(true, full_command.as_str()).await {
                log::error!("Failed to execute command: {e}");
                let _ = output_stream_clone.send(e.to_string()).await;
                return;
            }

            let mut line_buffer = String::new();
            let mut stderr_line_buffer = String::new();

            loop {
                tokio::select! {
                    // Check if we've been asked to cancel
                    _ = &mut cancel_rx => {
                        log::debug!("SSH command execution cancelled");
                        break;
                    }

                    // Wait for channel messages
                    msg = channel.wait() => {
                        let Some(msg) = msg else {
                            break;
                        };

                        match msg {
                            ChannelMsg::Data { data } => {
                                if let Ok(data_str) = std::str::from_utf8(&data) {
                                    line_buffer.push_str(data_str);

                                    // Process complete lines
                                    while let Some(pos) = line_buffer.find('\n') {
                                        let line = line_buffer[..pos].to_string();
                                        line_buffer = line_buffer[pos + 1..].to_string();

                                        if output_stream_clone.send(line).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                            }
                            ChannelMsg::ExtendedData { data, ext: 1 } => {
                                // stderr
                                if let Ok(data_str) = std::str::from_utf8(&data) {
                                    stderr_line_buffer.push_str(data_str);

                                    // Process complete lines
                                    while let Some(pos) = stderr_line_buffer.find('\n') {
                                        let line = stderr_line_buffer[..pos].to_string();
                                        stderr_line_buffer = stderr_line_buffer[pos + 1..].to_string();

                                        if output_stream_clone.send(line).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                            }
                            ChannelMsg::ExitStatus { .. } => {
                                // Send any remaining data
                                if !line_buffer.is_empty() {
                                    let _ = output_stream_clone.send(line_buffer).await;
                                }
                                if !stderr_line_buffer.is_empty() {
                                    let _ = output_stream_clone.send(stderr_line_buffer).await;
                                }
                                break;
                            }
                            ChannelMsg::Eof => {
                                break;
                            }
                            ChannelMsg::Close => {
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }

            log::debug!("Sending exec finished for channel {channel_id_clone}");
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
                        log::debug!("SSH PTY session cancelled");
                        break;
                    }

                    resize = resize_stream.recv() => {
                        match resize {
                            Some((width, height)) => {
                                let _ = channel.window_change(width as u32, height as u32, 0, 0).await;
                            }
                            None => {
                                log::debug!("SSH resize stream closed");
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
                                    log::error!("Failed to write to channel: {e}");
                                    break;
                                }
                            }
                            None => {
                                log::debug!("SSH input stream closed");
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
                                    log::error!("Failed to send output to stream: {e}");
                                    break;
                                }
                            }
                            ChannelMsg::Close => {
                                log::debug!("SSH channel closed");
                                break;
                            }
                            ChannelMsg::Eof => {
                                log::debug!("SSH channel EOF");
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
}
