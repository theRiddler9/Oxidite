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
    /// Saved cursor state (`ESC 7` / `ESC 8`).
    pub saved_cursor: Option<Cursor>,
    /// Whether a deferred wrap is pending at the right margin.
    pub wrap_pending: bool,
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
            saved_cursor: None,
            wrap_pending: false,
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

    /// Moves the cursor to an absolute position clamped within grid bounds.
    pub fn move_cursor_to(&mut self, row: usize, col: usize) {
        self.cursor.row = row.min(self.rows.saturating_sub(1));
        self.cursor.col = col.min(self.cols.saturating_sub(1));
        self.wrap_pending = false;
    }

    /// Moves the cursor by a signed relative row and column offset, clamped within grid bounds.
    pub fn move_cursor_by(&mut self, d_row: isize, d_col: isize) {
        let max_row = isize::try_from(self.rows.saturating_sub(1)).unwrap_or(isize::MAX);
        let max_col = isize::try_from(self.cols.saturating_sub(1)).unwrap_or(isize::MAX);

        let current_row = isize::try_from(self.cursor.row).unwrap_or(isize::MAX);
        let current_col = isize::try_from(self.cursor.col).unwrap_or(isize::MAX);

        let new_row = (current_row + d_row).clamp(0, max_row);
        let new_col = (current_col + d_col).clamp(0, max_col);

        self.cursor.row = usize::try_from(new_row).unwrap_or(0);
        self.cursor.col = usize::try_from(new_col).unwrap_or(0);
        self.wrap_pending = false;
    }

    /// Advances the cursor to the next 8-column tab stop, clamped to the right margin.
    pub fn tab(&mut self) {
        let next_tab = (self.cursor.col / 8 + 1) * 8;
        self.cursor.col = next_tab.min(self.cols.saturating_sub(1));
        self.wrap_pending = false;
    }

    /// Saves the current cursor position and attributes (`ESC 7`, DECSC).
    pub fn save_cursor(&mut self) {
        self.saved_cursor = Some(self.cursor.clone());
    }

    /// Restores the saved cursor position and attributes (`ESC 8`, DECRC).
    pub fn restore_cursor(&mut self) {
        if let Some(saved) = &self.saved_cursor {
            self.cursor = saved.clone();
            self.wrap_pending = false;
        }
    }

    /// Erases parts of the line containing the cursor (`CSI Ps K`, EL).
    pub fn erase_in_line(&mut self, mode: crate::handler::EraseMode) {
        if self.cursor.row >= self.rows {
            return;
        }
        let col = self.cursor.col.min(self.cols);
        let row = &mut self.visible_rows[self.cursor.row];

        match mode {
            crate::handler::EraseMode::ToEnd => {
                for cell in &mut row.cells[col..] {
                    cell.reset();
                }
            }
            crate::handler::EraseMode::ToStart => {
                let end = (col + 1).min(self.cols);
                for cell in &mut row.cells[..end] {
                    cell.reset();
                }
            }
            crate::handler::EraseMode::All | crate::handler::EraseMode::AllIncludingScrollback => {
                row.reset();
            }
        }
    }

    /// Erases parts of the visible display (`CSI Ps J`, ED).
    pub fn erase_in_display(&mut self, mode: crate::handler::EraseMode) {
        let cur_row = self.cursor.row.min(self.rows.saturating_sub(1));
        let cur_col = self.cursor.col.min(self.cols);

        match mode {
            crate::handler::EraseMode::ToEnd => {
                // Erase from cursor to end of current line
                for cell in &mut self.visible_rows[cur_row].cells[cur_col..] {
                    cell.reset();
                }
                // Erase all lines below current line
                for row in &mut self.visible_rows[cur_row + 1..] {
                    row.reset();
                }
            }
            crate::handler::EraseMode::ToStart => {
                // Erase all lines above current line
                for row in &mut self.visible_rows[..cur_row] {
                    row.reset();
                }
                // Erase from start of line through cursor
                let end = (cur_col + 1).min(self.cols);
                for cell in &mut self.visible_rows[cur_row].cells[..end] {
                    cell.reset();
                }
            }
            crate::handler::EraseMode::All | crate::handler::EraseMode::AllIncludingScrollback => {
                for row in &mut self.visible_rows {
                    row.reset();
                }
            }
        }
    }

    /// Erases `count` characters starting at the cursor on the current row (`CSI Ps X`, ECH).
    pub fn erase_chars(&mut self, count: usize) {
        if self.cursor.row >= self.rows {
            return;
        }
        let start = self.cursor.col.min(self.cols);
        let end = (start + count).min(self.cols);
        for cell in &mut self.visible_rows[self.cursor.row].cells[start..end] {
            cell.reset();
        }
    }

    /// Writes a character at the cursor position with the specified styling,
    /// respecting autowrap mode and deferred wrap state.
    pub fn put_char(&mut self, c: char, fg: Color, bg: Color, attrs: Attrs, autowrap: bool) {
        if self.wrap_pending && autowrap {
            self.wrap_pending = false;
            self.cursor.col = 0;
            if self.cursor.row + 1 < self.rows {
                self.cursor.row += 1;
            } else {
                self.scroll_up(1);
            }
        }

        let row = self.cursor.row.min(self.rows.saturating_sub(1));
        let col = self.cursor.col.min(self.cols.saturating_sub(1));

        if let Some(cell) = self.get_mut(row, col) {
            *cell = Cell::new(c, fg, bg, attrs, 1);
        }

        if self.cursor.col + 1 < self.cols {
            self.cursor.col += 1;
            self.wrap_pending = false;
        } else if autowrap {
            self.wrap_pending = true;
        }
    }

    /// Scrolls the visible grid upwards by `count` lines, inserting blank rows at the bottom.
    pub fn scroll_up(&mut self, count: usize) {
        let count = count.min(self.rows);
        for _ in 0..count {
            if !self.visible_rows.is_empty() {
                self.visible_rows.remove(0);
                self.visible_rows.push(Row::new(self.cols));
            }
        }
    }

    /// Scrolls the visible grid downwards by `count` lines, inserting blank rows at the top.
    pub fn scroll_down(&mut self, count: usize) {
        let count = count.min(self.rows);
        for _ in 0..count {
            if !self.visible_rows.is_empty() {
                self.visible_rows.pop();
                self.visible_rows.insert(0, Row::new(self.cols));
            }
        }
    }

    /// Performs a reverse index (`ESC M`, RI).
    ///
    /// Moves cursor up one row, or scrolls the grid down if cursor is at the top row.
    pub fn reverse_index(&mut self) {
        self.wrap_pending = false;
        if self.cursor.row > 0 {
            self.cursor.row -= 1;
        } else {
            self.scroll_down(1);
        }
    }

    /// Performs a carriage return, moving cursor to column 0.
    pub fn carriage_return(&mut self) {
        self.cursor.col = 0;
        self.wrap_pending = false;
    }

    /// Performs a linefeed, advancing cursor down one line or scrolling if at bottom.
    pub fn linefeed(&mut self) {
        self.wrap_pending = false;
        if self.cursor.row + 1 < self.rows {
            self.cursor.row += 1;
        } else {
            self.scroll_up(1);
        }
    }

    /// Performs a backspace, moving cursor left by one column.
    pub fn backspace(&mut self) {
        self.wrap_pending = false;
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        }
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

    #[test]
    fn put_char_autowrap_and_scrolling() {
        let mut grid = Grid::new(4, 2);

        // Fill row 0 (columns 0, 1, 2, 3)
        grid.put_char('A', Color::Default, Color::Default, Attrs::empty(), true);
        grid.put_char('B', Color::Default, Color::Default, Attrs::empty(), true);
        grid.put_char('C', Color::Default, Color::Default, Attrs::empty(), true);
        grid.put_char('D', Color::Default, Color::Default, Attrs::empty(), true);

        // At end of row 0, wrap is pending
        assert_eq!(grid.cursor.row, 0);
        assert_eq!(grid.cursor.col, 3);
        assert!(grid.wrap_pending);

        // Next character wraps to row 1, col 0, writes 'E', and cursor advances to col 1
        grid.put_char('E', Color::Default, Color::Default, Attrs::empty(), true);
        assert_eq!(grid.cursor.row, 1);
        assert_eq!(grid.cursor.col, 1);
        assert!(!grid.wrap_pending);
        assert_eq!(grid[0][3].ch, 'D');
        assert_eq!(grid[1][0].ch, 'E');

        // Autowrap disabled: does not wrap to next row, overwrites last cell
        let mut no_wrap_grid = Grid::new(3, 2);
        no_wrap_grid.put_char('X', Color::Default, Color::Default, Attrs::empty(), false);
        no_wrap_grid.put_char('Y', Color::Default, Color::Default, Attrs::empty(), false);
        no_wrap_grid.put_char('Z', Color::Default, Color::Default, Attrs::empty(), false);
        no_wrap_grid.put_char('W', Color::Default, Color::Default, Attrs::empty(), false);
        assert_eq!(no_wrap_grid.cursor.row, 0);
        assert_eq!(no_wrap_grid.cursor.col, 2);
        assert_eq!(no_wrap_grid[0][2].ch, 'W');
        assert!(!no_wrap_grid.wrap_pending);
    }

    #[test]
    fn cursor_movement_clamping_and_save_restore() {
        let mut grid = Grid::new(80, 24);

        // Absolute move with clamping
        grid.move_cursor_to(10, 20);
        assert_eq!(grid.cursor.row, 10);
        assert_eq!(grid.cursor.col, 20);

        grid.move_cursor_to(100, 200);
        assert_eq!(grid.cursor.row, 23);
        assert_eq!(grid.cursor.col, 79);

        // Relative move with clamping
        grid.move_cursor_by(-5, -10);
        assert_eq!(grid.cursor.row, 18);
        assert_eq!(grid.cursor.col, 69);

        grid.move_cursor_by(-50, -100);
        assert_eq!(grid.cursor.row, 0);
        assert_eq!(grid.cursor.col, 0);

        // Tab stops
        grid.cursor.col = 3;
        grid.tab();
        assert_eq!(grid.cursor.col, 8);

        grid.tab();
        assert_eq!(grid.cursor.col, 16);

        grid.cursor.col = 78;
        grid.tab();
        assert_eq!(grid.cursor.col, 79);

        // Save & restore
        grid.move_cursor_to(5, 12);
        grid.save_cursor();
        grid.move_cursor_to(0, 0);
        assert_eq!(grid.cursor.row, 0);
        assert_eq!(grid.cursor.col, 0);

        grid.restore_cursor();
        assert_eq!(grid.cursor.row, 5);
        assert_eq!(grid.cursor.col, 12);
    }

    #[test]
    fn erase_operations() {
        use crate::handler::EraseMode;

        let mut grid = Grid::new(5, 3);
        // Fill grid with 'A'..'O'
        let mut ch = 'A';
        for r in 0..3 {
            for c in 0..5 {
                grid[r][c] = Cell::new(ch, Color::Default, Color::Default, Attrs::empty(), 1);
                ch = (ch as u8 + 1) as char;
            }
        }

        // Row 0: A B C D E
        // Row 1: F G H I J
        // Row 2: K L M N O

        // Erase in line: ToEnd at col 2 on row 1
        grid.move_cursor_to(1, 2);
        grid.erase_in_line(EraseMode::ToEnd);
        assert_eq!(grid[1][0].ch, 'F');
        assert_eq!(grid[1][1].ch, 'G');
        assert!(grid[1][2].is_empty());
        assert!(grid[1][3].is_empty());
        assert!(grid[1][4].is_empty());

        // Erase in line: ToStart at col 3 on row 0
        grid.move_cursor_to(0, 3);
        grid.erase_in_line(EraseMode::ToStart);
        assert!(grid[0][0].is_empty());
        assert!(grid[0][1].is_empty());
        assert!(grid[0][2].is_empty());
        assert!(grid[0][3].is_empty());
        assert_eq!(grid[0][4].ch, 'E');

        // Erase chars: 2 chars on row 2 at col 1
        grid.move_cursor_to(2, 1);
        grid.erase_chars(2);
        assert_eq!(grid[2][0].ch, 'K');
        assert!(grid[2][1].is_empty());
        assert!(grid[2][2].is_empty());
        assert_eq!(grid[2][3].ch, 'N');
        assert_eq!(grid[2][4].ch, 'O');

        // Refill grid and test erase_in_display
        for r in 0..3 {
            for c in 0..5 {
                grid[r][c] = Cell::new('X', Color::Default, Color::Default, Attrs::empty(), 1);
            }
        }

        // Erase in display: ToEnd at (1, 2)
        grid.move_cursor_to(1, 2);
        grid.erase_in_display(EraseMode::ToEnd);
        assert_eq!(grid[0][0].ch, 'X');
        assert_eq!(grid[0][4].ch, 'X');
        assert_eq!(grid[1][0].ch, 'X');
        assert_eq!(grid[1][1].ch, 'X');
        assert!(grid[1][2].is_empty());
        assert!(grid[1][4].is_empty());
        assert!(grid[2][0].is_empty());
        assert!(grid[2][4].is_empty());

        // Erase in display: All
        grid.erase_in_display(EraseMode::All);
        assert!(grid[0][0].is_empty());
        assert!(grid[1][0].is_empty());
        assert!(grid[2][0].is_empty());
    }

    #[test]
    fn scroll_up_down_and_reverse_index() {
        let mut grid = Grid::new(3, 3);
        grid[0][0].ch = '1';
        grid[1][0].ch = '2';
        grid[2][0].ch = '3';

        // Scroll up by 1: row 0 becomes '2', row 1 becomes '3', row 2 becomes empty
        grid.scroll_up(1);
        assert_eq!(grid[0][0].ch, '2');
        assert_eq!(grid[1][0].ch, '3');
        assert!(grid[2][0].is_empty());

        // Scroll down by 1: row 0 becomes empty, row 1 becomes '2', row 2 becomes '3'
        grid.scroll_down(1);
        assert!(grid[0][0].is_empty());
        assert_eq!(grid[1][0].ch, '2');
        assert_eq!(grid[2][0].ch, '3');

        // Reverse index when cursor is on row 1: moves cursor to row 0 without scrolling
        grid.cursor.row = 1;
        grid.reverse_index();
        assert_eq!(grid.cursor.row, 0);

        // Reverse index when cursor is on row 0: scrolls down, cursor stays at row 0
        grid.reverse_index();
        assert_eq!(grid.cursor.row, 0);
        assert!(grid[0][0].is_empty());
        assert!(grid[1][0].is_empty());
        assert_eq!(grid[2][0].ch, '2');
    }
}
