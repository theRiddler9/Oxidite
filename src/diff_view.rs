//! Side-by-side code diff viewer workspace (visual center pane).

use crate::renderer::draw::{QuadVertex, TabLabel, build_quad_vertices};
use glyphon::Color as GlyphonColor;

/// Height of the top diff header in logical pixels.
pub const DIFF_HEADER_HEIGHT: f32 = 34.0;
/// Line height in logical pixels for each diff code line.
pub const DIFF_LINE_HEIGHT: f32 = 20.0;
/// Width in logical pixels of the line number gutter on each column.
pub const DIFF_GUTTER_WIDTH: f32 = 42.0;

/// Type of line in a side-by-side code diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineType {
    /// Neutral unchanged line.
    Context,
    /// Added line (green highlight).
    Addition,
    /// Removed line (red highlight).
    Deletion,
    /// Blank line for vertical alignment with opposite column.
    Empty,
}

/// A single line of text with line number and diff category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// Line number in file, if not an empty spacer line.
    pub line_number: Option<usize>,
    /// Text content of this line.
    pub content: String,
    /// Diff status (addition, deletion, context, empty).
    pub line_type: DiffLineType,
}

/// A contiguous diff hunk containing original (left) and modified (right) lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    /// Hunk header description (e.g. "@@ -1,9 +1,10 @@").
    pub header: String,
    /// Lines on the left (base/original) column.
    pub left_lines: Vec<DiffLine>,
    /// Lines on the right (modified) column.
    pub right_lines: Vec<DiffLine>,
}

/// State for the side-by-side diff review workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffViewState {
    /// Active file name being reviewed (e.g. "sw.js").
    pub file_name: String,
    /// Path / folder metadata (e.g. "ExamCooker-2024/public/").
    pub path_info: String,
    /// Diff hunks for this file.
    pub hunks: Vec<DiffHunk>,
    /// Scroll offset in lines.
    pub scroll_offset: usize,
    /// Whether this file is currently staged.
    pub staged: bool,
}

impl Default for DiffViewState {
    #[allow(clippy::too_many_lines)]
    fn default() -> Self {
        Self {
            file_name: "sw.js".to_string(),
            path_info: "ExamCooker-2024/public/".to_string(),
            hunks: vec![DiffHunk {
                header: "@@ -1,9 +1,10 @@".to_string(),
                left_lines: vec![
                    DiffLine {
                        line_number: Some(1),
                        content: "const CACHE_NAME = 'v1.0.4';".to_string(),
                        line_type: DiffLineType::Deletion,
                    },
                    DiffLine {
                        line_number: Some(2),
                        content: "const STATIC_ASSETS = [".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(3),
                        content: "  '/',".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(4),
                        content: "  '/index.html',".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(5),
                        content: "  '/bundle.js',".to_string(),
                        line_type: DiffLineType::Deletion,
                    },
                    DiffLine {
                        line_number: None,
                        content: String::new(),
                        line_type: DiffLineType::Empty,
                    },
                    DiffLine {
                        line_number: Some(6),
                        content: "];".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(7),
                        content: "self.addEventListener('install', (e) => {".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(8),
                        content: "  e.waitUntil(caches.open(CACHE_NAME));".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(9),
                        content: "});".to_string(),
                        line_type: DiffLineType::Context,
                    },
                ],
                right_lines: vec![
                    DiffLine {
                        line_number: Some(1),
                        content: "const CACHE_NAME = 'v1.0.5-beta';".to_string(),
                        line_type: DiffLineType::Addition,
                    },
                    DiffLine {
                        line_number: Some(2),
                        content: "const STATIC_ASSETS = [".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(3),
                        content: "  '/',".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(4),
                        content: "  '/index.html',".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(5),
                        content: "  '/bundle.min.js',".to_string(),
                        line_type: DiffLineType::Addition,
                    },
                    DiffLine {
                        line_number: Some(6),
                        content: "  '/vendor.wasm',".to_string(),
                        line_type: DiffLineType::Addition,
                    },
                    DiffLine {
                        line_number: Some(7),
                        content: "];".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(8),
                        content: "self.addEventListener('install', (e) => {".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(9),
                        content: "  e.waitUntil(caches.open(CACHE_NAME));".to_string(),
                        line_type: DiffLineType::Context,
                    },
                    DiffLine {
                        line_number: Some(10),
                        content: "});".to_string(),
                        line_type: DiffLineType::Context,
                    },
                ],
            }],
            scroll_offset: 0,
            staged: true,
        }
    }
}

/// Interactive actions dispatched by clicking inside the diff view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffAction {
    /// No interactive control was clicked.
    None,
    /// Open the current file in external editor.
    OpenFile,
    /// Stage or unstage the current file.
    ToggleStage,
    /// Restore / discard changes to the current file.
    Restore,
    /// Close the diff view.
    Close,
}

/// Builds quads for the side-by-side diff: pitch black background, column gutters,
/// center divider, and green/red tinted line highlight bars.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::too_many_lines
)]
pub fn assemble_diff_view_quads(
    diff_x: f32,
    diff_y: f32,
    diff_width: f32,
    diff_height: f32,
    accent_color: [f32; 4],
    state: &DiffViewState,
) -> Vec<QuadVertex> {
    let mut quads = Vec::new();

    // 1. Pure pitch black workspace background (#000000)
    quads.extend_from_slice(&build_quad_vertices(
        diff_x,
        diff_y,
        diff_width,
        diff_height,
        [0.0, 0.0, 0.0, 1.0],
    ));

    // 2. Left 1px border dividing from terminal / agent pane (#181a20)
    quads.extend_from_slice(&build_quad_vertices(
        diff_x,
        diff_y,
        1.0,
        diff_height,
        [0.095, 0.105, 0.13, 1.0],
    ));

    // 3. Top header bar background and bottom 1px divider (#181a20)
    quads.extend_from_slice(&build_quad_vertices(
        diff_x,
        diff_y,
        diff_width,
        DIFF_HEADER_HEIGHT,
        [0.025, 0.028, 0.038, 0.95],
    ));
    quads.extend_from_slice(&build_quad_vertices(
        diff_x,
        diff_y + DIFF_HEADER_HEIGHT - 1.0,
        diff_width,
        1.0,
        [0.095, 0.105, 0.13, 1.0],
    ));

    // Active tab underline indicator
    quads.extend_from_slice(&build_quad_vertices(
        diff_x + 8.0,
        diff_y + DIFF_HEADER_HEIGHT - 2.0,
        120.0,
        2.0,
        accent_color,
    ));

    // Action button backgrounds in header
    let btn_y = diff_y + 5.0;
    // Stage button
    quads.extend_from_slice(&build_quad_vertices(
        diff_x + diff_width - 170.0,
        btn_y,
        50.0,
        24.0,
        [0.06, 0.07, 0.09, 0.9],
    ));
    // Open File button
    quads.extend_from_slice(&build_quad_vertices(
        diff_x + diff_width - 115.0,
        btn_y,
        65.0,
        24.0,
        [0.06, 0.07, 0.09, 0.9],
    ));
    // Close button
    quads.extend_from_slice(&build_quad_vertices(
        diff_x + diff_width - 45.0,
        btn_y,
        24.0,
        24.0,
        [0.06, 0.07, 0.09, 0.9],
    ));

    // 4. Side-by-side columns geometry
    let content_y = diff_y + DIFF_HEADER_HEIGHT;
    let content_h = (diff_height - DIFF_HEADER_HEIGHT).max(0.0);
    let half_w = (diff_width / 2.0).floor();

    // Center vertical split line
    quads.extend_from_slice(&build_quad_vertices(
        diff_x + half_w,
        content_y,
        1.0,
        content_h,
        [0.095, 0.105, 0.13, 1.0],
    ));

    // Left gutter background
    quads.extend_from_slice(&build_quad_vertices(
        diff_x,
        content_y,
        DIFF_GUTTER_WIDTH,
        content_h,
        [0.015, 0.018, 0.024, 0.95],
    ));
    // Left gutter divider
    quads.extend_from_slice(&build_quad_vertices(
        diff_x + DIFF_GUTTER_WIDTH,
        content_y,
        1.0,
        content_h,
        [0.07, 0.075, 0.095, 0.8],
    ));

    // Right gutter background
    quads.extend_from_slice(&build_quad_vertices(
        diff_x + half_w,
        content_y,
        DIFF_GUTTER_WIDTH,
        content_h,
        [0.015, 0.018, 0.024, 0.95],
    ));
    // Right gutter divider
    quads.extend_from_slice(&build_quad_vertices(
        diff_x + half_w + DIFF_GUTTER_WIDTH,
        content_y,
        1.0,
        content_h,
        [0.07, 0.075, 0.095, 0.8],
    ));

    // 5. Line highlight quads for additions and deletions
    let mut current_y = content_y;
    for hunk in &state.hunks {
        // Hunk header bar (subtle dark navy strip)
        quads.extend_from_slice(&build_quad_vertices(
            diff_x,
            current_y,
            diff_width,
            DIFF_LINE_HEIGHT,
            [0.04, 0.05, 0.07, 0.75],
        ));
        current_y += DIFF_LINE_HEIGHT;

        let max_lines = hunk.left_lines.len().max(hunk.right_lines.len());
        for line_idx in 0..max_lines {
            if current_y + DIFF_LINE_HEIGHT > diff_y + diff_height {
                break;
            }

            // Left line highlight
            if let Some(left) = hunk.left_lines.get(line_idx)
                && left.line_type == DiffLineType::Deletion
            {
                // Red tinted deletion bar (--diff-remove-bg: #2b1114)
                quads.extend_from_slice(&build_quad_vertices(
                    diff_x + DIFF_GUTTER_WIDTH + 1.0,
                    current_y,
                    half_w - DIFF_GUTTER_WIDTH - 1.0,
                    DIFF_LINE_HEIGHT,
                    [0.169, 0.067, 0.078, 0.85],
                ));
                // Red gutter tint
                quads.extend_from_slice(&build_quad_vertices(
                    diff_x,
                    current_y,
                    DIFF_GUTTER_WIDTH,
                    DIFF_LINE_HEIGHT,
                    [0.294, 0.09, 0.11, 0.8],
                ));
            }

            // Right line highlight
            if let Some(right) = hunk.right_lines.get(line_idx)
                && right.line_type == DiffLineType::Addition
            {
                // Green tinted addition bar (--diff-add-bg: #102719)
                quads.extend_from_slice(&build_quad_vertices(
                    diff_x + half_w + DIFF_GUTTER_WIDTH + 1.0,
                    current_y,
                    half_w - DIFF_GUTTER_WIDTH - 1.0,
                    DIFF_LINE_HEIGHT,
                    [0.063, 0.153, 0.098, 0.85],
                ));
                // Green gutter tint
                quads.extend_from_slice(&build_quad_vertices(
                    diff_x + half_w,
                    current_y,
                    DIFF_GUTTER_WIDTH,
                    DIFF_LINE_HEIGHT,
                    [0.09, 0.27, 0.16, 0.8],
                ));
            }

            current_y += DIFF_LINE_HEIGHT;
        }
    }

    quads
}

/// Builds text labels for the diff header, line number gutters, and code text.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::too_many_lines
)]
pub fn build_diff_view_labels(
    diff_x: f32,
    diff_y: f32,
    diff_width: f32,
    diff_height: f32,
    state: &DiffViewState,
) -> Vec<TabLabel> {
    let mut labels = Vec::new();

    // 1. Top File Tab and Action Buttons
    let tab_text = format!("{} {}", state.file_name, state.path_info);
    labels.push(TabLabel {
        text: tab_text,
        x: diff_x + 12.0,
        y: diff_y + 8.0,
        color: GlyphonColor::rgb(240, 245, 255),
        bold: true,
        max_width: diff_width - 200.0,
    });

    let btn_y = diff_y + 9.0;
    // Stage button
    let stage_text = if state.staged { "Unstage" } else { "Stage" };
    labels.push(TabLabel {
        text: stage_text.to_string(),
        x: diff_x + diff_width - 165.0,
        y: btn_y,
        color: GlyphonColor::rgb(220, 225, 240),
        bold: false,
        max_width: 45.0,
    });

    // Open File button
    labels.push(TabLabel {
        text: "Open File".to_string(),
        x: diff_x + diff_width - 110.0,
        y: btn_y,
        color: GlyphonColor::rgb(220, 225, 240),
        bold: false,
        max_width: 60.0,
    });

    // Close '×' button
    labels.push(TabLabel {
        text: "×".to_string(),
        x: diff_x + diff_width - 38.0,
        y: btn_y,
        color: GlyphonColor::rgb(160, 165, 180),
        bold: true,
        max_width: 20.0,
    });

    // 2. Diff Lines Rendering
    let content_y = diff_y + DIFF_HEADER_HEIGHT;
    let half_w = (diff_width / 2.0).floor();
    let mut current_y = content_y;

    for hunk in &state.hunks {
        // Hunk header text e.g. @@ -1,9 +1,10 @@
        labels.push(TabLabel {
            text: hunk.header.clone(),
            x: diff_x + 10.0,
            y: current_y + 3.0,
            color: GlyphonColor::rgb(120, 150, 200),
            bold: true,
            max_width: diff_width - 20.0,
        });
        current_y += DIFF_LINE_HEIGHT;

        let max_lines = hunk.left_lines.len().max(hunk.right_lines.len());
        for line_idx in 0..max_lines {
            if current_y + DIFF_LINE_HEIGHT > diff_y + diff_height {
                break;
            }

            // Left line (Original)
            if let Some(left) = hunk.left_lines.get(line_idx) {
                if let Some(num) = left.line_number {
                    labels.push(TabLabel {
                        text: format!("{num:>3}"),
                        x: diff_x + 8.0,
                        y: current_y + 3.0,
                        color: GlyphonColor::rgb(110, 115, 130),
                        bold: false,
                        max_width: DIFF_GUTTER_WIDTH - 10.0,
                    });
                }

                if !left.content.is_empty() {
                    let code_color = match left.line_type {
                        DiffLineType::Deletion => GlyphonColor::rgb(255, 154, 161), // red
                        _ => GlyphonColor::rgb(190, 195, 210),                      // neutral
                    };
                    labels.push(TabLabel {
                        text: left.content.clone(),
                        x: diff_x + DIFF_GUTTER_WIDTH + 8.0,
                        y: current_y + 3.0,
                        color: code_color,
                        bold: false,
                        max_width: half_w - DIFF_GUTTER_WIDTH - 16.0,
                    });
                }
            }

            // Right line (Modified)
            if let Some(right) = hunk.right_lines.get(line_idx) {
                if let Some(num) = right.line_number {
                    labels.push(TabLabel {
                        text: format!("{num:>3}"),
                        x: diff_x + half_w + 8.0,
                        y: current_y + 3.0,
                        color: GlyphonColor::rgb(110, 115, 130),
                        bold: false,
                        max_width: DIFF_GUTTER_WIDTH - 10.0,
                    });
                }

                if !right.content.is_empty() {
                    let code_color = match right.line_type {
                        DiffLineType::Addition => GlyphonColor::rgb(141, 229, 169), // green
                        _ => GlyphonColor::rgb(190, 195, 210),                      // neutral
                    };
                    labels.push(TabLabel {
                        text: right.content.clone(),
                        x: diff_x + half_w + DIFF_GUTTER_WIDTH + 8.0,
                        y: current_y + 3.0,
                        color: code_color,
                        bold: false,
                        max_width: half_w - DIFF_GUTTER_WIDTH - 16.0,
                    });
                }
            }

            current_y += DIFF_LINE_HEIGHT;
        }
    }

    labels
}

/// Hit tests mouse clicks within the diff view pane.
#[must_use]
pub fn hit_test_diff_view(
    x: f32,
    y: f32,
    diff_x: f32,
    diff_y: f32,
    diff_width: f32,
    diff_height: f32,
) -> DiffAction {
    if x < diff_x || x > diff_x + diff_width || y < diff_y || y > diff_y + diff_height {
        return DiffAction::None;
    }

    // Check header buttons
    if y <= diff_y + DIFF_HEADER_HEIGHT {
        let btn_y = diff_y + 5.0;
        if y >= btn_y && y <= btn_y + 24.0 {
            if x >= diff_x + diff_width - 170.0 && x <= diff_x + diff_width - 120.0 {
                return DiffAction::ToggleStage;
            }
            if x >= diff_x + diff_width - 115.0 && x <= diff_x + diff_width - 50.0 {
                return DiffAction::OpenFile;
            }
            if x >= diff_x + diff_width - 45.0 && x <= diff_x + diff_width - 20.0 {
                return DiffAction::Close;
            }
        }
    }

    DiffAction::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_diff_view_state_has_hunk_and_lines() {
        let state = DiffViewState::default();
        assert_eq!(state.file_name, "sw.js");
        assert_eq!(state.hunks.len(), 1);
        let hunk = &state.hunks[0];
        assert_eq!(hunk.left_lines.len(), 10);
        assert_eq!(hunk.right_lines.len(), 10);
    }

    #[test]
    fn assemble_diff_view_quads_generates_quads() {
        let state = DiffViewState::default();
        let quads =
            assemble_diff_view_quads(200.0, 36.0, 600.0, 500.0, [0.0, 1.0, 0.5, 1.0], &state);
        assert_ne!(quads.len(), 0);
    }

    #[test]
    fn build_diff_view_labels_contains_code_and_buttons() {
        let state = DiffViewState::default();
        let labels = build_diff_view_labels(200.0, 36.0, 600.0, 500.0, &state);
        assert_ne!(labels.len(), 0);
        assert!(labels.iter().any(|l| l.text.contains("sw.js")));
        assert!(labels.iter().any(|l| l.text.contains("CACHE_NAME")));
        assert!(labels.iter().any(|l| l.text.contains("Open File")));
    }

    #[test]
    fn hit_test_diff_view_detects_buttons() {
        let x = 200.0;
        let y = 36.0;
        let w = 600.0;
        let h = 500.0;

        assert_eq!(
            hit_test_diff_view(x + w - 140.0, y + 10.0, x, y, w, h),
            DiffAction::ToggleStage
        );
        assert_eq!(
            hit_test_diff_view(x + w - 80.0, y + 10.0, x, y, w, h),
            DiffAction::OpenFile
        );
        assert_eq!(
            hit_test_diff_view(x + w - 30.0, y + 10.0, x, y, w, h),
            DiffAction::Close
        );
    }
}
