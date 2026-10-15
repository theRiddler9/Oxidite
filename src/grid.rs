//! Terminal grid shapes: cells, colors, cursor.
//!
//! Full grid storage and mutation land in Phase 3.

/// Cell foreground / background color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

/// Text attributes for a cell (SGR styles).
///
/// Phase 3 may replace this with `bitflags`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Attrs {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
    pub hidden: bool,
    pub strikethrough: bool,
}

/// Cursor display shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorShape {
    #[default]
    Block,
    Underline,
    Bar,
}

/// One character cell on the grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub attrs: Attrs,
    /// Display columns occupied (1 for most chars, 2 for wide / CJK).
    pub width: u8,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: Color::Default,
            bg: Color::Default,
            attrs: Attrs::default(),
            width: 1,
        }
    }
}

/// Cursor position and appearance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    pub row: usize,
    pub col: usize,
    pub visible: bool,
    pub shape: CursorShape,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            row: 0,
            col: 0,
            visible: true,
            shape: CursorShape::Block,
        }
    }
}

/// Screen grid storage (stub — Phase 3).
#[derive(Debug, Default)]
pub struct Grid {
    pub cursor: Cursor,
}
