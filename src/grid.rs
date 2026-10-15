//! Terminal grid shapes: cells, colors, cursor.
//!
//! Full grid storage and mutation land in Phase 3.

/// Cell foreground / background color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    /// Use the theme default foreground or background.
    #[default]
    Default,
    /// Indexed color (0–255).
    Indexed(u8),
    /// Truecolor RGB.
    Rgb(u8, u8, u8),
}

/// Text attributes for a cell (SGR styles).
///
/// Phase 3 may replace this with `bitflags`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Attrs {
    /// Bold text attribute.
    pub bold: bool,
    /// Dim / faint text attribute.
    pub dim: bool,
    /// Italic text attribute.
    pub italic: bool,
    /// Underline text attribute.
    pub underline: bool,
    /// Inverse / reverse video attribute.
    pub inverse: bool,
    /// Hidden / invisible text attribute.
    pub hidden: bool,
    /// Strikethrough text attribute.
    pub strikethrough: bool,
}

/// Cursor display shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorShape {
    /// Filled block cursor (default).
    #[default]
    Block,
    /// Underline cursor.
    Underline,
    /// Vertical bar cursor.
    Bar,
}

/// One character cell on the grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// Character stored in this cell.
    pub ch: char,
    /// Cell foreground color.
    pub fg: Color,
    /// Cell background color.
    pub bg: Color,
    /// Text style attributes.
    pub attrs: Attrs,
    /// Display columns occupied (1 for most chars, 2 for wide / CJK).
    pub width: u8,
}

impl Default for Cell {
    /// Empty space cell with default colors and attributes.
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
    /// 0-indexed row position.
    pub row: usize,
    /// 0-indexed column position.
    pub col: usize,
    /// Whether the cursor is currently visible.
    pub visible: bool,
    /// Display shape of the cursor.
    pub shape: CursorShape,
}

impl Default for Cursor {
    /// Origin cell, visible block cursor (matches [`crate::terminal::Modes`] default).
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
    /// Active cursor state.
    pub cursor: Cursor,
}
