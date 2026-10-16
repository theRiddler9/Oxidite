//! Terminal state: modes, alternate screen, `Handler` implementation (stub — Phase 3).

use crate::grid::{Attrs, Color, Grid};
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

/// Full terminal state owning grid storage, alternate buffer, modes, and title.
#[derive(Debug)]
pub struct Terminal {
    /// Active screen grid storage.
    pub grid: Grid,
    /// Inactive screen grid buffer (swapped when `Mode::AlternateScreen` is toggled).
    pub alternate_grid: Grid,
    /// Runtime terminal modes.
    pub modes: Modes,
    /// Window title set via OSC sequences.
    pub title: String,
    /// Active foreground color applied to newly printed cells.
    pub fg: Color,
    /// Active background color applied to newly printed cells.
    pub bg: Color,
    /// Active SGR style attributes applied to newly printed cells.
    pub attrs: Attrs,
    /// Number of bell events received.
    pub bell_count: u64,
    /// Flags marking which visible rows need redrawing.
    pub dirty_rows: Vec<bool>,
}

impl Default for Terminal {
    fn default() -> Self {
        Self::new(80, 24)
    }
}

impl Terminal {
    /// Creates a new terminal with the given column and row dimensions.
    #[must_use]
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            grid: Grid::new(cols, rows),
            alternate_grid: Grid::with_scrollback(cols, rows, 0),
            modes: Modes::default(),
            title: String::new(),
            fg: Color::Default,
            bg: Color::Default,
            attrs: Attrs::empty(),
            bell_count: 0,
            dirty_rows: vec![true; rows],
        }
    }

    /// Resizes both the active grid and alternate screen grid.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.grid.resize(cols, rows);
        self.alternate_grid.resize(cols, rows);
        self.dirty_rows = vec![true; self.grid.rows];
    }

    /// Enables or disables the alternate screen buffer, swapping the active and inactive grids.
    pub fn set_alternate_screen(&mut self, active: bool) {
        if self.modes.alternate_screen == active {
            return;
        }
        self.modes.alternate_screen = active;
        std::mem::swap(&mut self.grid, &mut self.alternate_grid);
        if active {
            self.grid.clear_scrollback();
            self.grid.erase_in_display(EraseMode::All);
            self.grid.move_cursor_to(0, 0);
        }
        self.mark_all_dirty();
    }

    /// Marks a specific visible row as dirty (damaged).
    pub fn mark_row_dirty(&mut self, row: usize) {
        if let Some(dirty) = self.dirty_rows.get_mut(row) {
            *dirty = true;
        }
    }

    /// Marks all visible rows as dirty (damaged).
    pub fn mark_all_dirty(&mut self) {
        self.dirty_rows.fill(true);
    }

    /// Clears damage flags for all rows after redrawing.
    pub fn clear_damage(&mut self) {
        self.dirty_rows.fill(false);
    }

    /// Returns whether any visible row is currently marked dirty.
    #[must_use]
    pub fn has_damage(&self) -> bool {
        self.dirty_rows.iter().any(|&d| d)
    }

    /// Returns whether a specific row is marked dirty.
    #[must_use]
    pub fn is_row_dirty(&self, row: usize) -> bool {
        self.dirty_rows.get(row).copied().unwrap_or(false)
    }
}

impl Handler for Terminal {
    /// Writes a printable character at the cursor using active colors and styles.
    fn print(&mut self, c: char) {
        let prev_row = self.grid.cursor.row;
        self.grid
            .put_char(c, self.fg, self.bg, self.attrs, self.modes.autowrap);
        self.mark_row_dirty(prev_row);
        self.mark_row_dirty(self.grid.cursor.row);
    }

    /// Advances to the next line (C0 `LF`).
    fn linefeed(&mut self) {
        let prev_row = self.grid.cursor.row;
        self.grid.linefeed();
        if self.grid.cursor.row == prev_row {
            self.mark_all_dirty();
        } else {
            self.mark_row_dirty(prev_row);
            self.mark_row_dirty(self.grid.cursor.row);
        }
    }

    /// Moves the cursor to column 0 (C0 `CR`).
    fn carriage_return(&mut self) {
        self.mark_row_dirty(self.grid.cursor.row);
        self.grid.carriage_return();
    }

    /// Moves the cursor one column left (C0 `BS`).
    fn backspace(&mut self) {
        self.mark_row_dirty(self.grid.cursor.row);
        self.grid.backspace();
    }

    /// Advances to the next tab stop (C0 `HT`).
    fn tab(&mut self) {
        self.mark_row_dirty(self.grid.cursor.row);
        self.grid.tab();
    }

    /// Rings the bell (C0 `BEL`).
    fn bell(&mut self) {
        self.bell_count = self.bell_count.wrapping_add(1);
    }

    /// Moves the cursor to an absolute 0-indexed row and column (`CSI Ps ; Ps H / f`).
    fn move_cursor_to(&mut self, row: usize, col: usize) {
        let prev_row = self.grid.cursor.row;
        self.grid.move_cursor_to(row, col);
        self.mark_row_dirty(prev_row);
        self.mark_row_dirty(self.grid.cursor.row);
    }

    /// Moves the cursor by a relative offset (`CSI A / B / C / D`).
    fn move_cursor_by(&mut self, d_row: isize, d_col: isize) {
        let prev_row = self.grid.cursor.row;
        self.grid.move_cursor_by(d_row, d_col);
        self.mark_row_dirty(prev_row);
        self.mark_row_dirty(self.grid.cursor.row);
    }

    /// Erases part or all of the display (`CSI Ps J`, ED).
    fn erase_in_display(&mut self, mode: EraseMode) {
        self.grid.erase_in_display(mode);
        self.mark_all_dirty();
    }

    /// Erases part or all of the current line (`CSI Ps K`, EL).
    fn erase_in_line(&mut self, mode: EraseMode) {
        self.grid.erase_in_line(mode);
        self.mark_row_dirty(self.grid.cursor.row);
    }

    /// Applies an SGR attribute (`CSI … m`).
    fn set_sgr(&mut self, attr: SgrAttr) {
        match attr {
            SgrAttr::Reset => {
                self.fg = Color::Default;
                self.bg = Color::Default;
                self.attrs = Attrs::empty();
            }
            SgrAttr::Bold => self.attrs.insert(Attrs::BOLD),
            SgrAttr::Dim => self.attrs.insert(Attrs::DIM),
            SgrAttr::Italic => self.attrs.insert(Attrs::ITALIC),
            SgrAttr::Underline => self.attrs.insert(Attrs::UNDERLINE),
            SgrAttr::Inverse => self.attrs.insert(Attrs::INVERSE),
            SgrAttr::Hidden => self.attrs.insert(Attrs::HIDDEN),
            SgrAttr::Strikethrough => self.attrs.insert(Attrs::STRIKETHROUGH),
            SgrAttr::NormalIntensity => self.attrs.remove(Attrs::BOLD | Attrs::DIM),
            SgrAttr::NotItalic => self.attrs.remove(Attrs::ITALIC),
            SgrAttr::NotUnderline => self.attrs.remove(Attrs::UNDERLINE),
            SgrAttr::NotInverse => self.attrs.remove(Attrs::INVERSE),
            SgrAttr::NotHidden => self.attrs.remove(Attrs::HIDDEN),
            SgrAttr::NotStrikethrough => self.attrs.remove(Attrs::STRIKETHROUGH),
            SgrAttr::Foreground(color) => self.fg = color,
            SgrAttr::Background(color) => self.bg = color,
        }
    }

    /// Enables or disables a terminal mode.
    fn set_mode(&mut self, mode: Mode, enabled: bool) {
        match mode {
            Mode::AlternateScreen => self.set_alternate_screen(enabled),
            Mode::CursorVisible => {
                self.modes.cursor_visible = enabled;
                self.grid.cursor.visible = enabled;
                self.mark_row_dirty(self.grid.cursor.row);
            }
            _ => self.modes.set(mode, enabled),
        }
    }

    /// Sets the scrolling region top and bottom rows (`CSI Ps ; Ps r`, DECSTBM).
    fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        self.grid.set_scroll_region(top, bottom);
        self.mark_all_dirty();
    }

    /// Sets the window title from an OSC sequence.
    fn set_title(&mut self, title: String) {
        self.title = title;
    }

    /// Saves the current cursor position and attributes (`ESC 7`, DECSC).
    fn save_cursor(&mut self) {
        self.grid.save_cursor();
    }

    /// Restores the saved cursor position and attributes (`ESC 8`, DECRC).
    fn restore_cursor(&mut self) {
        let prev_row = self.grid.cursor.row;
        self.grid.restore_cursor();
        self.mark_row_dirty(prev_row);
        self.mark_row_dirty(self.grid.cursor.row);
    }

    /// Performs a reverse index (`ESC M`, RI).
    fn reverse_index(&mut self) {
        let prev_row = self.grid.cursor.row;
        self.grid.reverse_index();
        if self.grid.cursor.row == prev_row {
            self.mark_all_dirty();
        } else {
            self.mark_row_dirty(prev_row);
            self.mark_row_dirty(self.grid.cursor.row);
        }
    }

    /// Inserts `count` blank lines at the cursor within scroll region (`CSI Ps L`, IL).
    fn insert_lines(&mut self, count: usize) {
        self.grid.insert_lines(count);
        self.mark_all_dirty();
    }

    /// Deletes `count` lines starting at the cursor within scroll region (`CSI Ps M`, DL).
    fn delete_lines(&mut self, count: usize) {
        self.grid.delete_lines(count);
        self.mark_all_dirty();
    }

    /// Inserts `count` blank characters starting at the cursor (`CSI Ps @`, ICH).
    fn insert_blank_chars(&mut self, count: usize) {
        self.grid.insert_blank_chars(count);
        self.mark_row_dirty(self.grid.cursor.row);
    }

    /// Deletes `count` characters starting at the cursor (`CSI Ps P`, DCH).
    fn delete_chars(&mut self, count: usize) {
        self.grid.delete_chars(count);
        self.mark_row_dirty(self.grid.cursor.row);
    }

    /// Erases `count` characters starting at the cursor (`CSI Ps X`, ECH).
    fn erase_chars(&mut self, count: usize) {
        self.grid.erase_chars(count);
        self.mark_row_dirty(self.grid.cursor.row);
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

    #[test]
    fn alternate_screen_switching_and_preservation() {
        let mut terminal = Terminal::new(5, 3);
        // Put content on primary screen
        terminal.grid[0][0].ch = 'P';
        terminal.grid.move_cursor_to(1, 2);

        // Switch to alternate screen
        terminal.set_mode(Mode::AlternateScreen, true);
        assert!(terminal.modes.alternate_screen);
        assert!(terminal.grid[0][0].is_empty());
        assert_eq!(terminal.grid.cursor.row, 0);
        assert_eq!(terminal.grid.cursor.col, 0);

        // Put content on alternate screen
        terminal.grid[0][0].ch = 'A';

        // Switch back to primary screen
        terminal.set_mode(Mode::AlternateScreen, false);
        assert!(!terminal.modes.alternate_screen);
        assert_eq!(terminal.grid[0][0].ch, 'P');
        assert_eq!(terminal.grid.cursor.row, 1);
        assert_eq!(terminal.grid.cursor.col, 2);
    }

    #[test]
    fn terminal_handler_full_workflow() {
        let mut term = Terminal::new(10, 4);

        // Title and Bell
        term.set_title("Test Window".to_string());
        assert_eq!(term.title, "Test Window");
        term.bell();
        assert_eq!(term.bell_count, 1);

        // SGR attributes
        term.set_sgr(SgrAttr::Bold);
        term.set_sgr(SgrAttr::Foreground(Color::Indexed(2)));
        term.print('H');
        term.print('i');
        assert_eq!(term.grid[0][0].ch, 'H');
        assert_eq!(term.grid[0][0].fg, Color::Indexed(2));
        assert!(term.grid[0][0].attrs.contains(Attrs::BOLD));

        // C0 controls
        term.carriage_return();
        assert_eq!(term.grid.cursor.col, 0);
        term.linefeed();
        assert_eq!(term.grid.cursor.row, 1);
        term.tab();
        assert_eq!(term.grid.cursor.col, 8);
        term.backspace();
        assert_eq!(term.grid.cursor.col, 7);

        // Insert and delete characters
        term.print('X');
        term.print('Y');
        term.move_cursor_to(1, 7);
        term.insert_blank_chars(1);
        assert_eq!(term.grid[1][7].ch, ' ');
        assert_eq!(term.grid[1][8].ch, 'X');
        term.delete_chars(1);
        assert_eq!(term.grid[1][7].ch, 'X');

        // Cursor visibility mode
        term.set_mode(Mode::CursorVisible, false);
        assert!(!term.grid.cursor.visible);
        assert!(!term.modes.cursor_visible);
    }

    #[test]
    fn damage_tracking_workflow() {
        let mut term = Terminal::new(10, 4);
        assert!(term.has_damage());
        assert!(term.is_row_dirty(0));
        assert!(term.is_row_dirty(3));

        // Clear initial damage
        term.clear_damage();
        assert!(!term.has_damage());
        assert!(!term.is_row_dirty(0));

        // Printing a character damages row 0
        term.print('Z');
        assert!(term.has_damage());
        assert!(term.is_row_dirty(0));
        assert!(!term.is_row_dirty(1));

        // Moving cursor to row 2 damages row 0 (old cursor) and row 2 (new cursor)
        term.clear_damage();
        term.move_cursor_to(2, 5);
        assert!(term.is_row_dirty(0));
        assert!(term.is_row_dirty(2));
        assert!(!term.is_row_dirty(1));

        // Erase in display damages all rows
        term.clear_damage();
        term.erase_in_display(EraseMode::All);
        assert!(term.is_row_dirty(0));
        assert!(term.is_row_dirty(1));
        assert!(term.is_row_dirty(2));
        assert!(term.is_row_dirty(3));
    }
}
