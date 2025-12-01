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

/// Initialize the tracing subscriber for standalone logging to the terminal.
///
/// This sets up tracing to output directly to stderr with the log level
/// controlled by the `RUST_LOG` environment variable.
///
/// Use this for standalone applications or CLI tools that don't have an
/// existing logging setup.
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

/// Tracing events are automatically forwarded to the `log` crate.
///
/// This crate has the `log` feature enabled on `tracing`, which means all
/// `tracing` events are automatically emitted as `log` records when no
/// tracing subscriber is set up.
///
/// # How it works
///
/// The `tracing` crate's `log` feature provides automatic compatibility:
///
/// ```text
/// ┌─────────────────────────────────────────────────────────────┐
/// │  atuin-desktop-runtime                                      │
/// │  ┌─────────────────────┐                                    │
/// │  │ tracing::info!(..); │                                    │
/// │  └──────────┬──────────┘                                    │
/// │             │                                               │
/// │             ▼                                               │
/// │  ┌─────────────────────┐                                    │
/// │  │  tracing `log`      │  When no subscriber is set,        │
/// │  │  feature            │  events emit as log records        │
/// │  └──────────┬──────────┘                                    │
/// │             │                                               │
/// └─────────────┼───────────────────────────────────────────────┘
///               │
///               ▼
/// ┌─────────────────────────────────────────────────────────────┐
/// │  Application (e.g., Tauri backend)                          │
/// │  ┌─────────────────────┐                                    │
/// │  │  log crate facade   │                                    │
/// │  └──────────┬──────────┘                                    │
/// │             │                                               │
/// │             ▼                                               │
/// │  ┌─────────────────────┐                                    │
/// │  │  tauri-plugin-log   │  Handles log output to file,       │
/// │  │  (or other logger)  │  console, system log, etc.         │
/// │  └─────────────────────┘                                    │
/// └─────────────────────────────────────────────────────────────┘
/// ```
///
/// Level mapping:
/// - `tracing::trace!` → `log::trace!`
/// - `tracing::debug!` → `log::debug!`
/// - `tracing::info!`  → `log::info!`
/// - `tracing::warn!`  → `log::warn!`
/// - `tracing::error!` → `log::error!`
///
/// # Usage with tauri-plugin-log
///
/// If your application uses `tauri-plugin-log`, you don't need to call any
/// initialization function. Just set up `tauri-plugin-log` as usual, and
/// tracing events from this crate will automatically appear in your logs.
///
/// If you want to use tracing's native output (e.g., for a CLI tool),
/// call [`init_tracing()`] instead.
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
