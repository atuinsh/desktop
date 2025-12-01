//! Runtime library for Atuin Desktop
//!
//! This crate provides the core runtime functionality for executing runbook blocks
//! in the Atuin Desktop application. It includes:
//!
//! - Block types for various operations (terminal, script, SQL, HTTP, etc.)
//! - Context management for sharing state between blocks
//! - Document handling and lifecycle management
//! - Execution context and control flow
//! - SSH connection pooling and PTY management
//! - Event emission for monitoring execution state
//!
//! # Example
//!
//! The typical flow for using this crate involves:
//! 1. Creating a `Document` from runbook data
//! 2. Building an `ExecutionContext` for block execution
//! 3. Executing blocks and managing their lifecycle
//! 4. Collecting execution results and context updates

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize the tracing subscriber for logging.
///
/// This sets up tracing to output to the terminal with the log level
/// controlled by the `RUST_LOG` environment variable.
///
/// # Examples
///
/// ```ignore
/// // Set RUST_LOG=debug before running to see debug logs
/// // Set RUST_LOG=atuin_desktop_runtime=trace for trace-level logs in this crate
/// atuin_desktop_runtime::init_tracing();
/// ```
pub fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();
}

pub mod blocks;
pub mod client;
pub mod context;
pub mod document;
pub mod events;
pub mod exec_log;
pub mod execution;
pub mod pty;
pub mod ssh;
pub mod workflow;
