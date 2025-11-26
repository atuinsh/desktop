use crossterm::{
    style::Print,
    terminal::{self},
    QueueableCommand,
};
use std::io::{self, Write};
use textwrap::{wrap, Options};

/// Calculate visual width of a string, ignoring ANSI escape codes
fn visual_width(s: &str) -> usize {
    let mut width = 0;
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Skip ANSI escape sequence
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                              // Skip until we hit a letter (the command character)
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            // Count visible character
            width += 1;
        }
    }

    width
}

/// Box drawing characters
pub(super) const TOP_LEFT: char = '┌';
pub(super) const TOP_RIGHT: char = '┐';
pub(super) const BOTTOM_LEFT: char = '└';
pub(super) const BOTTOM_RIGHT: char = '┘';
pub(super) const HORIZONTAL: char = '─';
pub(super) const VERTICAL: char = '│';

/// A viewport for displaying a block's execution
pub struct BlockViewport {
    /// Block number (1-indexed for display)
    pub number: usize,
    /// Block title/name
    pub title: String,
    /// Whether this block is currently executing
    pub is_active: bool,
    /// Height of the content area (in lines)
    pub content_height: usize,
    /// Lines to display in the content area
    pub lines: Vec<String>,
    /// Width of the terminal (used for testing)
    pub terminal_width: Option<usize>,
}

#[allow(dead_code)]
impl BlockViewport {
    pub fn new(number: usize, title: String, content_height: usize) -> Self {
        Self {
            number,
            title,
            is_active: false,
            content_height,
            lines: Vec::new(),
            terminal_width: None,
        }
    }

    pub fn set_terminal_width(&mut self, width: usize) {
        self.terminal_width = Some(width);
    }

    /// Add a line to the viewport (will scroll if needed)
    /// Long lines will be automatically wrapped at word boundaries
    pub fn add_line(&mut self, line: String) {
        // Calculate content width (same as in render)
        let term_width = self
            .terminal_width
            .unwrap_or_else(|| terminal::size().map(|s| s.0 as usize).unwrap_or(80));
        let content_width = term_width.saturating_sub(8); // Account for padding and borders

        // Wrap the line if it's too long
        let options = Options::new(content_width).break_words(false); // Don't break words mid-word

        let wrapped_lines = wrap(&line, options);

        // Add each wrapped segment as a separate line
        for (i, wrapped_line) in wrapped_lines.iter().enumerate() {
            let display_line = if i == 0 {
                // First line: use as-is
                wrapped_line.to_string()
            } else {
                // Continuation lines: indent slightly
                format!("  {}", wrapped_line)
            };

            self.lines.push(display_line);
        }

        // If nothing was wrapped (empty line), still add it
        if wrapped_lines.is_empty() {
            self.lines.push(String::new());
        }

        // Keep only the last N lines where N = content_height
        while self.lines.len() > self.content_height {
            self.lines.remove(0);
        }
    }

    /// Render the viewport to stdout
    pub fn render(&self, stdout: &mut io::Stdout) -> io::Result<()> {
        self.render_internal(stdout, true)
    }

    /// Render the viewport without flushing (for use in replace_lines)
    pub fn render_no_flush(&self, stdout: &mut io::Stdout) -> io::Result<()> {
        self.render_internal(stdout, false)
    }

    fn render_internal(&self, stdout: &mut io::Stdout, should_flush: bool) -> io::Result<()> {
        // Use terminal_width if set (for terminal blocks with PTY), otherwise use actual terminal size
        let term_width = self
            .terminal_width
            .unwrap_or_else(|| terminal::size().map(|(w, _)| w as usize).unwrap_or(80));

        // Account for padding (2 spaces on each side + outer border)
        let inner_width = term_width.saturating_sub(6);
        let content_width = inner_width.saturating_sub(2);

        // Render title line
        // Use \r\n instead of \n to ensure proper line breaks in raw mode
        let prefix = if self.is_active { " >  " } else { "    " };
        stdout.queue(Print(format!(
            "{}{}. {}\r\n",
            prefix, self.number, self.title
        )))?;

        // Render top border of content box
        stdout.queue(Print("   "))?;
        stdout.queue(Print(TOP_LEFT))?;
        stdout.queue(Print(HORIZONTAL.to_string().repeat(content_width)))?;
        stdout.queue(Print(TOP_RIGHT))?;
        stdout.queue(Print("\r\n"))?;

        // Render content lines
        for i in 0..self.content_height {
            stdout.queue(Print("   "))?;
            stdout.queue(Print(VERTICAL))?;

            if let Some(line) = self.lines.get(i) {
                // Calculate visual width (ignoring ANSI codes)
                let vis_width = visual_width(line);

                if vis_width > content_width {
                    // Line is too long, truncate it
                    // This is complex with ANSI codes, so just print as-is and let it wrap
                    stdout.queue(Print(line))?;
                    stdout.queue(Print(
                        " ".repeat(content_width.saturating_sub(vis_width.min(content_width))),
                    ))?;
                } else {
                    // Line fits, print it and pad with spaces
                    stdout.queue(Print(line))?;
                    stdout.queue(Print(" ".repeat(content_width - vis_width)))?;
                }
            } else {
                // Empty line
                stdout.queue(Print(" ".repeat(content_width)))?;
            }

            stdout.queue(Print(VERTICAL))?;
            stdout.queue(Print("\r\n"))?;
        }

        // Render bottom border
        stdout.queue(Print("   "))?;
        stdout.queue(Print(BOTTOM_LEFT))?;
        stdout.queue(Print(HORIZONTAL.to_string().repeat(content_width)))?;
        stdout.queue(Print(BOTTOM_RIGHT))?;
        stdout.queue(Print("\r\n"))?;

        // Empty line after block
        stdout.queue(Print("\r\n"))?;

        if should_flush {
            stdout.flush()?;
        }
        Ok(())
    }

    /// Clear the viewport area and re-render
    /// Note: This is a simplified version that just re-prints.
    /// For true in-place updates, you'd need to track cursor positions.
    pub fn update(&self, stdout: &mut io::Stdout) -> io::Result<()> {
        self.render(stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_viewport_creation() {
        let viewport = BlockViewport::new(1, "Test Block".to_string(), 8);
        assert_eq!(viewport.number, 1);
        assert_eq!(viewport.title, "Test Block");
        assert_eq!(viewport.content_height, 8);
        assert_eq!(viewport.lines.len(), 0);
    }

    #[test]
    fn test_add_line_scrolling() {
        let mut viewport = BlockViewport::new(1, "Test".to_string(), 3);

        viewport.add_line("Line 1".to_string());
        viewport.add_line("Line 2".to_string());
        viewport.add_line("Line 3".to_string());
        // Note: Each add_line may add multiple physical lines if wrapping occurs
        // For short lines, it should still be 3 lines
        assert!(viewport.lines.len() >= 3);

        // Adding a 4th line should remove the oldest
        viewport.add_line("Line 4".to_string());
        assert_eq!(viewport.lines.len(), 3);
        assert_eq!(viewport.lines[2], "Line 4");
    }

    #[test]
    fn test_line_wrapping() {
        let mut viewport = BlockViewport::new(1, "Test".to_string(), 10);

        // Add a very long line that should wrap
        let long_line = "This is a very long line that should definitely wrap across multiple lines in the viewport";
        viewport.add_line(long_line.to_string());

        // Should have added multiple lines (at least 2)
        assert!(viewport.lines.len() > 1);

        // First line should be the start
        assert!(viewport.lines[0].starts_with("This is"));

        // Continuation lines should be indented
        if viewport.lines.len() > 1 {
            assert!(viewport.lines[1].starts_with("  "));
        }
    }
}
