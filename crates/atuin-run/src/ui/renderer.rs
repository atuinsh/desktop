//! Renderer abstraction for different output modes
//!
//! This module provides a trait that abstracts over the interactive (TUI)
//! and streaming (CI) rendering modes.

use std::io;

/// Trait for rendering block output
///
/// This is implemented by both ViewportManager (interactive TUI) and
/// StreamingRenderer (CI/non-interactive).
#[allow(dead_code)]
pub trait Renderer {
    /// Add a new block with the given title and content height
    /// Returns the block index
    fn add_block(&mut self, title: String, content_height: usize) -> io::Result<usize>;

    /// Add a terminal block (with PTY support in interactive mode)
    fn add_terminal_block(
        &mut self,
        title: String,
        content_height: usize,
        terminal_width: usize,
    ) -> io::Result<usize>;

    /// Add a line of output to a block
    fn add_line(&mut self, index: usize, line: &str) -> io::Result<()>;

    /// Replace all lines in a block (used for terminal viewport updates)
    fn replace_lines(&mut self, index: usize, lines: Vec<String>) -> io::Result<()>;

    /// Mark a block as complete
    fn mark_complete(&mut self, index: usize) -> io::Result<()>;

    /// Check if this renderer is interactive (TUI mode)
    fn is_interactive(&self) -> bool;
}
