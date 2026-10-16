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

bitflags::bitflags! {
    /// Text attributes for a cell (SGR styles).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct Attrs: u16 {
        /// Bold text attribute (SGR 1).
        const BOLD          = 1 << 0;
        /// Dim / faint text attribute (SGR 2).
        const DIM           = 1 << 1;
        /// Italic text attribute (SGR 3).
        const ITALIC        = 1 << 2;
        /// Underline text attribute (SGR 4).
        const UNDERLINE     = 1 << 3;
        /// Inverse / reverse video attribute (SGR 7).
        const INVERSE       = 1 << 4;
        /// Hidden / invisible text attribute (SGR 8).
        const HIDDEN        = 1 << 5;
        /// Strikethrough text attribute (SGR 9).
        const STRIKETHROUGH = 1 << 6;
    }
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

impl Cell {
    /// Creates a cell with custom character and styling attributes.
    #[must_use]
    pub const fn new(ch: char, fg: Color, bg: Color, attrs: Attrs, width: u8) -> Self {
        Self {
            ch,
            fg,
            bg,
            attrs,
            width,
        }
    }

    /// Creates an empty cell with whitespace, default colors, and default attributes.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            ch: ' ',
            fg: Color::Default,
            bg: Color::Default,
            attrs: Attrs::empty(),
            width: 1,
        }
    }

    /// Resets this cell back to empty whitespace with default styles.
    pub fn reset(&mut self) {
        *self = Self::empty();
    }

    /// Checks if this cell is an empty whitespace cell with default colors and no attributes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ch == ' '
            && self.fg == Color::Default
            && self.bg == Color::Default
            && self.attrs.is_empty()
    }
}

impl Default for Cell {
    /// Empty space cell with default colors and attributes.
    fn default() -> Self {
        Self::empty()
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

/// A single row of character cells on the grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Cells within this row.
    pub cells: Vec<Cell>,
}

impl Row {
    /// Creates a row with `cols` empty cells.
    #[must_use]
    pub fn new(cols: usize) -> Self {
        Self {
            cells: vec![Cell::empty(); cols],
        }
    }

    /// Resets all cells in this row to default empty cells.
    pub fn reset(&mut self) {
        for cell in &mut self.cells {
            cell.reset();
        }
    }

    /// Returns the number of cells in this row.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// Returns true if this row has zero cells.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
}

impl std::ops::Deref for Row {
    type Target = [Cell];

    fn deref(&self) -> &Self::Target {
        &self.cells
    }
}

impl std::ops::DerefMut for Row {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.cells
    }
}

/// 2D screen grid storing character cells and cursor state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid {
    /// Number of columns.
    pub cols: usize,
    /// Number of rows.
    pub rows: usize,
    /// Visible screen rows (0..rows).
    pub visible_rows: Vec<Row>,
    /// Active cursor state.
    pub cursor: Cursor,
}

impl Default for Grid {
    /// Default standard 80x24 terminal grid.
    fn default() -> Self {
        Self::new(80, 24)
    }
}

impl Grid {
    /// Creates a new grid with visible dimensions `cols` × `rows`.
    #[must_use]
    pub fn new(cols: usize, rows: usize) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        Self {
            cols,
            rows,
            visible_rows: (0..rows).map(|_| Row::new(cols)).collect(),
            cursor: Cursor::default(),
        }
    }

    /// Returns a reference to the cell at `(row, col)`, if within bounds.
    #[must_use]
    pub fn get(&self, row: usize, col: usize) -> Option<&Cell> {
        self.visible_rows.get(row).and_then(|r| r.cells.get(col))
    }

    /// Returns a mutable reference to the cell at `(row, col)`, if within bounds.
    pub fn get_mut(&mut self, row: usize, col: usize) -> Option<&mut Cell> {
        self.visible_rows
            .get_mut(row)
            .and_then(|r| r.cells.get_mut(col))
    }

    /// Returns a reference to the row at `row_idx`, if within bounds.
    #[must_use]
    pub fn row(&self, row_idx: usize) -> Option<&Row> {
        self.visible_rows.get(row_idx)
    }

    /// Returns a mutable reference to the row at `row_idx`, if within bounds.
    pub fn row_mut(&mut self, row_idx: usize) -> Option<&mut Row> {
        self.visible_rows.get_mut(row_idx)
    }
}

impl std::ops::Index<usize> for Grid {
    type Output = Row;

    fn index(&self, index: usize) -> &Self::Output {
        &self.visible_rows[index]
    }
}

impl std::ops::IndexMut<usize> for Grid {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.visible_rows[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_defaults_and_helpers() {
        let mut cell = Cell::default();
        assert!(cell.is_empty());
        assert_eq!(cell.ch, ' ');
        assert_eq!(cell.fg, Color::Default);
        assert_eq!(cell.bg, Color::Default);
        assert_eq!(cell.attrs, Attrs::empty());

        let bold_italic = Attrs::BOLD | Attrs::ITALIC;
        cell = Cell::new(
            'A',
            Color::Indexed(1),
            Color::Rgb(10, 20, 30),
            bold_italic,
            1,
        );
        assert!(!cell.is_empty());
        assert!(cell.attrs.contains(Attrs::BOLD));
        assert!(cell.attrs.contains(Attrs::ITALIC));
        assert!(!cell.attrs.contains(Attrs::UNDERLINE));

        cell.reset();
        assert!(cell.is_empty());
    }

    #[test]
    fn color_variants() {
        assert_eq!(Color::default(), Color::Default);
        let indexed = Color::Indexed(42);
        let rgb = Color::Rgb(255, 128, 0);
        assert_ne!(indexed, rgb);
    }

    #[test]
    fn grid_storage_and_indexing() {
        let mut grid = Grid::new(80, 24);
        assert_eq!(grid.cols, 80);
        assert_eq!(grid.rows, 24);
        assert_eq!(grid.visible_rows.len(), 24);
        assert_eq!(grid[0].len(), 80);

        // Get and get_mut
        assert!(grid.get(0, 0).is_some());
        assert!(grid.get(23, 79).is_some());
        assert!(grid.get(24, 0).is_none());
        assert!(grid.get(0, 80).is_none());

        // Mutation via indexing
        grid[2][5].ch = 'Z';
        grid[2][5].fg = Color::Indexed(3);
        assert_eq!(grid.get(2, 5).unwrap().ch, 'Z');
        assert_eq!(grid[2][5].fg, Color::Indexed(3));

        // Row reset
        grid.row_mut(2).unwrap().reset();
        assert!(grid[2][5].is_empty());
    }
}
