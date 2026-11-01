//! Hyperlink and file path detection, hover matching, and system launching.

use crate::grid::{Grid, Row};
use crate::renderer::draw::{QuadVertex, build_quad_vertices};

/// A detected URL or file path span within a terminal grid row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedUrl {
    /// Starting column index in the row (inclusive).
    pub start_col: usize,
    /// Ending column index in the row (exclusive).
    pub end_col: usize,
    /// Target URL or path string.
    pub url: String,
    /// Whether this URL was explicitly registered via an OSC 8 escape sequence.
    pub is_osc8: bool,
}

/// Regular expression pattern for detecting standard HTTP(S) and file URLs.
const URL_REGEX_PATTERN: &str = r#"(https?|file)://[^\s<>"'()\[\]{}]+"#;

/// Scans a grid row for both OSC 8 hyperlinks and heuristic/regex URL patterns.
#[must_use]
pub fn find_urls_in_row(row: &Row, grid: &Grid) -> Vec<DetectedUrl> {
    let mut detected = Vec::new();

    // 1. Gather explicit OSC 8 hyperlink spans
    let mut current_link_id: u16 = 0;
    let mut link_start_col: usize = 0;

    for (col, cell) in row.iter().enumerate() {
        if cell.hyperlink_id != current_link_id {
            if current_link_id > 0
                && let Some(url) = grid.lookup_hyperlink(current_link_id)
            {
                detected.push(DetectedUrl {
                    start_col: link_start_col,
                    end_col: col,
                    url: url.to_string(),
                    is_osc8: true,
                });
            }
            current_link_id = cell.hyperlink_id;
            link_start_col = col;
        }
    }

    if current_link_id > 0
        && let Some(url) = grid.lookup_hyperlink(current_link_id)
    {
        detected.push(DetectedUrl {
            start_col: link_start_col,
            end_col: row.len(),
            url: url.to_string(),
            is_osc8: true,
        });
    }

    // 2. Scan text for regex-based URL patterns
    let mut line_text = String::with_capacity(row.len());
    let mut col_indices = Vec::with_capacity(row.len());

    let last_content_col = row
        .iter()
        .rposition(|c| !c.is_empty())
        .map_or(0, |idx| idx + 1);

    for (col_idx, cell) in row.iter().enumerate().take(last_content_col) {
        if cell.width == 0 {
            continue;
        }
        let byte_pos = line_text.len();
        line_text.push(cell.ch);
        col_indices.push((byte_pos, col_idx));
    }

    if let Ok(re) = regex::Regex::new(URL_REGEX_PATTERN) {
        for mat in re.find_iter(&line_text) {
            let raw_match = mat.as_str();
            let trimmed =
                raw_match.trim_end_matches(['.', ',', ';', ':', ')', '>', ']', '"', '\'']);
            let trimmed_len = trimmed.len();
            if trimmed_len == 0 {
                continue;
            }

            let byte_start = mat.start();
            let byte_end = byte_start + trimmed_len;

            // Map byte span back to grid columns
            let start_entry = col_indices.iter().find(|&&(b, _)| b >= byte_start);
            let end_entry = col_indices.iter().rev().find(|&&(b, _)| b < byte_end);

            if let (Some(&(_, start_col)), Some(&(last_b, last_col))) = (start_entry, end_entry) {
                let char_at = line_text[last_b..].chars().next().unwrap_or(' ');
                let char_w = unicode_width::UnicodeWidthChar::width(char_at)
                    .unwrap_or(1)
                    .max(1);
                let end_col = (last_col + char_w).min(row.len());

                // Don't add if already covered by an OSC 8 hyperlink
                let overlaps_osc8 = detected
                    .iter()
                    .any(|d| d.is_osc8 && !(end_col <= d.start_col || start_col >= d.end_col));

                if !overlaps_osc8 {
                    detected.push(DetectedUrl {
                        start_col,
                        end_col,
                        url: trimmed.to_string(),
                        is_osc8: false,
                    });
                }
            }
        }
    }

    detected.sort_by_key(|d| d.start_col);
    detected
}

/// Finds the URL (if any) at a specific viewport coordinate `(viewport_row, col)`.
#[must_use]
pub fn find_url_at_coord(grid: &Grid, viewport_row: usize, col: usize) -> Option<DetectedUrl> {
    let row = grid.display_row(viewport_row)?;
    let urls = find_urls_in_row(row, grid);
    urls.into_iter()
        .find(|u| col >= u.start_col && col < u.end_col)
}

/// Opens a URL or file path using the default platform handler.
pub fn open_url(url: &str) -> Result<(), std::io::Error> {
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()?;
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    Ok(())
}

/// Builds quad vertices for rendering an underline under a hovered URL.
#[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
#[must_use]
pub fn build_url_hover_underline_vertices(
    row: usize,
    start_col: usize,
    end_col: usize,
    cell_width: f32,
    cell_height: f32,
    x_offset: f32,
    y_offset: f32,
    color: [f32; 4],
) -> Option<[QuadVertex; 6]> {
    if start_col >= end_col {
        return None;
    }
    let x = x_offset + start_col as f32 * cell_width;
    let w = (end_col - start_col) as f32 * cell_width;
    let underline_thickness = 1.5;
    let y = y_offset + (row as f32 + 1.0) * cell_height - underline_thickness;
    Some(build_quad_vertices(x, y, w, underline_thickness, color))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{Attrs, Color};

    #[test]
    fn test_regex_url_detection() {
        let mut grid = Grid::new(40, 3);
        let text = "Visit https://oxidite.dev for docs.";
        for ch in text.chars() {
            grid.put_char(ch, Color::Default, Color::Default, Attrs::empty(), true);
        }

        let row = grid.row(0).unwrap();
        let urls = find_urls_in_row(row, &grid);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].url, "https://oxidite.dev");
        assert_eq!(urls[0].start_col, 6);
        assert_eq!(urls[0].end_col, 25);
        assert!(!urls[0].is_osc8);
    }

    #[test]
    fn test_osc8_hyperlink_detection() {
        let mut grid = Grid::new(30, 3);
        let link_id = grid.register_hyperlink("https://github.com".to_string());
        for ch in "GitHub".chars() {
            grid.put_char_with_link(
                ch,
                Color::Default,
                Color::Default,
                Attrs::empty(),
                true,
                link_id,
            );
        }
        for ch in " rocks".chars() {
            grid.put_char_with_link(ch, Color::Default, Color::Default, Attrs::empty(), true, 0);
        }

        let row = grid.row(0).unwrap();
        let urls = find_urls_in_row(row, &grid);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].url, "https://github.com");
        assert_eq!(urls[0].start_col, 0);
        assert_eq!(urls[0].end_col, 6);
        assert!(urls[0].is_osc8);
    }

    #[test]
    fn test_find_url_at_coord() {
        let mut grid = Grid::new(40, 3);
        let text = "Check https://rust-lang.org now";
        for ch in text.chars() {
            grid.put_char(ch, Color::Default, Color::Default, Attrs::empty(), true);
        }

        assert!(find_url_at_coord(&grid, 0, 2).is_none());
        let found = find_url_at_coord(&grid, 0, 10);
        assert!(found.is_some());
        assert_eq!(found.unwrap().url, "https://rust-lang.org");
    }

    #[test]
    fn test_build_url_hover_underline_vertices() {
        let quads = build_url_hover_underline_vertices(
            1,
            5,
            15,
            10.0,
            20.0,
            0.0,
            0.0,
            [0.3, 0.6, 1.0, 1.0],
        );
        // build_quad_vertices produces 6 vertices (2 triangles) per quad
        assert!(quads.is_some());
        assert_eq!(quads.unwrap().len(), 6);
        let empty_quads = build_url_hover_underline_vertices(
            1,
            15,
            5,
            10.0,
            20.0,
            0.0,
            0.0,
            [0.3, 0.6, 1.0, 1.0],
        );
        assert!(empty_quads.is_none());
    }
}
