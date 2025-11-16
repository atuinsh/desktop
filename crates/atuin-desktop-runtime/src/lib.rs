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
