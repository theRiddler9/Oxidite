//! Terminal state: modes, alternate screen, `Handler` implementation (stub — Phase 3).

use crate::grid::Grid;
use crate::handler::{EraseMode, Handler, Mode, SgrAttr};

/// Runtime terminal modes.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct Modes {
    /// Automatic wrap at the right margin (DECAWM).
    pub autowrap: bool,
    /// Cursor visibility (DECTCEM).
    pub cursor_visible: bool,
    /// Application cursor keys mode (DECCKM).
    pub application_cursor_keys: bool,
    /// Bracketed paste mode.
    pub bracketed_paste: bool,
    /// Mouse reporting mode (SGR 1006).
    pub mouse_reporting: bool,
    /// Alternate screen buffer active.
    pub alternate_screen: bool,
}

impl Default for Modes {
    /// Defaults match a typical xterm: autowrap on, cursor visible.
    fn default() -> Self {
        Self {
            autowrap: true,
            cursor_visible: true,
            application_cursor_keys: false,
            bracketed_paste: false,
            mouse_reporting: false,
            alternate_screen: false,
        }
    }
}

/// Full terminal state owned by the app (stub).
#[derive(Debug, Default)]
pub struct Terminal {
    /// Screen grid storage.
    pub grid: Grid,
    /// Runtime terminal modes.
    pub modes: Modes,
    /// Window title set via OSC sequences.
    pub title: String,
}

impl Handler for Terminal {
    /// Writes a printable character at the cursor (stub).
    fn print(&mut self, _c: char) {}

    /// Advances to the next line (stub).
    fn linefeed(&mut self) {}

    /// Moves the cursor to column 0 (stub).
    fn carriage_return(&mut self) {}

    /// Moves the cursor one column left (stub).
    fn backspace(&mut self) {}

    /// Advances to the next tab stop (stub).
    fn tab(&mut self) {}

    /// Rings the bell / triggers visual bell (stub).
    fn bell(&mut self) {}

    /// Moves the cursor to an absolute cell (stub).
    fn move_cursor_to(&mut self, _row: usize, _col: usize) {}

    /// Moves the cursor by a relative offset (stub).
    fn move_cursor_by(&mut self, _d_row: isize, _d_col: isize) {}

    /// Erases part or all of the display (stub).
    fn erase_in_display(&mut self, _mode: EraseMode) {}

    /// Erases part or all of the current line (stub).
    fn erase_in_line(&mut self, _mode: EraseMode) {}

    /// Applies an SGR attribute (stub).
    fn set_sgr(&mut self, _attr: SgrAttr) {}

    /// Enables or disables a terminal mode (stub).
    fn set_mode(&mut self, _mode: Mode, _enabled: bool) {}

    /// Sets the scrolling region top and bottom rows (stub).
    fn set_scroll_region(&mut self, _top: usize, _bottom: usize) {}

    /// Sets the window title from an OSC sequence.
    fn set_title(&mut self, title: String) {
        self.title = title;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that terminal modes and grid cursor have matching initial cursor visibility.
    #[test]
    fn cursor_visibility_defaults_match() {
        let terminal = Terminal::default();
        assert!(terminal.modes.cursor_visible);
        assert_eq!(terminal.modes.cursor_visible, terminal.grid.cursor.visible);
    }
}
