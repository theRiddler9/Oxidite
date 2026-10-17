//! Terminal grid shapes: cells, colors, cursor.
//!
//! Full grid storage and mutation land in Phase 3.

use std::collections::VecDeque;
use unicode_width::UnicodeWidthChar;

/// Default maximum number of lines preserved in the scrollback buffer.
pub const DEFAULT_MAX_SCROLLBACK: usize = 10_000;

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

    /// Returns whether this cell is the leading half of a 2-cell wide character.
    #[must_use]
    pub const fn is_wide_lead(&self) -> bool {
        self.width == 2
    }

    /// Returns whether this cell is the trailing/continuation half of a 2-cell wide character.
    #[must_use]
    pub const fn is_wide_continuation(&self) -> bool {
        self.width == 0
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

    /// Resizes the row to `new_cols` cells, padding with empty cells or truncating.
    pub fn resize(&mut self, new_cols: usize) {
        if new_cols > self.cells.len() {
            self.cells.resize(new_cols, Cell::empty());
        } else if new_cols < self.cells.len() {
            self.cells.truncate(new_cols);
            if new_cols > 0 && self.cells[new_cols - 1].is_wide_lead() {
                self.cells[new_cols - 1] = Cell::empty();
            }
        }
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
    /// Active vertical scroll region (top..=bottom inclusive).
    pub scroll_region: std::ops::RangeInclusive<usize>,
    /// Scrollback ring buffer holding rows that scrolled off the top margin.
    pub scrollback: VecDeque<Row>,
    /// Maximum number of scrollback lines retained.
    pub max_scrollback: usize,
}

impl Default for Grid {
    /// Default standard 80x24 terminal grid.
    fn default() -> Self {
        Self::new(80, 24)
    }
}

impl Grid {
    /// Creates a new grid with visible dimensions `cols` × `rows` and default scrollback limit.
    #[must_use]
    pub fn new(cols: usize, rows: usize) -> Self {
        Self::with_scrollback(cols, rows, DEFAULT_MAX_SCROLLBACK)
    }

    /// Creates a new grid with visible dimensions and a custom scrollback line limit.
    #[must_use]
    pub fn with_scrollback(cols: usize, rows: usize, max_scrollback: usize) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        Self {
            cols,
            rows,
            visible_rows: (0..rows).map(|_| Row::new(cols)).collect(),
            cursor: Cursor::default(),
            saved_cursor: None,
            wrap_pending: false,
            scroll_region: 0..=(rows.saturating_sub(1)),
            scrollback: VecDeque::new(),
            max_scrollback,
        }
    }

    /// Returns the number of lines currently held in the scrollback buffer.
    #[must_use]
    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    /// Returns a reference to a scrollback row by 0-based index (0 is oldest).
    #[must_use]
    pub fn scrollback_row(&self, idx: usize) -> Option<&Row> {
        self.scrollback.get(idx)
    }

    /// Clears all lines currently stored in the scrollback buffer.
    pub fn clear_scrollback(&mut self) {
        self.scrollback.clear();
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

    /// Resets the cell at `(row, col)` to an empty cell with default attributes, if within bounds.
    pub fn reset_cell(&mut self, row: usize, col: usize) {
        if let Some(cell) = self.get_mut(row, col) {
            *cell = Cell::empty();
        }
    }

    /// Resizes the grid to new column and row dimensions, adjusting visible rows,
    /// scrollback rows, scroll regions, and clamping the cursor.
    pub fn resize(&mut self, new_cols: usize, new_rows: usize) {
        let new_cols = new_cols.max(1);
        let new_rows = new_rows.max(1);

        if new_cols == self.cols && new_rows == self.rows {
            return;
        }

        // 1. Adjust column width across all visible rows and scrollback
        if new_cols != self.cols {
            for row in &mut self.visible_rows {
                row.resize(new_cols);
            }
            for row in &mut self.scrollback {
                row.resize(new_cols);
            }
        }

        // 2. Adjust row count
        if new_rows > self.rows {
            // Expand: pull lines from scrollback if available, else append empty rows
            while self.visible_rows.len() < new_rows {
                if let Some(mut row) = self.scrollback.pop_back() {
                    row.resize(new_cols);
                    self.visible_rows.insert(0, row);
                    self.cursor.row = self.cursor.row.saturating_add(1);
                } else {
                    self.visible_rows.push(Row::new(new_cols));
                }
            }
        } else if new_rows < self.rows {
            // Shrink: pop empty bottom rows if cursor is above them,
            // otherwise push top lines to scrollback and shift cursor up.
            while self.visible_rows.len() > new_rows {
                if self.cursor.row < self.visible_rows.len() - 1
                    && self
                        .visible_rows
                        .last()
                        .is_some_and(|r| r.cells.iter().all(Cell::is_empty))
                {
                    self.visible_rows.pop();
                } else {
                    let row = self.visible_rows.remove(0);
                    if self.max_scrollback > 0 {
                        if self.scrollback.len() >= self.max_scrollback {
                            self.scrollback.pop_front();
                        }
                        self.scrollback.push_back(row);
                    }
                    self.cursor.row = self.cursor.row.saturating_sub(1);
                }
            }
        }

        self.cols = new_cols;
        self.rows = new_rows;

        // 3. Re-clamp cursor and reset wrap
        self.cursor.row = self.cursor.row.min(self.rows.saturating_sub(1));
        self.cursor.col = self.cursor.col.min(self.cols.saturating_sub(1));
        self.wrap_pending = false;

        if let Some(ref mut saved) = self.saved_cursor {
            saved.row = saved.row.min(self.rows.saturating_sub(1));
            saved.col = saved.col.min(self.cols.saturating_sub(1));
        }

        // 4. Reset scroll region to full screen height
        self.reset_scroll_region();
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

    /// Returns the visible grid lines as plain text, trimming trailing spaces from each line.
    #[must_use]
    pub fn to_plain_text(&self) -> String {
        let mut out = String::new();
        for r in 0..self.rows {
            let mut line = String::new();
            if let Some(row) = self.row(r) {
                for cell in &row.cells {
                    if !cell.is_wide_continuation() {
                        line.push(cell.ch);
                    }
                }
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
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
            crate::handler::EraseMode::All => {
                for row in &mut self.visible_rows {
                    row.reset();
                }
            }
            crate::handler::EraseMode::AllIncludingScrollback => {
                for row in &mut self.visible_rows {
                    row.reset();
                }
                self.clear_scrollback();
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

    /// Inserts `count` blank characters starting at the cursor (`CSI Ps @`, ICH).
    pub fn insert_blank_chars(&mut self, count: usize) {
        if self.cursor.row >= self.rows || self.cursor.col >= self.cols {
            return;
        }
        self.wrap_pending = false;
        let col = self.cursor.col;
        let row = &mut self.visible_rows[self.cursor.row];
        let count = count.min(self.cols - col);
        for _ in 0..count {
            row.cells.pop();
            row.cells.insert(col, Cell::empty());
        }
    }

    /// Deletes `count` characters starting at the cursor (`CSI Ps P`, DCH).
    pub fn delete_chars(&mut self, count: usize) {
        if self.cursor.row >= self.rows || self.cursor.col >= self.cols {
            return;
        }
        self.wrap_pending = false;
        let col = self.cursor.col;
        let row = &mut self.visible_rows[self.cursor.row];
        let count = count.min(self.cols - col);
        for _ in 0..count {
            row.cells.remove(col);
            row.cells.push(Cell::empty());
        }
    }

    /// Writes a character at the cursor position with the specified styling,
    /// respecting autowrap mode, deferred wrap state, and wide character widths (CJK / emoji).
    pub fn put_char(&mut self, c: char, fg: Color, bg: Color, attrs: Attrs, autowrap: bool) {
        let char_width = if UnicodeWidthChar::width(c).unwrap_or(1) >= 2 {
            2u8
        } else {
            1u8
        };

        if self.wrap_pending && autowrap {
            self.wrap_pending = false;
            self.cursor.col = 0;
            self.linefeed();
        } else if char_width == 2 && self.cursor.col + 1 >= self.cols && autowrap {
            // Cannot fit 2-cell wide character in the last column: fill last cell with space and wrap
            let row = self.cursor.row.min(self.rows.saturating_sub(1));
            let col = self.cursor.col.min(self.cols.saturating_sub(1));
            if let Some(cell) = self.get_mut(row, col) {
                *cell = Cell::new(' ', fg, bg, attrs, 1);
            }
            self.cursor.col = 0;
            self.linefeed();
        }

        let row = self.cursor.row.min(self.rows.saturating_sub(1));
        let col = self.cursor.col.min(self.cols.saturating_sub(1));

        // Overwrite protection for wide characters:
        // 1. If overwriting a continuation cell, clear its wide leader cell at col - 1
        if col > 0 && self.get(row, col).is_some_and(Cell::is_wide_continuation) {
            self.reset_cell(row, col - 1);
        }
        if self.get(row, col).is_some_and(Cell::is_wide_lead) {
            self.reset_cell(row, col + 1);
        }

        // 2. If writing a wide character and next cell is a wide leader, clear its continuation cell
        if char_width == 2 && self.get(row, col + 1).is_some_and(Cell::is_wide_lead) {
            self.reset_cell(row, col + 2);
        }

        // Write the character
        if let Some(cell) = self.get_mut(row, col) {
            *cell = Cell::new(c, fg, bg, attrs, char_width);
        }

        if char_width == 2 {
            if let Some(next) = self.get_mut(row, col + 1) {
                *next = Cell::new(' ', fg, bg, attrs, 0);
            }
            if self.cursor.col + 2 < self.cols {
                self.cursor.col += 2;
                self.wrap_pending = false;
            } else {
                self.cursor.col = self.cols.saturating_sub(1);
                if autowrap {
                    self.wrap_pending = true;
                }
            }
        } else if self.cursor.col + 1 < self.cols {
            self.cursor.col += 1;
            self.wrap_pending = false;
        } else if autowrap {
            self.wrap_pending = true;
        }
    }

    /// Sets the vertical scroll region (`CSI Ps ; Ps r`, DECSTBM).
    ///
    /// The parameters `top` and `bottom` are 0-indexed row boundaries.
    /// Resets the cursor to (0, 0) and clears pending wrap.
    pub fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        if top < bottom && bottom < self.rows {
            self.scroll_region = top..=bottom;
        } else {
            self.scroll_region = 0..=(self.rows.saturating_sub(1));
        }
        self.move_cursor_to(0, 0);
    }

    /// Resets the scroll region to the full visible screen height.
    pub fn reset_scroll_region(&mut self) {
        self.scroll_region = 0..=(self.rows.saturating_sub(1));
    }

    /// Scrolls rows upwards by `count` lines within the active scroll region,
    /// inserting blank rows at the bottom of the region.
    ///
    /// If the top margin is 0, scrolled-off rows are pushed into the scrollback ring buffer.
    pub fn scroll_up(&mut self, count: usize) {
        let top = *self.scroll_region.start();
        let bottom = (*self.scroll_region.end()).min(self.rows.saturating_sub(1));
        if top >= bottom || self.visible_rows.is_empty() {
            return;
        }
        let region_height = bottom - top + 1;
        let count = count.min(region_height);
        for _ in 0..count {
            let removed = self.visible_rows.remove(top);
            if top == 0 && self.max_scrollback > 0 {
                if self.scrollback.len() >= self.max_scrollback {
                    self.scrollback.pop_front();
                }
                self.scrollback.push_back(removed);
            }
            self.visible_rows.insert(bottom, Row::new(self.cols));
        }
    }

    /// Scrolls rows downwards by `count` lines within the active scroll region,
    /// inserting blank rows at the top of the region.
    pub fn scroll_down(&mut self, count: usize) {
        let top = *self.scroll_region.start();
        let bottom = (*self.scroll_region.end()).min(self.rows.saturating_sub(1));
        if top >= bottom || self.visible_rows.is_empty() {
            return;
        }
        let region_height = bottom - top + 1;
        let count = count.min(region_height);
        for _ in 0..count {
            self.visible_rows.remove(bottom);
            self.visible_rows.insert(top, Row::new(self.cols));
        }
    }

    /// Inserts `count` blank lines at the cursor within the scroll region (`CSI Ps L`, IL).
    pub fn insert_lines(&mut self, count: usize) {
        let top = *self.scroll_region.start();
        let bottom = (*self.scroll_region.end()).min(self.rows.saturating_sub(1));
        if self.cursor.row < top || self.cursor.row > bottom || top >= bottom {
            return;
        }
        self.wrap_pending = false;
        let cur = self.cursor.row;
        let count = count.min(bottom - cur + 1);
        for _ in 0..count {
            self.visible_rows.remove(bottom);
            self.visible_rows.insert(cur, Row::new(self.cols));
        }
    }

    /// Deletes `count` lines starting at the cursor within the scroll region (`CSI Ps M`, DL).
    pub fn delete_lines(&mut self, count: usize) {
        let top = *self.scroll_region.start();
        let bottom = (*self.scroll_region.end()).min(self.rows.saturating_sub(1));
        if self.cursor.row < top || self.cursor.row > bottom || top >= bottom {
            return;
        }
        self.wrap_pending = false;
        let cur = self.cursor.row;
        let count = count.min(bottom - cur + 1);
        for _ in 0..count {
            self.visible_rows.remove(cur);
            self.visible_rows.insert(bottom, Row::new(self.cols));
        }
    }

    /// Performs a reverse index (`ESC M`, RI).
    ///
    /// Moves cursor up one row, or scrolls the grid down if cursor is at the top margin.
    pub fn reverse_index(&mut self) {
        self.wrap_pending = false;
        let top = *self.scroll_region.start();
        if self.cursor.row == top {
            self.scroll_down(1);
        } else if self.cursor.row > 0 {
            self.cursor.row -= 1;
        }
    }

    /// Performs a carriage return, moving cursor to column 0.
    pub fn carriage_return(&mut self) {
        self.cursor.col = 0;
        self.wrap_pending = false;
    }

    /// Performs a linefeed, advancing cursor down one line or scrolling if at bottom margin.
    pub fn linefeed(&mut self) {
        self.wrap_pending = false;
        let top = *self.scroll_region.start();
        let bottom = (*self.scroll_region.end()).min(self.rows.saturating_sub(1));
        if self.cursor.row == bottom {
            self.scroll_up(1);
        } else if self.cursor.row + 1 < self.rows {
            self.cursor.row += 1;
        } else if self.cursor.row >= self.rows.saturating_sub(1)
            && top == 0
            && bottom >= self.rows.saturating_sub(1)
        {
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

    #[test]
    fn scroll_region_and_insert_delete_lines() {
        let mut grid = Grid::new(5, 5);
        let digits = ['0', '1', '2', '3', '4'];
        for (r, &d) in digits.iter().enumerate() {
            grid[r][0].ch = d;
        }

        // Set scroll region to rows 1..=3
        grid.set_scroll_region(1, 3);
        assert_eq!(*grid.scroll_region.start(), 1);
        assert_eq!(*grid.scroll_region.end(), 3);
        assert_eq!(grid.cursor.row, 0);
        assert_eq!(grid.cursor.col, 0);

        // Scroll up in region: row 0 and 4 untouched, rows 1..=3 shift up
        // Before: [0: '0', 1: '1', 2: '2', 3: '3', 4: '4']
        grid.scroll_up(1);
        assert_eq!(grid[0][0].ch, '0');
        assert_eq!(grid[1][0].ch, '2');
        assert_eq!(grid[2][0].ch, '3');
        assert!(grid[3][0].is_empty());
        assert_eq!(grid[4][0].ch, '4');

        // Scroll down in region: row 0 and 4 untouched, rows 1..=3 shift down
        grid.scroll_down(1);
        assert_eq!(grid[0][0].ch, '0');
        assert!(grid[1][0].is_empty());
        assert_eq!(grid[2][0].ch, '2');
        assert_eq!(grid[3][0].ch, '3');
        assert_eq!(grid[4][0].ch, '4');

        // Insert line at row 2
        grid.move_cursor_to(2, 0);
        grid.insert_lines(1);
        assert_eq!(grid[0][0].ch, '0');
        assert!(grid[1][0].is_empty());
        assert!(grid[2][0].is_empty());
        assert_eq!(grid[3][0].ch, '2');
        assert_eq!(grid[4][0].ch, '4');

        // Delete line at row 2
        grid.delete_lines(1);
        assert_eq!(grid[0][0].ch, '0');
        assert!(grid[1][0].is_empty());
        assert_eq!(grid[2][0].ch, '2');
        assert!(grid[3][0].is_empty());
        assert_eq!(grid[4][0].ch, '4');

        // Reset scroll region
        grid.reset_scroll_region();
        assert_eq!(*grid.scroll_region.start(), 0);
        assert_eq!(*grid.scroll_region.end(), 4);
    }

    #[test]
    fn scrollback_ring_buffer_and_clear() {
        use crate::handler::EraseMode;

        let mut grid = Grid::with_scrollback(5, 2, 3);
        grid[0][0].ch = 'A';
        grid[1][0].ch = 'B';

        // Scroll up by 1: row 'A' enters scrollback
        grid.scroll_up(1);
        assert_eq!(grid.scrollback_len(), 1);
        assert_eq!(grid.scrollback_row(0).unwrap()[0].ch, 'A');
        assert_eq!(grid[0][0].ch, 'B');

        // Write 'C' to visible row 1 and scroll up: row 'B' enters scrollback
        grid[1][0].ch = 'C';
        grid.scroll_up(1);
        assert_eq!(grid.scrollback_len(), 2);
        assert_eq!(grid.scrollback_row(1).unwrap()[0].ch, 'B');

        // Write 'D' and scroll up: row 'C' enters scrollback (now 3 rows: A, B, C)
        grid[1][0].ch = 'D';
        grid.scroll_up(1);
        assert_eq!(grid.scrollback_len(), 3);
        assert_eq!(grid.scrollback_row(0).unwrap()[0].ch, 'A');
        assert_eq!(grid.scrollback_row(2).unwrap()[0].ch, 'C');

        // Write 'E' and scroll up: scrollback reaches cap (3), oldest row 'A' evicted
        grid[1][0].ch = 'E';
        grid.scroll_up(1);
        assert_eq!(grid.scrollback_len(), 3);
        assert_eq!(grid.scrollback_row(0).unwrap()[0].ch, 'B');
        assert_eq!(grid.scrollback_row(1).unwrap()[0].ch, 'C');
        assert_eq!(grid.scrollback_row(2).unwrap()[0].ch, 'D');

        // Erase in display with scrollback clear
        grid.erase_in_display(EraseMode::AllIncludingScrollback);
        assert_eq!(grid.scrollback_len(), 0);
        assert!(grid[0][0].is_empty());
        assert!(grid[1][0].is_empty());
    }

    #[test]
    fn wide_characters_and_overwrite_handling() {
        let mut grid = Grid::new(4, 3);

        // Write '語' (width 2) at col 0
        grid.put_char('語', Color::Default, Color::Default, Attrs::empty(), true);
        assert_eq!(grid[0][0].ch, '語');
        assert_eq!(grid[0][0].width, 2);
        assert!(grid[0][0].is_wide_lead());
        assert_eq!(grid[0][1].ch, ' ');
        assert_eq!(grid[0][1].width, 0);
        assert!(grid[0][1].is_wide_continuation());
        assert_eq!(grid.cursor.col, 2);

        // Write '🦀' (width 2) at col 2
        grid.put_char('🦀', Color::Default, Color::Default, Attrs::empty(), true);
        assert_eq!(grid[0][2].ch, '🦀');
        assert_eq!(grid[0][2].width, 2);
        assert_eq!(grid[0][3].width, 0);
        assert!(grid.wrap_pending);

        // Next character wraps to row 1
        grid.put_char('A', Color::Default, Color::Default, Attrs::empty(), true);
        assert_eq!(grid.cursor.row, 1);
        assert_eq!(grid[1][0].ch, 'A');
        assert_eq!(grid[1][0].width, 1);

        // Wide character placed at last column (col 3) when autowrap is true
        // should pad col 3 and wrap to row 2
        grid.move_cursor_to(1, 3);
        grid.put_char('漢', Color::Default, Color::Default, Attrs::empty(), true);
        assert_eq!(grid[1][3].ch, ' ');
        assert_eq!(grid[2][0].ch, '漢');
        assert_eq!(grid[2][0].width, 2);
        assert_eq!(grid[2][1].width, 0);

        // Overwrite continuation cell clears the leader cell
        grid.move_cursor_to(2, 1);
        grid.put_char('X', Color::Default, Color::Default, Attrs::empty(), true);
        assert!(grid[2][0].is_empty());
        assert_eq!(grid[2][1].ch, 'X');
        assert_eq!(grid[2][1].width, 1);

        // Overwrite leader cell clears the continuation cell
        grid.move_cursor_to(0, 0);
        grid.put_char('Z', Color::Default, Color::Default, Attrs::empty(), true);
        assert_eq!(grid[0][0].ch, 'Z');
        assert_eq!(grid[0][0].width, 1);
        assert!(grid[0][1].is_empty());
    }

    #[test]
    fn grid_resize_expand_and_shrink() {
        let mut grid = Grid::new(3, 3);
        grid[0][0].ch = '1';
        grid[1][0].ch = '2';
        grid[2][0].ch = '3';
        grid.move_cursor_to(2, 2);

        // Shrink height to 2 rows: row 0 ('1') pushed to scrollback
        grid.resize(3, 2);
        assert_eq!(grid.rows, 2);
        assert_eq!(grid.scrollback_len(), 1);
        assert_eq!(grid.scrollback_row(0).unwrap()[0].ch, '1');
        assert_eq!(grid[0][0].ch, '2');
        assert_eq!(grid[1][0].ch, '3');
        assert_eq!(grid.cursor.row, 1);
        assert_eq!(grid.cursor.col, 2);

        // Expand height to 4 rows: row '1' pulled from scrollback, row 3 is blank
        grid.resize(3, 4);
        assert_eq!(grid.rows, 4);
        assert_eq!(grid.scrollback_len(), 0);
        assert_eq!(grid[0][0].ch, '1');
        assert_eq!(grid[1][0].ch, '2');
        assert_eq!(grid[2][0].ch, '3');
        assert!(grid[3][0].is_empty());
        assert_eq!(grid.cursor.row, 2);

        // Truncate columns to 2: cursor col clamped
        grid.cursor.col = 2;
        grid.resize(2, 4);
        assert_eq!(grid.cols, 2);
        assert_eq!(grid.cursor.col, 1);

        // Wide character truncated at right boundary
        grid[0][0] = Cell::new('語', Color::Default, Color::Default, Attrs::empty(), 2);
        grid[0][1] = Cell::new(' ', Color::Default, Color::Default, Attrs::empty(), 0);
        grid.resize(1, 4);
        assert!(grid[0][0].is_empty());
    }

    #[test]
    fn insert_delete_blank_chars() {
        let mut grid = Grid::new(5, 2);
        grid[0][0].ch = 'A';
        grid[0][1].ch = 'B';
        grid[0][2].ch = 'C';
        grid[0][3].ch = 'D';
        grid[0][4].ch = 'E';

        // Insert 2 blank characters at col 1
        grid.move_cursor_to(0, 1);
        grid.insert_blank_chars(2);
        assert_eq!(grid[0][0].ch, 'A');
        assert!(grid[0][1].is_empty());
        assert!(grid[0][2].is_empty());
        assert_eq!(grid[0][3].ch, 'B');
        assert_eq!(grid[0][4].ch, 'C');

        // Delete 2 characters at col 1
        grid.delete_chars(2);
        assert_eq!(grid[0][0].ch, 'A');
        assert_eq!(grid[0][1].ch, 'B');
        assert_eq!(grid[0][2].ch, 'C');
        assert!(grid[0][3].is_empty());
        assert!(grid[0][4].is_empty());
    }
}
