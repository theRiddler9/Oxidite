//! Terminal state: modes, alternate screen, `Handler` implementation (stub — Phase 3).

use crate::grid::Grid;
use crate::handler::{EraseMode, Handler, Mode, SgrAttr};

/// Runtime terminal modes.
#[derive(Debug, Clone, PartialEq, Eq)]
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

impl Modes {
    /// Updates a mode flag.
    pub fn set(&mut self, mode: Mode, enabled: bool) {
        match mode {
            Mode::Autowrap => self.autowrap = enabled,
            Mode::CursorVisible => self.cursor_visible = enabled,
            Mode::ApplicationCursorKeys => self.application_cursor_keys = enabled,
            Mode::BracketedPaste => self.bracketed_paste = enabled,
            Mode::MouseReporting => self.mouse_reporting = enabled,
            Mode::AlternateScreen => self.alternate_screen = enabled,
        }
    }

    /// Checks if a mode is currently enabled.
    #[must_use]
    pub const fn is_enabled(&self, mode: Mode) -> bool {
        match mode {
            Mode::Autowrap => self.autowrap,
            Mode::CursorVisible => self.cursor_visible,
            Mode::ApplicationCursorKeys => self.application_cursor_keys,
            Mode::BracketedPaste => self.bracketed_paste,
            Mode::MouseReporting => self.mouse_reporting,
            Mode::AlternateScreen => self.alternate_screen,
        }
    }
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

    #[test]
    fn modes_defaults_and_toggle() {
        let mut modes = Modes::default();
        assert!(modes.is_enabled(Mode::Autowrap));
        assert!(modes.is_enabled(Mode::CursorVisible));
        assert!(!modes.is_enabled(Mode::ApplicationCursorKeys));
        assert!(!modes.is_enabled(Mode::BracketedPaste));
        assert!(!modes.is_enabled(Mode::MouseReporting));
        assert!(!modes.is_enabled(Mode::AlternateScreen));

        modes.set(Mode::BracketedPaste, true);
        assert!(modes.is_enabled(Mode::BracketedPaste));
        assert!(modes.bracketed_paste);

        modes.set(Mode::Autowrap, false);
        assert!(!modes.is_enabled(Mode::Autowrap));
        assert!(!modes.autowrap);
    }
}
