//! SSH connection pooling and management
//!
//! This module provides connection pooling for SSH sessions, allowing blocks
//! to reuse connections and execute commands on remote hosts.
//!
//! Features:
//! - Connection pooling with automatic cleanup
//! - SSH configuration file parsing
//! - Multiple authentication methods
//! - Remote PTY support

mod pool;
mod session;
mod ssh_pool;

pub use pool::Pool;
pub use session::{Authentication, OutputLine, Session, SshConfig};
pub use ssh_pool::{SshPoolHandle, SshPty};
