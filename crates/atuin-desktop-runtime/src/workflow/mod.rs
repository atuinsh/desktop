//! Workflow execution and coordination
//!
//! This module provides workflow execution capabilities including:
//! - Serial execution of blocks in order
//! - Dependency-based execution ordering
//! - Workflow event broadcasting
//! - Execution orchestration

mod dependency;
mod event;
mod executor;
mod serial;

pub use dependency::DependencySpec;
pub use event::{WorkflowCommand, WorkflowEvent};
pub use executor::ExecutorHandle;
pub use serial::serial_execute;
