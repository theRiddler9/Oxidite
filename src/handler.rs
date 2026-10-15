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
    Reset,
    Bold,
    Dim,
    Italic,
    Underline,
    Inverse,
    Hidden,
    Strikethrough,
    NormalIntensity,
    NotItalic,
    NotUnderline,
    NotInverse,
    NotHidden,
    NotStrikethrough,
    Foreground(crate::grid::Color),
    Background(crate::grid::Color),
}

/// Terminal modes toggled by DECSET / DECRST and related sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Autowrap,
    CursorVisible,
    ApplicationCursorKeys,
    BracketedPaste,
    /// SGR mouse reporting (1006) and related mouse modes as they land.
    MouseReporting,
    AlternateScreen,
}

/// Actions the parser applies to terminal state.
pub trait Handler {
    fn print(&mut self, c: char);
    fn linefeed(&mut self);
    fn carriage_return(&mut self);
    fn backspace(&mut self);
    fn tab(&mut self);
    fn bell(&mut self);
    fn move_cursor_to(&mut self, row: usize, col: usize);
    fn move_cursor_by(&mut self, d_row: isize, d_col: isize);
    fn erase_in_display(&mut self, mode: EraseMode);
    fn erase_in_line(&mut self, mode: EraseMode);
    fn set_sgr(&mut self, attr: SgrAttr);
    fn set_mode(&mut self, mode: Mode, enabled: bool);
    fn set_scroll_region(&mut self, top: usize, bottom: usize);
    fn set_title(&mut self, title: String);
}
