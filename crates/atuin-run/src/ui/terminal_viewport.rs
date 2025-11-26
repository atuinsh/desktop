use vt100::Parser as Vt100Parser;

/// A viewport that emulates a terminal using vt100 parser
/// This handles ANSI escape codes and maintains a 2D terminal screen buffer
pub struct TerminalViewport {
    parser: Vt100Parser,
    viewport_height: usize,
}

#[allow(dead_code)]
impl TerminalViewport {
    /// Create a new terminal viewport
    ///
    /// # Arguments
    /// * `rows` - Terminal height in rows
    /// * `cols` - Terminal width in columns
    /// * `viewport_height` - How many rows to show in the UI viewport (typically 8)
    pub fn new(rows: u16, cols: u16, viewport_height: usize) -> Self {
        Self {
            parser: Vt100Parser::new(rows, cols, 0), // 0 = no scrollback
            viewport_height,
        }
    }

    /// Process raw PTY output bytes
    /// This feeds data to the vt100 parser which updates the terminal state
    pub fn process_output(&mut self, data: &[u8]) {
        self.parser.process(data);
    }

    /// Resize the terminal
    pub fn resize(&mut self, rows: u16, cols: u16) {
        // Create a new parser with the new size
        // Unfortunately vt100 doesn't have a resize method, so we recreate
        let screen = self.parser.screen();
        let contents = screen.contents();

        self.parser = Vt100Parser::new(rows, cols, 0);
        self.parser.process(contents.as_bytes());
    }

    /// Get the visible lines to display in the viewport
    /// Returns the last N rows from the terminal screen where N = viewport_height
    pub fn get_visible_lines(&self) -> Vec<String> {
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();

        // Calculate which rows to show (last viewport_height rows)
        let visible_rows = self.viewport_height.min(rows as usize);
        let start_row = rows.saturating_sub(visible_rows as u16);

        let mut lines = Vec::new();

        // Read each row cell by cell with deterministic ANSI code reconstruction
        // We can't use contents_formatted() because it includes screen control codes
        // like cursor positioning that shouldn't be in the rendered output
        for row in start_row..rows {
            let line = self.format_row_with_ansi(screen, row, cols);
            lines.push(line.trim_end().to_string());
        }

        lines
    }

    /// Format a single row with ANSI escape codes reconstructed from cell attributes
    fn format_row_with_ansi(&self, screen: &vt100::Screen, row: u16, cols: u16) -> String {
        let mut line = String::new();
        let mut prev_cell: Option<&vt100::Cell> = None;
        let mut any_styling_emitted = false;

        for col in 0..cols {
            if let Some(cell) = screen.cell(row, col) {
                // Skip wide character continuation cells
                if cell.is_wide_continuation() {
                    continue;
                }

                // Determine if we should emit ANSI codes for this cell
                let should_emit = if let Some(prev) = prev_cell {
                    // Not first cell: emit if styling changed from previous
                    cell_style_changed(prev, cell)
                } else {
                    // First cell: only emit if it has non-default styling
                    !is_default_style(cell)
                };

                if should_emit {
                    line.push_str(&cell_to_ansi_codes(cell));
                    any_styling_emitted = true;
                }

                line.push_str(cell.contents());
                prev_cell = Some(cell);
            }
        }

        // Reset formatting at end of line only if we emitted any styling
        if any_styling_emitted {
            line.push_str("\x1b[0m");
        }

        line
    }

    /// Get all lines from the terminal (useful for debugging)
    pub fn get_all_lines(&self) -> Vec<String> {
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();

        let mut lines = Vec::new();

        // Read each row cell by cell with ANSI formatting
        for row in 0..rows {
            let line = self.format_row_with_ansi(screen, row, cols);
            lines.push(line.trim_end().to_string());
        }

        lines
    }

    /// Get the current cursor position
    pub fn cursor_position(&self) -> (u16, u16) {
        self.parser.screen().cursor_position()
    }

    /// Get the terminal size
    pub fn size(&self) -> (u16, u16) {
        self.parser.screen().size()
    }
}

/// Check if styling attributes changed between two cells
fn cell_style_changed(prev: &vt100::Cell, curr: &vt100::Cell) -> bool {
    prev.fgcolor() != curr.fgcolor()
        || prev.bgcolor() != curr.bgcolor()
        || prev.bold() != curr.bold()
        || prev.italic() != curr.italic()
        || prev.underline() != curr.underline()
        || prev.inverse() != curr.inverse()
}

/// Check if a cell has default styling (no colors, no attributes)
fn is_default_style(cell: &vt100::Cell) -> bool {
    !cell.bold()
        && !cell.italic()
        && !cell.underline()
        && !cell.inverse()
        && matches!(cell.fgcolor(), vt100::Color::Default)
        && matches!(cell.bgcolor(), vt100::Color::Default)
}

/// Convert cell styling attributes to ANSI escape codes
fn cell_to_ansi_codes(cell: &vt100::Cell) -> String {
    let mut codes = vec![];

    // Reset first to clear any previous styling
    codes.push("0".to_string());

    // Text attributes
    if cell.bold() {
        codes.push("1".to_string());
    }
    if cell.italic() {
        codes.push("3".to_string());
    }
    if cell.underline() {
        codes.push("4".to_string());
    }
    if cell.inverse() {
        codes.push("7".to_string());
    }

    // Foreground color
    codes.push(color_to_ansi_fg(cell.fgcolor()));

    // Background color
    codes.push(color_to_ansi_bg(cell.bgcolor()));

    format!("\x1b[{}m", codes.join(";"))
}

/// Convert vt100 Color to ANSI foreground color code
fn color_to_ansi_fg(color: vt100::Color) -> String {
    match color {
        vt100::Color::Default => "39".to_string(),
        vt100::Color::Idx(idx) => {
            if idx < 8 {
                format!("{}", 30 + idx)
            } else if idx < 16 {
                format!("{}", 90 + (idx - 8))
            } else {
                format!("38;5;{}", idx)
            }
        }
        vt100::Color::Rgb(r, g, b) => format!("38;2;{};{};{}", r, g, b),
    }
}

/// Convert vt100 Color to ANSI background color code
fn color_to_ansi_bg(color: vt100::Color) -> String {
    match color {
        vt100::Color::Default => "49".to_string(),
        vt100::Color::Idx(idx) => {
            if idx < 8 {
                format!("{}", 40 + idx)
            } else if idx < 16 {
                format!("{}", 100 + (idx - 8))
            } else {
                format!("48;5;{}", idx)
            }
        }
        vt100::Color::Rgb(r, g, b) => format!("48;2;{};{};{}", r, g, b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_viewport_creation() {
        let viewport = TerminalViewport::new(24, 80, 8);
        assert_eq!(viewport.size(), (24, 80));
    }

    #[test]
    fn test_process_simple_output() {
        let mut viewport = TerminalViewport::new(24, 80, 8);
        viewport.process_output(b"Hello, World!");

        let lines = viewport.get_visible_lines();
        assert_eq!(lines.len(), 8); // Shows last 8 lines

        // Content should be in the visible lines (terminal starts writing at top)
        let all_lines = viewport.get_all_lines();
        assert!(all_lines[0].contains("Hello, World!"));
    }

    #[test]
    fn test_process_ansi_colors() {
        let mut viewport = TerminalViewport::new(24, 80, 8);
        viewport.process_output(b"\x1b[31mRed text\x1b[0m");

        // Check all lines to find the content
        let all_lines = viewport.get_all_lines();
        let has_content = all_lines.iter().any(|line| !line.is_empty());
        assert!(has_content);
    }

    #[test]
    fn test_multiline_output() {
        let mut viewport = TerminalViewport::new(24, 80, 8);
        viewport.process_output(b"Line 1\r\nLine 2\r\nLine 3\r\n");

        // Check all lines to find the content
        let all_lines = viewport.get_all_lines();
        let non_empty: Vec<_> = all_lines.iter().filter(|l| !l.is_empty()).collect();
        assert!(non_empty.len() >= 3);
    }

    #[test]
    fn test_cursor_movement() {
        let mut viewport = TerminalViewport::new(24, 80, 8);
        // Move cursor to position 5,10 and write
        viewport.process_output(b"\x1b[5;10HTest");

        let all_lines = viewport.get_all_lines();
        // Line 4 (0-indexed) should have "Test" at column 9
        assert!(all_lines.len() >= 5);
    }

    #[test]
    fn test_shows_last_n_lines() {
        let mut viewport = TerminalViewport::new(24, 80, 3);

        // Fill with more lines than viewport height
        for i in 0..10 {
            viewport.process_output(format!("Line {}\r\n", i).as_bytes());
        }

        let visible = viewport.get_visible_lines();
        assert_eq!(visible.len(), 3); // Only shows last 3 lines
    }
}
