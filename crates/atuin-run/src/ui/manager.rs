use crossterm::{
    cursor::{MoveDown, MoveToColumn, MoveUp},
    terminal::{Clear, ClearType},
    QueueableCommand,
};
use std::io;

use super::renderer::Renderer;
use super::viewport::BlockViewport;

/// Manager for coordinating multiple block viewports
#[allow(dead_code)]
pub struct ViewportManager {
    viewports: Vec<BlockViewport>,
    viewport_positions: Vec<u16>,   // Relative line offsets from start
    stdout: io::Stdout,             //
    current_line: u16,              // Track our current line position
    active_viewport: Option<usize>, // Index of currently active viewport
    is_terminal: Vec<bool>,         // Track which viewports are terminal viewports
}

#[allow(dead_code)]
impl ViewportManager {
    pub fn new() -> Self {
        Self {
            viewports: Vec::new(),
            viewport_positions: Vec::new(),
            stdout: io::stdout(),
            current_line: 0,
            active_viewport: None,
            is_terminal: Vec::new(),
        }
    }

    /// Add a new block viewport with the given title and content height
    /// This becomes the new active viewport
    pub fn add_block(&mut self, title: String, content_height: usize) -> io::Result<usize> {
        self.add_block_internal(title, content_height, false, None)
    }

    /// Add a new terminal block viewport
    /// Terminal viewports replace all lines on update instead of appending
    pub fn add_terminal_block(
        &mut self,
        title: String,
        content_height: usize,
        terminal_width: usize,
    ) -> io::Result<usize> {
        self.add_block_internal(title, content_height, true, Some(terminal_width))
    }

    fn add_block_internal(
        &mut self,
        title: String,
        content_height: usize,
        is_terminal: bool,
        terminal_width: Option<usize>,
    ) -> io::Result<usize> {
        // Auto-number the viewport (1-indexed)
        let number = self.viewports.len() + 1;

        // Create the viewport
        let mut viewport = BlockViewport::new(number, title, content_height);
        viewport.is_active = true;
        viewport.terminal_width = terminal_width;

        // Store the relative position for this viewport (offset from start)
        let viewport_offset = self.current_line;

        // Render the viewport
        viewport.render(&mut self.stdout)?;

        // Calculate how many lines this viewport takes up
        // Title line + top border + content_height + bottom border + empty line
        let viewport_height = 1 + 1 + viewport.content_height + 1 + 1;
        self.current_line += viewport_height as u16;

        // Store the viewport and its relative position
        self.viewport_positions.push(viewport_offset);
        self.viewports.push(viewport);
        self.is_terminal.push(is_terminal);

        let index = self.viewports.len() - 1;
        self.active_viewport = Some(index);

        Ok(index)
    }

    /// Add a line to a specific viewport by index
    pub fn add_line(&mut self, index: usize, line: &str) -> io::Result<()> {
        self.update_viewport(index, line.to_string())
    }

    /// Add a line to the currently active viewport
    pub fn add_line_to_current(&mut self, line: &str) -> io::Result<()> {
        if let Some(index) = self.active_viewport {
            self.update_viewport(index, line.to_string())
        } else {
            Ok(())
        }
    }

    /// Replace all lines in a viewport (for terminal viewports)
    /// This clears the viewport and sets new content
    pub fn replace_lines(&mut self, index: usize, lines: Vec<String>) -> io::Result<()> {
        let viewport_offset = if let Some(&pos) = self.viewport_positions.get(index) {
            pos
        } else {
            return Ok(());
        };

        let viewport_height = if let Some(vp) = self.viewports.get(index) {
            (1 + 1 + vp.content_height + 1 + 1) as u16
        } else {
            return Ok(());
        };

        // Now we can mutably borrow and update
        if let Some(viewport) = self.viewports.get_mut(index) {
            // Replace all lines
            viewport.lines.clear();
            for line in lines {
                viewport.lines.push(line);
            }

            // Calculate how far up we need to move from the end
            let lines_from_end = self.current_line - viewport_offset;

            // Move up to the viewport start
            if lines_from_end > 0 {
                self.stdout.queue(MoveUp(lines_from_end))?;
            }
            self.stdout.queue(MoveToColumn(0))?;

            // Clear and re-render the viewport
            for i in 0..viewport_height {
                self.stdout.queue(Clear(ClearType::CurrentLine))?;
                if i < viewport_height - 1 {
                    self.stdout.queue(MoveDown(1))?;
                    self.stdout.queue(MoveToColumn(0))?;
                }
            }

            // Move back to viewport start
            if viewport_height > 1 {
                self.stdout.queue(MoveUp(viewport_height - 1))?;
            }
            self.stdout.queue(MoveToColumn(0))?;

            // Render the viewport
            viewport.render_no_flush(&mut self.stdout)?;

            // Move back down to the end position
            if lines_from_end > 0 {
                self.stdout.queue(MoveDown(lines_from_end))?;
            }
            self.stdout.queue(MoveToColumn(0))?;

            // Flush all queued commands at once for atomic update
            use std::io::Write;
            self.stdout.flush()?;
        }

        Ok(())
    }

    /// Update a specific viewport by index
    pub fn update_viewport(&mut self, index: usize, new_line: String) -> io::Result<()> {
        let viewport_offset = if let Some(&pos) = self.viewport_positions.get(index) {
            pos
        } else {
            return Ok(());
        };

        let viewport_height = if let Some(vp) = self.viewports.get(index) {
            (1 + 1 + vp.content_height + 1 + 1) as u16
        } else {
            return Ok(());
        };

        // Now we can mutably borrow and update
        if let Some(viewport) = self.viewports.get_mut(index) {
            viewport.add_line(new_line);

            // Calculate how far up we need to move from the end
            let lines_from_end = self.current_line - viewport_offset;

            // Move up to the viewport start
            if lines_from_end > 0 {
                self.stdout.queue(MoveUp(lines_from_end))?;
            }
            self.stdout.queue(MoveToColumn(0))?;

            // Clear and re-render the viewport
            for i in 0..viewport_height {
                self.stdout.queue(Clear(ClearType::CurrentLine))?;
                if i < viewport_height - 1 {
                    self.stdout.queue(MoveDown(1))?;
                    self.stdout.queue(MoveToColumn(0))?;
                }
            }

            // Move back to viewport start
            if viewport_height > 1 {
                self.stdout.queue(MoveUp(viewport_height - 1))?;
            }
            self.stdout.queue(MoveToColumn(0))?;

            // Render the viewport
            viewport.render_no_flush(&mut self.stdout)?;

            // Move back down to the end position
            if lines_from_end > 0 {
                self.stdout.queue(MoveDown(lines_from_end))?;
            }
            self.stdout.queue(MoveToColumn(0))?;

            // Flush all queued commands at once for atomic update
            use std::io::Write;
            self.stdout.flush()?;
        }

        Ok(())
    }

    /// Calculate how many lines from the current end position to a viewport's start
    fn calculate_lines_from_end(&self, index: usize) -> u16 {
        let mut lines = 0;

        // Add up heights of all viewports after this index
        for i in (index + 1)..self.viewports.len() {
            if let Some(vp) = self.viewports.get(i) {
                lines += 1 + 1 + vp.content_height + 1 + 1; // title + top + content + bottom + empty
            }
        }

        lines as u16
    }

    /// Get a reference to a viewport
    pub fn get_viewport(&self, index: usize) -> Option<&BlockViewport> {
        self.viewports.get(index)
    }

    /// Get a mutable reference to a viewport
    pub fn get_viewport_mut(&mut self, index: usize) -> Option<&mut BlockViewport> {
        self.viewports.get_mut(index)
    }

    /// Mark a viewport as complete (inactive) and re-render it
    pub fn mark_complete(&mut self, index: usize) -> io::Result<()> {
        let viewport_offset = if let Some(&pos) = self.viewport_positions.get(index) {
            pos
        } else {
            return Ok(());
        };

        let viewport_height = if let Some(vp) = self.viewports.get(index) {
            (1 + 1 + vp.content_height + 1 + 1) as u16
        } else {
            return Ok(());
        };

        // Now we can mutably borrow and update
        if let Some(viewport) = self.viewports.get_mut(index) {
            viewport.is_active = false;

            // Calculate how far up we need to move from the end
            let lines_from_end = self.current_line - viewport_offset;

            // Move up to the viewport start
            if lines_from_end > 0 {
                self.stdout.queue(MoveUp(lines_from_end))?;
            }
            self.stdout.queue(MoveToColumn(0))?;

            // Clear and re-render the viewport
            for i in 0..viewport_height {
                self.stdout.queue(Clear(ClearType::CurrentLine))?;
                if i < viewport_height - 1 {
                    self.stdout.queue(MoveDown(1))?;
                    self.stdout.queue(MoveToColumn(0))?;
                }
            }

            // Move back to viewport start
            if viewport_height > 1 {
                self.stdout.queue(MoveUp(viewport_height - 1))?;
            }
            self.stdout.queue(MoveToColumn(0))?;

            // Render the viewport
            viewport.render_no_flush(&mut self.stdout)?;

            // Move back down to the end position
            if lines_from_end > 0 {
                self.stdout.queue(MoveDown(lines_from_end))?;
            }
            self.stdout.queue(MoveToColumn(0))?;

            // Flush all queued commands at once for atomic update
            use std::io::Write;
            self.stdout.flush()?;
        }

        // Clear active viewport if this was it
        if self.active_viewport == Some(index) {
            self.active_viewport = None;
        }

        Ok(())
    }

    /// Mark the currently active viewport as complete
    pub fn mark_current_complete(&mut self) -> io::Result<()> {
        if let Some(index) = self.active_viewport {
            self.mark_complete(index)?;
        }
        Ok(())
    }

    /// Get the number of viewports
    pub fn len(&self) -> usize {
        self.viewports.len()
    }
}

impl Renderer for ViewportManager {
    fn add_block(&mut self, title: String, content_height: usize) -> io::Result<usize> {
        ViewportManager::add_block(self, title, content_height)
    }

    fn add_terminal_block(
        &mut self,
        title: String,
        content_height: usize,
        terminal_width: usize,
    ) -> io::Result<usize> {
        ViewportManager::add_terminal_block(self, title, content_height, terminal_width)
    }

    fn add_line(&mut self, index: usize, line: &str) -> io::Result<()> {
        ViewportManager::add_line(self, index, line)
    }

    fn replace_lines(&mut self, index: usize, lines: Vec<String>) -> io::Result<()> {
        ViewportManager::replace_lines(self, index, lines)
    }

    fn mark_complete(&mut self, index: usize) -> io::Result<()> {
        ViewportManager::mark_complete(self, index)
    }

    fn is_interactive(&self) -> bool {
        true
    }
}
