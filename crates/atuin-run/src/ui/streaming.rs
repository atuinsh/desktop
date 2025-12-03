//! Streaming renderer for non-interactive (CI) environments
//!
//! This module provides a simple streaming output renderer that works well
//! in non-TTY environments like CI pipelines. Instead of using cursor
//! manipulation and box drawing, it simply streams logs to stdout with
//! nice formatting.

use std::io::{self, Write};

use super::renderer::Renderer;

/// A streaming renderer for non-interactive environments
///
/// This renderer simply streams output line by line, with prefixes
/// to identify which block the output is from. No cursor manipulation
/// or in-place updates are performed.
pub struct StreamingRenderer {
    stdout: io::Stdout,
    blocks: Vec<BlockInfo>,
    /// Track last lines for terminal blocks to avoid duplicate output
    last_terminal_lines: Vec<Option<Vec<String>>>,
    /// Current indentation level for nested sub-runbook output
    indent_level: usize,
}

#[derive(Clone)]
#[allow(dead_code)]
struct BlockInfo {
    number: usize,
    title: String,
    is_complete: bool,
    /// Indentation level when this block was created
    indent_level: usize,
}

#[allow(dead_code)]
impl StreamingRenderer {
    pub fn new() -> Self {
        Self {
            stdout: io::stdout(),
            blocks: Vec::new(),
            last_terminal_lines: Vec::new(),
            indent_level: 0,
        }
    }

    /// Get the indentation prefix for the current level
    fn indent_prefix(&self) -> String {
        "    ".repeat(self.indent_level)
    }

    /// Get the indentation prefix for a specific block
    fn block_indent_prefix(&self, index: usize) -> String {
        if let Some(block) = self.blocks.get(index) {
            "    ".repeat(block.indent_level)
        } else {
            String::new()
        }
    }

    /// Start a new block and print a header
    fn start_block(&mut self, title: String, is_terminal: bool) -> io::Result<usize> {
        let number = self.blocks.len() + 1;
        let indent = self.indent_prefix();
        let block = BlockInfo {
            number,
            title: title.clone(),
            is_complete: false,
            indent_level: self.indent_level,
        };

        // Print block header
        writeln!(self.stdout)?;
        writeln!(self.stdout, "{indent}━━━ [{number}] {title} ━━━")?;
        self.stdout.flush()?;

        self.blocks.push(block);
        self.last_terminal_lines
            .push(if is_terminal { Some(Vec::new()) } else { None });

        Ok(number - 1) // Return 0-indexed
    }

    /// Get the number of blocks
    pub fn len(&self) -> usize {
        self.blocks.len()
    }
}

impl Default for StreamingRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer for StreamingRenderer {
    fn add_block(&mut self, title: String, _content_height: usize) -> io::Result<usize> {
        self.start_block(title, false)
    }

    fn add_terminal_block(
        &mut self,
        title: String,
        _content_height: usize,
        _terminal_width: usize,
    ) -> io::Result<usize> {
        self.start_block(title, true)
    }

    fn add_line(&mut self, index: usize, line: &str) -> io::Result<()> {
        let indent = self.block_indent_prefix(index);
        // Print the line immediately
        writeln!(self.stdout, "{indent}  │ {}", line)?;
        self.stdout.flush()?;
        Ok(())
    }

    fn replace_lines(&mut self, index: usize, lines: Vec<String>) -> io::Result<()> {
        let indent = self.block_indent_prefix(index);
        // For terminal blocks, we get the full viewport each time
        // We need to diff against last output to only print new content
        if let Some(Some(last_lines)) = self.last_terminal_lines.get_mut(index) {
            // Find new lines that weren't in the previous output
            // Simple approach: if the new lines end differs from old, print the diff
            let last_len = last_lines.len();

            // Check if content has actually changed
            if lines != *last_lines {
                // Print only lines that are different or new
                for (i, line) in lines.iter().enumerate() {
                    let is_new = i >= last_len || last_lines.get(i) != Some(line);
                    if is_new && !line.trim().is_empty() {
                        writeln!(self.stdout, "{indent}  │ {}", line)?;
                    }
                }
                self.stdout.flush()?;
            }

            *last_lines = lines;
        }

        Ok(())
    }

    fn mark_complete(&mut self, index: usize) -> io::Result<()> {
        if let Some(block) = self.blocks.get_mut(index) {
            let indent = "    ".repeat(block.indent_level);
            block.is_complete = true;
            writeln!(
                self.stdout,
                "{indent}  ✓ [{}] {} complete",
                index + 1,
                block.title
            )?;
            writeln!(self.stdout)?;
            self.stdout.flush()?;
        }
        Ok(())
    }

    fn is_interactive(&self) -> bool {
        false
    }

    fn set_indent_level(&mut self, level: usize) {
        self.indent_level = level;
    }

    fn indent_level(&self) -> usize {
        self.indent_level
    }
}
