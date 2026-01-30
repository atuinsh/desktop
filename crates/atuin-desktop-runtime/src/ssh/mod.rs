//! SSH connection pooling and management
//!
//! This module provides connection pooling for SSH sessions, allowing blocks
//! to reuse connections and execute commands on remote hosts.
//!
//! Features:
//! - Connection pooling with automatic cleanup
//! - SSH configuration file parsing
//! - Multiple authentication methods (keys and certificates)
//! - Remote PTY support
//!
//! ## Certificate Support
//!
//! SSH certificate authentication is supported for file-based certificates.
//! If a key file (e.g., `~/.ssh/id_ed25519`) has a companion certificate file
//! (e.g., `~/.ssh/id_ed25519-cert.pub`), the certificate will be used for authentication.
//!
//! **Known limitation:** SSH certificates loaded in an SSH agent are not currently
//! supported due to limitations in the russh library. Users relying on agent-based
//! certificate authentication should ensure the private key and certificate files
//! are available on disk.

mod pool;
mod session;
mod ssh_env;
mod ssh_pool;

#[cfg(test)]
mod integration_tests;

pub use pool::Pool;
pub use session::{Authentication, CommandResult, OutputLine, Session, SshConfig, SshWarning};
pub use ssh_env::build_env_exports;
pub use ssh_pool::{SshPoolHandle, SshPty};
