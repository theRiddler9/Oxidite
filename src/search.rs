//! In-terminal search engine, state, highlighting, and UI quad generation.

use crate::grid::Grid;
use crate::renderer::draw::{QuadVertex, TabLabel, build_quad_vertices};
use glyphon::Color as GlyphonColor;

/// A single search match location on the terminal grid or in scrollback history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchMatch {
    /// Absolute row index: `0..scrollback.len()` is scrollback, `scrollback.len()..` is visible screen.
    pub abs_row: usize,
    /// Starting column index in the row (inclusive).
    pub start_col: usize,
    /// Ending column index in the row (exclusive).
    pub end_col: usize,
}

/// Runtime state for in-terminal scrollback and visible grid search.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchState {
    /// Whether the search bar overlay is currently open and focused.
    pub is_open: bool,
    /// Active search query string.
    pub query: String,
    /// Whether to interpret the query as a regular expression.
    pub is_regex: bool,
    /// Whether matching is case-sensitive.
    pub case_sensitive: bool,
    /// All matches found across scrollback and visible rows, ordered from top to bottom.
    pub matches: Vec<SearchMatch>,
    /// Index of the currently focused match in `matches`.
    pub current_match_idx: Option<usize>,
}

impl SearchState {
    /// Creates a new inactive search state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens the search bar and focuses the input.
    pub fn open(&mut self) {
        self.is_open = true;
    }

    /// Closes the search bar and clears match state.
    pub fn close(&mut self) {
        self.is_open = false;
        self.matches.clear();
        self.current_match_idx = None;
    }

    /// Toggles the search bar open or closed.
    pub fn toggle(&mut self) {
        if self.is_open {
            self.close();
        } else {
            self.open();
        }
    }

    /// Returns the currently active match, if any.
    #[must_use]
    pub fn current_match(&self) -> Option<SearchMatch> {
        self.current_match_idx
            .and_then(|idx| self.matches.get(idx).copied())
    }

    /// Returns total match count.
    #[must_use]
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// Navigates to the next match, wrapping to the start. Returns the newly selected match.
    pub fn next_match(&mut self) -> Option<SearchMatch> {
        if self.matches.is_empty() {
            self.current_match_idx = None;
            return None;
        }
        let next_idx = match self.current_match_idx {
            Some(idx) => (idx + 1) % self.matches.len(),
            None => 0,
        };
        self.current_match_idx = Some(next_idx);
        self.matches.get(next_idx).copied()
    }

    /// Navigates to the previous match, wrapping to the end. Returns the newly selected match.
    pub fn prev_match(&mut self) -> Option<SearchMatch> {
        if self.matches.is_empty() {
            self.current_match_idx = None;
            return None;
        }
        let prev_idx = match self.current_match_idx {
            Some(idx) => (idx + self.matches.len() - 1) % self.matches.len(),
            None => self.matches.len().saturating_sub(1),
        };
        self.current_match_idx = Some(prev_idx);
        self.matches.get(prev_idx).copied()
    }

    /// Executes search against `grid` scrollback and visible rows.
    /// Updates `matches` and preserves or selects the closest match.
    pub fn execute_search(&mut self, grid: &Grid) {
        self.matches.clear();

        if self.query.is_empty() {
            self.current_match_idx = None;
            return;
        }

        let total_scrollback = grid.scrollback_len();
        let total_lines = total_scrollback + grid.rows;

        if self.is_regex {
            let regex_result = regex::RegexBuilder::new(&self.query)
                .case_insensitive(!self.case_sensitive)
                .build();

            let Ok(re) = regex_result else {
                self.current_match_idx = None;
                return;
            };

            for abs_row in 0..total_lines {
                let row_opt = if abs_row < total_scrollback {
                    grid.scrollback_row(abs_row)
                } else {
                    grid.visible_rows.get(abs_row - total_scrollback)
                };

                let Some(row) = row_opt else { continue };
                let (line_text, col_indices) = extract_row_text_and_cols(row);

                for mat in re.find_iter(&line_text) {
                    if let Some(search_match) = map_char_span_to_match(
                        abs_row,
                        mat.start(),
                        mat.end(),
                        &line_text,
                        &col_indices,
                        row.len(),
                    ) {
                        self.matches.push(search_match);
                    }
                }
            }
        } else {
            let query_str = if self.case_sensitive {
                self.query.clone()
            } else {
                self.query.to_lowercase()
            };

            for abs_row in 0..total_lines {
                let row_opt = if abs_row < total_scrollback {
                    grid.scrollback_row(abs_row)
                } else {
                    grid.visible_rows.get(abs_row - total_scrollback)
                };

                let Some(row) = row_opt else { continue };
                let (line_text, col_indices) = extract_row_text_and_cols(row);

                let haystack = if self.case_sensitive {
                    line_text.clone()
                } else {
                    line_text.to_lowercase()
                };

                let mut start_byte = 0;
                while let Some(found_offset) = haystack[start_byte..].find(&query_str) {
                    let match_byte_start = start_byte + found_offset;
                    let match_byte_end = match_byte_start + query_str.len();

                    if let Some(search_match) = map_char_span_to_match(
                        abs_row,
                        match_byte_start,
                        match_byte_end,
                        &line_text,
                        &col_indices,
                        row.len(),
                    ) {
                        self.matches.push(search_match);
                    }

                    // Advance by at least 1 character/byte to find overlapping/subsequent matches
                    if let Some(next_char) = haystack[match_byte_start..].chars().next() {
                        start_byte = match_byte_start + next_char.len_utf8();
                    } else {
                        break;
                    }
                }
            }
        }

        if self.matches.is_empty() {
            self.current_match_idx = None;
        } else {
            // Default to the last match (closest to prompt) or keep index in range
            self.current_match_idx = Some(
                self.current_match_idx
                    .map_or(self.matches.len().saturating_sub(1), |idx| {
                        idx.min(self.matches.len().saturating_sub(1))
                    }),
            );
        }
    }

    /// Scrolls `grid` viewport so that the currently focused match is visible.
    pub fn scroll_to_current_match(&self, grid: &mut Grid) {
        let Some(m) = self.current_match() else {
            return;
        };
        let sb_len = grid.scrollback_len();

        if m.abs_row >= sb_len {
            // Match is in the visible screen rows
            grid.display_offset = 0;
        } else {
            // Match is in scrollback history
            // Position the match approximately in the middle third of the screen
            let target_viewport_row = (grid.rows / 3).max(1);
            let needed_offset = (sb_len + target_viewport_row).saturating_sub(m.abs_row);
            grid.display_offset = needed_offset.clamp(1, sb_len);
        }
    }
}

/// Extracts printable characters and their corresponding grid column indices for a `Row`.
fn extract_row_text_and_cols(row: &crate::grid::Row) -> (String, Vec<(usize, usize)>) {
    let mut text = String::with_capacity(row.len());
    let mut col_indices = Vec::with_capacity(row.len());

    // Find last non-empty column to avoid trailing blank noise
    let last_content_col = row
        .iter()
        .rposition(|c| !c.is_empty())
        .map_or(0, |idx| idx + 1);

    for (col_idx, cell) in row.iter().enumerate().take(last_content_col) {
        if cell.width == 0 {
            // Continuation cell of a wide character
            continue;
        }
        let byte_start = text.len();
        text.push(cell.ch);
        col_indices.push((byte_start, col_idx));
    }

    (text, col_indices)
}

/// Maps a byte range within `line_text` to column start and end on the grid.
fn map_char_span_to_match(
    abs_row: usize,
    byte_start: usize,
    byte_end: usize,
    line_text: &str,
    col_indices: &[(usize, usize)],
    row_len: usize,
) -> Option<SearchMatch> {
    if col_indices.is_empty() {
        return None;
    }

    // Find the starting column
    let start_entry = col_indices
        .iter()
        .find(|&&(b, _)| b >= byte_start)
        .or_else(|| col_indices.last())?;
    let start_col = start_entry.1;

    // Find the ending column
    let end_col = if let Some(last_entry) = col_indices.iter().rev().find(|&&(b, _)| b < byte_end) {
        // Add character width for the last character
        let char_at = line_text[last_entry.0..].chars().next().unwrap_or(' ');
        let char_w = unicode_width::UnicodeWidthChar::width(char_at)
            .unwrap_or(1)
            .max(1);
        (last_entry.1 + char_w).min(row_len)
    } else {
        (start_col + 1).min(row_len)
    };

    Some(SearchMatch {
        abs_row,
        start_col,
        end_col: end_col.max(start_col + 1),
    })
}

/// Generates quad vertices to highlight all search matches visible in the current viewport.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn build_search_highlight_vertices(
    grid: &Grid,
    search: &SearchState,
    cell_width: f32,
    cell_height: f32,
    x_offset: f32,
    y_offset: f32,
) -> Vec<QuadVertex> {
    let mut vertices = Vec::new();
    if !search.is_open || search.matches.is_empty() {
        return vertices;
    }

    let sb_len = grid.scrollback_len();
    let display_offset = grid.display_offset();

    // Normal match highlight: warm amber/yellow tint
    let normal_color = [0.95, 0.78, 0.22, 0.38];
    // Active focused match highlight: bright solid orange/gold
    let active_color = [1.0, 0.58, 0.0, 0.82];

    for (idx, m) in search.matches.iter().enumerate() {
        let is_current = search.current_match_idx == Some(idx);
        let color = if is_current {
            active_color
        } else {
            normal_color
        };

        // Convert abs_row to viewport row (0..grid.rows)
        // Formula: abs_row = (sb_len + viewport_row) - display_offset
        // Therefore: viewport_row = (abs_row + display_offset) - sb_len
        let viewport_calc = (m.abs_row + display_offset).checked_sub(sb_len);
        let Some(viewport_row) = viewport_calc else {
            continue;
        };

        if viewport_row >= grid.rows {
            continue;
        }

        let col_start = m.start_col.min(grid.cols.saturating_sub(1));
        let col_end = m.end_col.min(grid.cols);
        if col_start >= col_end {
            continue;
        }

        let x = x_offset + (col_start as f32) * cell_width;
        let y = y_offset + (viewport_row as f32) * cell_height;
        let w = ((col_end - col_start) as f32) * cell_width;
        let h = cell_height;

        vertices.extend_from_slice(&build_quad_vertices(x, y, w, h, color));
    }

    vertices
}

/// Width and height constants for the floating search bar overlay.
pub const SEARCH_BAR_WIDTH: f32 = 320.0;
pub const SEARCH_BAR_HEIGHT: f32 = 34.0;

/// Generates background and border quad vertices for the floating search bar.
#[must_use]
pub fn assemble_search_bar_quads(
    pane_x: f32,
    pane_y: f32,
    pane_w: f32,
    accent_color: [f32; 4],
    search: &SearchState,
) -> Vec<QuadVertex> {
    let mut quads = Vec::new();
    if !search.is_open {
        return quads;
    }

    // Position search bar at top-right of the terminal pane
    let bar_w = SEARCH_BAR_WIDTH.min(pane_w - 20.0);
    let bar_h = SEARCH_BAR_HEIGHT;
    let bar_x = pane_x + pane_w - bar_w - 16.0;
    let bar_y = pane_y + 8.0;

    // 1. Drop shadow / subtle dark halo
    quads.extend_from_slice(&build_quad_vertices(
        bar_x - 2.0,
        bar_y - 2.0,
        bar_w + 4.0,
        bar_h + 4.0,
        [0.0, 0.0, 0.0, 0.45],
    ));

    // 2. Main search bar background (deep card surface)
    quads.extend_from_slice(&build_quad_vertices(
        bar_x,
        bar_y,
        bar_w,
        bar_h,
        [0.08, 0.09, 0.12, 0.96],
    ));

    // 3. Accent border around search bar
    let border_color = if search.is_open {
        accent_color
    } else {
        [0.2, 0.22, 0.28, 1.0]
    };
    // Top border
    quads.extend_from_slice(&build_quad_vertices(bar_x, bar_y, bar_w, 1.0, border_color));
    // Bottom border
    quads.extend_from_slice(&build_quad_vertices(
        bar_x,
        bar_y + bar_h - 1.0,
        bar_w,
        1.0,
        border_color,
    ));
    // Left border
    quads.extend_from_slice(&build_quad_vertices(bar_x, bar_y, 1.0, bar_h, border_color));
    // Right border
    quads.extend_from_slice(&build_quad_vertices(
        bar_x + bar_w - 1.0,
        bar_y,
        1.0,
        bar_h,
        border_color,
    ));

    // 4. Input text box background
    let input_x = bar_x + 8.0;
    let input_y = bar_y + 5.0;
    let input_w = bar_w - 120.0;
    let input_h = bar_h - 10.0;
    quads.extend_from_slice(&build_quad_vertices(
        input_x,
        input_y,
        input_w,
        input_h,
        [0.04, 0.045, 0.06, 0.9],
    ));

    // 5. Match badge background (pill)
    let badge_x = bar_x + bar_w - 104.0;
    let badge_y = bar_y + 6.0;
    let badge_w = 46.0;
    let badge_h = bar_h - 12.0;
    let badge_bg = if search.matches.is_empty() && !search.query.is_empty() {
        [0.45, 0.12, 0.12, 0.85] // Red warning for no matches
    } else {
        [0.12, 0.14, 0.18, 0.85]
    };
    quads.extend_from_slice(&build_quad_vertices(
        badge_x, badge_y, badge_w, badge_h, badge_bg,
    ));

    // 6. Mode toggle indicators (Aa case sensitivity, .* regex)
    let opt_x = bar_x + bar_w - 52.0;
    let opt_y = bar_y + 6.0;
    let opt_w = 44.0;
    let opt_h = bar_h - 12.0;
    quads.extend_from_slice(&build_quad_vertices(
        opt_x,
        opt_y,
        opt_w,
        opt_h,
        [0.10, 0.11, 0.15, 0.85],
    ));

    quads
}

/// Generates text labels for the floating search bar (query, match counter, badges).
#[must_use]
pub fn build_search_bar_labels(
    pane_x: f32,
    pane_y: f32,
    pane_w: f32,
    search: &SearchState,
) -> Vec<TabLabel> {
    let mut labels = Vec::new();
    if !search.is_open {
        return labels;
    }

    let bar_w = SEARCH_BAR_WIDTH.min(pane_w - 20.0);
    let bar_x = pane_x + pane_w - bar_w - 16.0;
    let bar_y = pane_y + 8.0;

    // Search query text (or placeholder)
    let (query_display, query_color) = if search.query.is_empty() {
        ("Search...".to_string(), GlyphonColor::rgb(110, 115, 130))
    } else {
        (
            format!("{}▏", search.query),
            GlyphonColor::rgb(240, 242, 250),
        )
    };

    labels.push(TabLabel {
        text: query_display,
        x: bar_x + 14.0,
        y: bar_y + 8.0,
        color: query_color,
        bold: false,
        max_width: bar_w - 130.0,
    });

    // Match count badge: "[1/12]" or "0/0"
    let count_text = if search.query.is_empty() {
        "—".to_string()
    } else if search.matches.is_empty() {
        "0/0".to_string()
    } else {
        let current_num = search.current_match_idx.map_or(0, |i| i + 1);
        format!("{}/{}", current_num, search.matches.len())
    };

    let count_color = if search.matches.is_empty() && !search.query.is_empty() {
        GlyphonColor::rgb(255, 120, 120)
    } else {
        GlyphonColor::rgb(180, 190, 210)
    };

    labels.push(TabLabel {
        text: count_text,
        x: bar_x + bar_w - 100.0,
        y: bar_y + 9.0,
        color: count_color,
        bold: true,
        max_width: 44.0,
    });

    // Mode toggles: Case (Aa) and Regex (.*)
    let case_color = if search.case_sensitive {
        GlyphonColor::rgb(80, 210, 255)
    } else {
        GlyphonColor::rgb(100, 105, 120)
    };
    labels.push(TabLabel {
        text: "Aa".to_string(),
        x: bar_x + bar_w - 48.0,
        y: bar_y + 9.0,
        color: case_color,
        bold: search.case_sensitive,
        max_width: 18.0,
    });

    let regex_color = if search.is_regex {
        GlyphonColor::rgb(80, 210, 255)
    } else {
        GlyphonColor::rgb(100, 105, 120)
    };
    labels.push(TabLabel {
        text: ".*".to_string(),
        x: bar_x + bar_w - 26.0,
        y: bar_y + 9.0,
        color: regex_color,
        bold: search.is_regex,
        max_width: 18.0,
    });

    labels
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{Attrs, Color};

    #[test]
    fn search_state_open_close_toggle() {
        let mut state = SearchState::new();
        assert!(!state.is_open);

        state.open();
        assert!(state.is_open);

        state.toggle();
        assert!(!state.is_open);

        state.toggle();
        assert!(state.is_open);

        state.close();
        assert!(!state.is_open);
    }

    #[test]
    fn search_literal_case_insensitive() {
        let mut grid = Grid::new(20, 5);
        grid.put_char('H', Color::Default, Color::Default, Attrs::empty(), true);
        grid.put_char('e', Color::Default, Color::Default, Attrs::empty(), true);
        grid.put_char('l', Color::Default, Color::Default, Attrs::empty(), true);
        grid.put_char('l', Color::Default, Color::Default, Attrs::empty(), true);
        grid.put_char('o', Color::Default, Color::Default, Attrs::empty(), true);

        let mut state = SearchState::new();
        state.query = "hello".to_string();
        state.case_sensitive = false;
        state.is_regex = false;

        state.execute_search(&grid);
        assert_eq!(state.match_count(), 1);
        let m = state.current_match().unwrap();
        assert_eq!(m.abs_row, 0);
        assert_eq!(m.start_col, 0);
        assert_eq!(m.end_col, 5);
    }

    #[test]
    fn search_literal_case_sensitive() {
        let mut grid = Grid::new(20, 5);
        grid.put_char('H', Color::Default, Color::Default, Attrs::empty(), true);
        grid.put_char('e', Color::Default, Color::Default, Attrs::empty(), true);
        grid.put_char('l', Color::Default, Color::Default, Attrs::empty(), true);
        grid.put_char('l', Color::Default, Color::Default, Attrs::empty(), true);
        grid.put_char('o', Color::Default, Color::Default, Attrs::empty(), true);

        let mut state = SearchState::new();
        state.query = "hello".to_string();
        state.case_sensitive = true;
        state.is_regex = false;

        state.execute_search(&grid);
        assert_eq!(state.match_count(), 0);

        state.query = "Hello".to_string();
        state.execute_search(&grid);
        assert_eq!(state.match_count(), 1);
    }

    #[test]
    fn search_regex_pattern() {
        let mut grid = Grid::new(30, 5);
        let text = "error: code 404 not found";
        for ch in text.chars() {
            grid.put_char(ch, Color::Default, Color::Default, Attrs::empty(), true);
        }

        let mut state = SearchState::new();
        state.query = r"\d{3}".to_string();
        state.is_regex = true;

        state.execute_search(&grid);
        assert_eq!(state.match_count(), 1);
        let m = state.current_match().unwrap();
        assert_eq!(m.start_col, 12);
        assert_eq!(m.end_col, 15);
    }

    #[test]
    fn search_across_scrollback_and_visible() {
        let mut grid = Grid::with_scrollback(20, 2, 10);
        // Put text in row 0
        grid.put_char('T', Color::Default, Color::Default, Attrs::empty(), true);
        grid.put_char('o', Color::Default, Color::Default, Attrs::empty(), true);
        grid.put_char('p', Color::Default, Color::Default, Attrs::empty(), true);

        // Scroll up to push into scrollback
        grid.linefeed();
        grid.linefeed();
        grid.linefeed();
        assert!(grid.scrollback_len() >= 1);

        // Put text on visible screen
        grid.put_char('B', Color::Default, Color::Default, Attrs::empty(), true);
        grid.put_char('o', Color::Default, Color::Default, Attrs::empty(), true);
        grid.put_char('t', Color::Default, Color::Default, Attrs::empty(), true);

        let mut state = SearchState::new();
        state.query = "o".to_string();
        state.execute_search(&grid);

        // 'o' in "Top" (scrollback) and 'o' in "Bot" (visible)
        assert_eq!(state.match_count(), 2);

        // Next/prev navigation
        let first = state.matches[0];
        let second = state.matches[1];
        state.current_match_idx = Some(0);
        assert_eq!(state.next_match(), Some(second));
        assert_eq!(state.next_match(), Some(first)); // wraps
        assert_eq!(state.prev_match(), Some(second)); // wraps back
    }

    #[test]
    fn search_auto_scroll_calculates_correct_offset() {
        let mut grid = Grid::with_scrollback(20, 4, 20);
        // Push 10 lines into scrollback
        for _ in 0..12 {
            grid.linefeed();
        }
        let sb_len = grid.scrollback_len();
        assert!(sb_len >= 8);

        let mut state = SearchState::new();
        // Suppose match is at abs_row 2 (in scrollback)
        state.matches.push(SearchMatch {
            abs_row: 2,
            start_col: 0,
            end_col: 4,
        });
        state.current_match_idx = Some(0);

        state.scroll_to_current_match(&mut grid);
        assert!(grid.display_offset > 0);
        assert!(grid.display_offset <= sb_len);
    }
}
