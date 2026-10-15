//! Terminal state: modes, alternate screen, `Handler` implementation (stub — Phase 3).

use crate::grid::Grid;
use crate::handler::{EraseMode, Handler, Mode, SgrAttr};

/// Runtime terminal modes.
#[derive(Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Modes {
    pub autowrap: bool,
    pub cursor_visible: bool,
    pub application_cursor_keys: bool,
    pub bracketed_paste: bool,
    pub mouse_reporting: bool,
    pub alternate_screen: bool,
}

/// Full terminal state owned by the app (stub).
#[derive(Debug, Default)]
pub struct Terminal {
    pub grid: Grid,
    pub modes: Modes,
    pub title: String,
}

impl Handler for Terminal {
    fn print(&mut self, _c: char) {}
    fn linefeed(&mut self) {}
    fn carriage_return(&mut self) {}
    fn backspace(&mut self) {}
    fn tab(&mut self) {}
    fn bell(&mut self) {}
    fn move_cursor_to(&mut self, _row: usize, _col: usize) {}
    fn move_cursor_by(&mut self, _d_row: isize, _d_col: isize) {}
    fn erase_in_display(&mut self, _mode: EraseMode) {}
    fn erase_in_line(&mut self, _mode: EraseMode) {}
    fn set_sgr(&mut self, _attr: SgrAttr) {}
    fn set_mode(&mut self, _mode: Mode, _enabled: bool) {}
    fn set_scroll_region(&mut self, _top: usize, _bottom: usize) {}
    fn set_title(&mut self, title: String) {
        self.title = title;
    }
}
