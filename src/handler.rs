//! Parser → terminal contract.
//!
//! The VT parser translates tokenized sequences into these method calls.
//! [`crate::terminal::Terminal`] implements this trait. The parser must not
//! contain grid logic.

/// CSI `J` / `K` erase modes (xterm ED / EL).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EraseMode {
    /// Erase from the cursor to the end (Ps = 0, default).
    ToEnd,
    /// Erase from the start to the cursor (Ps = 1).
    ToStart,
    /// Erase everything (Ps = 2).
    All,
    /// Erase everything including scrollback when supported (Ps = 3, ED only).
    AllIncludingScrollback,
}

/// Select Graphic Rendition attributes (CSI `m`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SgrAttr {
    /// SGR 0 — reset all attributes.
    Reset,
    /// SGR 1 — bold / increased intensity.
    Bold,
    /// SGR 2 — dim / decreased intensity.
    Dim,
    /// SGR 3 — italic.
    Italic,
    /// SGR 4 — underline.
    Underline,
    /// SGR 7 — inverse video.
    Inverse,
    /// SGR 8 — hidden / invisible.
    Hidden,
    /// SGR 9 — strikethrough.
    Strikethrough,
    /// SGR 22 — normal intensity (not bold/dim).
    NormalIntensity,
    /// SGR 23 — not italic.
    NotItalic,
    /// SGR 24 — not underlined.
    NotUnderline,
    /// SGR 27 — not inverse.
    NotInverse,
    /// SGR 28 — not hidden.
    NotHidden,
    /// SGR 29 — not strikethrough.
    NotStrikethrough,
    /// Foreground color (indexed, RGB, or default).
    Foreground(crate::grid::Color),
    /// Background color (indexed, RGB, or default).
    Background(crate::grid::Color),
}

/// Terminal modes toggled by DECSET / DECRST and related sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Automatic wrap at the right margin (DECAWM).
    Autowrap,
    /// Cursor visibility (DECTCEM).
    CursorVisible,
    /// Application cursor keys (DECCKM).
    ApplicationCursorKeys,
    /// Bracketed paste mode.
    BracketedPaste,
    /// SGR mouse reporting (1006) and related mouse modes as they land.
    MouseReporting,
    /// Alternate screen buffer.
    AlternateScreen,
}

/// Actions the parser applies to terminal state.
pub trait Handler {
    /// Prints a Unicode character at the cursor.
    fn print(&mut self, c: char);

    /// Performs a line feed (C0 `LF`).
    fn linefeed(&mut self);

    /// Performs a carriage return (C0 `CR`).
    fn carriage_return(&mut self);

    /// Performs a backspace (C0 `BS`).
    fn backspace(&mut self);

    /// Performs a horizontal tab (C0 `HT`).
    fn tab(&mut self);

    /// Rings the bell (C0 `BEL`).
    fn bell(&mut self);

    /// Moves the cursor to an absolute row and column.
    fn move_cursor_to(&mut self, row: usize, col: usize);

    /// Moves the cursor by a relative row/column delta.
    fn move_cursor_by(&mut self, d_row: isize, d_col: isize);

    /// Erases part of the display (`CSI Ps J`, ED).
    fn erase_in_display(&mut self, mode: EraseMode);

    /// Erases part of the current line (`CSI Ps K`, EL).
    fn erase_in_line(&mut self, mode: EraseMode);

    /// Applies a Select Graphic Rendition attribute (`CSI … m`).
    fn set_sgr(&mut self, attr: SgrAttr);

    /// Enables or disables a DEC/private mode.
    fn set_mode(&mut self, mode: Mode, enabled: bool);

    /// Sets the scrolling region (`CSI Ps ; Ps r`, DECSTBM).
    fn set_scroll_region(&mut self, top: usize, bottom: usize);

    /// Sets the window title (OSC 0 / 2).
    fn set_title(&mut self, title: String);
}
