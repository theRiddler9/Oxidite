//! Side-by-side code diff viewer and in-built code review workspace (visual center pane).

use crate::renderer::draw::{QuadVertex, TabLabel, build_quad_vertices};
use glyphon::Color as GlyphonColor;

/// Height of the top diff header in logical pixels.
pub const DIFF_HEADER_HEIGHT: f32 = 34.0;
/// Line height in logical pixels for each diff code line.
pub const DIFF_LINE_HEIGHT: f32 = 20.0;
/// Height in logical pixels for an inline code review comment card.
pub const DIFF_COMMENT_HEIGHT: f32 = 22.0;
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

/// Column side in a side-by-side diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffSide {
    /// Left base / original column.
    Original,
    /// Right modified / new column.
    Modified,
}

/// Overall review verdict for the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewVerdict {
    /// Actively under review.
    InReview,
    /// All changes approved.
    Approved,
    /// Changes requested by reviewer.
    ChangesRequested,
}

impl ReviewVerdict {
    /// Badge label for the status ribbon.
    #[must_use]
    pub fn badge_text(self) -> &'static str {
        match self {
            ReviewVerdict::InReview => "● In Review",
            ReviewVerdict::Approved => "✓ Approved",
            ReviewVerdict::ChangesRequested => "⚠ Changes",
        }
    }

    /// Color representing this verdict in [r, g, b, a].
    #[must_use]
    pub fn badge_color(self) -> [f32; 4] {
        match self {
            ReviewVerdict::InReview => [0.91, 0.72, 0.36, 0.95], // Amber
            ReviewVerdict::Approved => [0.21, 0.82, 0.50, 0.95], // Emerald green
            ReviewVerdict::ChangesRequested => [1.0, 0.38, 0.42, 0.95], // Danger red
        }
    }

    /// Cycles to the next verdict state.
    #[must_use]
    pub fn cycle(self) -> Self {
        match self {
            ReviewVerdict::InReview => ReviewVerdict::Approved,
            ReviewVerdict::Approved => ReviewVerdict::ChangesRequested,
            ReviewVerdict::ChangesRequested => ReviewVerdict::InReview,
        }
    }
}

/// Tag categorization for inline review comments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewTag {
    /// Small cosmetic or style note.
    Nit,
    /// Improvement suggestion.
    Suggestion,
    /// Critical defect or bug.
    Issue,
    /// Commendation or approval.
    Approval,
    /// General observation.
    Note,
}

impl ReviewTag {
    /// Short display tag label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ReviewTag::Nit => "NIT",
            ReviewTag::Suggestion => "SUGGEST",
            ReviewTag::Issue => "ISSUE",
            ReviewTag::Approval => "LGTM",
            ReviewTag::Note => "NOTE",
        }
    }

    /// Primary accent color for the tag.
    #[must_use]
    pub fn color(self) -> [f32; 4] {
        match self {
            ReviewTag::Nit => [0.65, 0.68, 0.75, 1.0],
            ReviewTag::Suggestion => [0.47, 0.67, 1.0, 1.0],
            ReviewTag::Issue => [1.0, 0.45, 0.45, 1.0],
            ReviewTag::Approval => [0.35, 0.85, 0.55, 1.0],
            ReviewTag::Note => [0.95, 0.75, 0.35, 1.0],
        }
    }
}

/// An inline code review comment attached to a specific line in the diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewComment {
    /// Unique identifier for this comment.
    pub id: usize,
    /// File line number being reviewed.
    pub line_number: usize,
    /// Column side (original or modified).
    pub side: DiffSide,
    /// Author / reviewer name.
    pub author: String,
    /// Tag categorization.
    pub tag: ReviewTag,
    /// Comment body text.
    pub content: String,
    /// Whether this comment discussion has been resolved.
    pub resolved: bool,
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
    /// Whether review mode toolbar and annotations are active.
    pub review_mode: bool,
    /// Current review verdict (`InReview`, `Approved`, `ChangesRequested`).
    pub review_verdict: ReviewVerdict,
    /// Inline review comments attached to lines in this file.
    pub comments: Vec<ReviewComment>,
    /// Auto-incrementing identifier counter for new comments.
    pub next_comment_id: usize,
    /// Currently focused or selected line (`line_number`, side).
    pub selected_line: Option<(usize, DiffSide)>,
}

impl DiffViewState {
    /// Adds an inline code review comment and returns its unique ID.
    pub fn add_comment(
        &mut self,
        line_number: usize,
        side: DiffSide,
        tag: ReviewTag,
        content: String,
        author: String,
    ) -> usize {
        let id = self.next_comment_id;
        self.next_comment_id += 1;
        self.comments.push(ReviewComment {
            id,
            line_number,
            side,
            author,
            tag,
            content,
            resolved: false,
        });
        id
    }

    /// Toggles the resolved state of a comment by ID.
    pub fn toggle_resolve(&mut self, comment_id: usize) {
        if let Some(comment) = self.comments.iter_mut().find(|c| c.id == comment_id) {
            comment.resolved = !comment.resolved;
        }
    }

    /// Deletes a comment by ID.
    pub fn delete_comment(&mut self, comment_id: usize) {
        self.comments.retain(|c| c.id != comment_id);
    }

    /// Sets the overall review verdict.
    pub fn set_verdict(&mut self, verdict: ReviewVerdict) {
        self.review_verdict = verdict;
    }

    /// Number of total comments.
    #[must_use]
    pub fn comment_count(&self) -> usize {
        self.comments.len()
    }

    /// Number of resolved comments.
    #[must_use]
    pub fn resolved_comment_count(&self) -> usize {
        self.comments.iter().filter(|c| c.resolved).count()
    }

    /// Generates markdown export for PR review or clipboard sharing.
    #[must_use]
    pub fn generate_review_markdown(&self) -> String {
        use std::fmt::Write;
        let mut md = format!("# Code Review: {}\n\n", self.file_name);
        let _ = writeln!(md, "**Folder:** `{}`  ", self.path_info);
        let _ = writeln!(md, "**Status:** {}  ", self.review_verdict.badge_text());
        let _ = writeln!(
            md,
            "**Progress:** {} / {} comments resolved\n",
            self.resolved_comment_count(),
            self.comment_count()
        );
        md.push_str("### Inline Review Notes\n\n");
        if self.comments.is_empty() {
            md.push_str("_No review comments. Changes look ready to merge!_\n");
        } else {
            for c in &self.comments {
                let side_label = match c.side {
                    DiffSide::Original => "Original",
                    DiffSide::Modified => "Modified",
                };
                let status_badge = if c.resolved { "[RESOLVED]" } else { "[OPEN]" };
                let _ = writeln!(
                    md,
                    "- **Line {} ({})** `{}` {} {}\n  _Reviewer: {}_\n",
                    c.line_number,
                    side_label,
                    c.tag.label(),
                    status_badge,
                    c.content,
                    c.author
                );
            }
        }
        md
    }
}

impl Default for DiffViewState {
    #[allow(clippy::too_many_lines)]
    fn default() -> Self {
        Self {
            file_name: "sw.js".to_string(),
            path_info: "public/".to_string(),
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
            review_mode: true,
            review_verdict: ReviewVerdict::InReview,
            comments: vec![
                ReviewComment {
                    id: 1,
                    line_number: 1,
                    side: DiffSide::Modified,
                    author: "Reviewer".to_string(),
                    tag: ReviewTag::Suggestion,
                    content: "Consider pinning release version instead of -beta".to_string(),
                    resolved: false,
                },
                ReviewComment {
                    id: 2,
                    line_number: 6,
                    side: DiffSide::Modified,
                    author: "Reviewer".to_string(),
                    tag: ReviewTag::Approval,
                    content: "Good addition: vendor.wasm properly bundled".to_string(),
                    resolved: true,
                },
            ],
            next_comment_id: 3,
            selected_line: None,
        }
    }
}

/// Interactive actions dispatched by clicking inside the diff view.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Toggle between standard Diff and Code Review toolbar.
    ToggleReviewMode,
    /// Update the review verdict (`InReview`, `Approved`, `ChangesRequested`).
    SetVerdict(ReviewVerdict),
    /// Cycle the review verdict.
    CycleVerdict,
    /// Toggle resolved state of a comment by ID.
    ToggleResolve(usize),
    /// Delete a comment by ID.
    DeleteComment(usize),
    /// Copy review markdown summary to clipboard.
    CopyReviewSummary,
    /// Click on a diff line to select or review it.
    SelectLine(usize, DiffSide),
}

/// Builds quads for the side-by-side diff: pitch black background, column gutters,
/// center divider, green/red line highlights, and inline code review comment cards.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::too_many_lines,
    clippy::if_not_else
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

    // Active mode tab underline indicator
    let underline_x = if state.review_mode {
        diff_x + 236.0
    } else {
        diff_x + 180.0
    };
    let underline_w = if state.review_mode { 74.0 } else { 50.0 };
    quads.extend_from_slice(&build_quad_vertices(
        underline_x,
        diff_y + DIFF_HEADER_HEIGHT - 2.0,
        underline_w,
        2.0,
        accent_color,
    ));

    // Header buttons
    let btn_y = diff_y + 5.0;

    // Diff / Review mode toggle buttons
    let diff_btn_bg = if !state.review_mode {
        [0.09, 0.11, 0.15, 0.9]
    } else {
        [0.04, 0.05, 0.07, 0.7]
    };
    quads.extend_from_slice(&build_quad_vertices(
        diff_x + 180.0,
        btn_y,
        50.0,
        24.0,
        diff_btn_bg,
    ));

    let review_btn_bg = if state.review_mode {
        [0.09, 0.11, 0.15, 0.9]
    } else {
        [0.04, 0.05, 0.07, 0.7]
    };
    quads.extend_from_slice(&build_quad_vertices(
        diff_x + 236.0,
        btn_y,
        74.0,
        24.0,
        review_btn_bg,
    ));

    // If review mode is active, render review verdict badge and review actions
    if state.review_mode {
        let v_color = state.review_verdict.badge_color();
        // Verdict pill badge
        quads.extend_from_slice(&build_quad_vertices(
            diff_x + 316.0,
            btn_y,
            88.0,
            24.0,
            [v_color[0] * 0.25, v_color[1] * 0.25, v_color[2] * 0.25, 0.9],
        ));
        quads.extend_from_slice(&build_quad_vertices(
            diff_x + 316.0,
            btn_y,
            3.0,
            24.0,
            v_color,
        ));

        // Review actions: Approve, Changes, Export
        // [✓ Approve] button
        quads.extend_from_slice(&build_quad_vertices(
            diff_x + diff_width - 335.0,
            btn_y,
            60.0,
            24.0,
            [0.08, 0.18, 0.12, 0.85],
        ));
        // [⚠ Changes] button
        quads.extend_from_slice(&build_quad_vertices(
            diff_x + diff_width - 270.0,
            btn_y,
            62.0,
            24.0,
            [0.22, 0.10, 0.11, 0.85],
        ));
        // [📋 Export] button
        quads.extend_from_slice(&build_quad_vertices(
            diff_x + diff_width - 203.0,
            btn_y,
            55.0,
            24.0,
            [0.07, 0.08, 0.11, 0.85],
        ));
    }

    // Stage button
    quads.extend_from_slice(&build_quad_vertices(
        diff_x + diff_width - 143.0,
        btn_y,
        48.0,
        24.0,
        [0.06, 0.07, 0.09, 0.9],
    ));
    // Open File button
    quads.extend_from_slice(&build_quad_vertices(
        diff_x + diff_width - 90.0,
        btn_y,
        55.0,
        24.0,
        [0.06, 0.07, 0.09, 0.9],
    ));
    // Close button
    quads.extend_from_slice(&build_quad_vertices(
        diff_x + diff_width - 30.0,
        btn_y,
        22.0,
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

    // 5. Line highlights and inline code review cards
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

            let left_line = hunk.left_lines.get(line_idx);
            let right_line = hunk.right_lines.get(line_idx);

            // Left line highlight
            if let Some(left) = left_line
                && left.line_type == DiffLineType::Deletion
            {
                quads.extend_from_slice(&build_quad_vertices(
                    diff_x + DIFF_GUTTER_WIDTH + 1.0,
                    current_y,
                    half_w - DIFF_GUTTER_WIDTH - 1.0,
                    DIFF_LINE_HEIGHT,
                    [0.169, 0.067, 0.078, 0.85],
                ));
                quads.extend_from_slice(&build_quad_vertices(
                    diff_x,
                    current_y,
                    DIFF_GUTTER_WIDTH,
                    DIFF_LINE_HEIGHT,
                    [0.294, 0.09, 0.11, 0.8],
                ));
            }

            // Right line highlight
            if let Some(right) = right_line
                && right.line_type == DiffLineType::Addition
            {
                quads.extend_from_slice(&build_quad_vertices(
                    diff_x + half_w + DIFF_GUTTER_WIDTH + 1.0,
                    current_y,
                    half_w - DIFF_GUTTER_WIDTH - 1.0,
                    DIFF_LINE_HEIGHT,
                    [0.063, 0.153, 0.098, 0.85],
                ));
                quads.extend_from_slice(&build_quad_vertices(
                    diff_x + half_w,
                    current_y,
                    DIFF_GUTTER_WIDTH,
                    DIFF_LINE_HEIGHT,
                    [0.09, 0.27, 0.16, 0.8],
                ));
            }

            current_y += DIFF_LINE_HEIGHT;

            // 5.5 Inline Review Comment Cards (if review mode is active)
            if state.review_mode {
                let left_comments: Vec<&ReviewComment> = left_line
                    .and_then(|l| l.line_number)
                    .map(|ln| {
                        state
                            .comments
                            .iter()
                            .filter(|c| c.side == DiffSide::Original && c.line_number == ln)
                            .collect()
                    })
                    .unwrap_or_default();

                let right_comments: Vec<&ReviewComment> = right_line
                    .and_then(|r| r.line_number)
                    .map(|ln| {
                        state
                            .comments
                            .iter()
                            .filter(|c| c.side == DiffSide::Modified && c.line_number == ln)
                            .collect()
                    })
                    .unwrap_or_default();

                let max_comments = left_comments.len().max(right_comments.len());
                for c_idx in 0..max_comments {
                    if current_y + DIFF_COMMENT_HEIGHT > diff_y + diff_height {
                        break;
                    }

                    if let Some(c) = left_comments.get(c_idx) {
                        let card_w = half_w - DIFF_GUTTER_WIDTH - 6.0;
                        let card_x = diff_x + DIFF_GUTTER_WIDTH + 3.0;
                        let bg_color = if c.resolved {
                            [0.04, 0.05, 0.06, 0.85]
                        } else {
                            [0.08, 0.09, 0.13, 0.95]
                        };
                        quads.extend_from_slice(&build_quad_vertices(
                            card_x,
                            current_y + 1.0,
                            card_w,
                            DIFF_COMMENT_HEIGHT - 2.0,
                            bg_color,
                        ));
                        // Left accent bar
                        quads.extend_from_slice(&build_quad_vertices(
                            card_x,
                            current_y + 1.0,
                            3.0,
                            DIFF_COMMENT_HEIGHT - 2.0,
                            c.tag.color(),
                        ));
                        // Resolve button
                        quads.extend_from_slice(&build_quad_vertices(
                            card_x + card_w - 42.0,
                            current_y + 3.0,
                            18.0,
                            16.0,
                            [0.12, 0.14, 0.18, 0.9],
                        ));
                        // Delete button
                        quads.extend_from_slice(&build_quad_vertices(
                            card_x + card_w - 20.0,
                            current_y + 3.0,
                            16.0,
                            16.0,
                            [0.12, 0.14, 0.18, 0.9],
                        ));
                    }

                    if let Some(c) = right_comments.get(c_idx) {
                        let card_w = half_w - DIFF_GUTTER_WIDTH - 6.0;
                        let card_x = diff_x + half_w + DIFF_GUTTER_WIDTH + 3.0;
                        let bg_color = if c.resolved {
                            [0.04, 0.05, 0.06, 0.85]
                        } else {
                            [0.08, 0.09, 0.13, 0.95]
                        };
                        quads.extend_from_slice(&build_quad_vertices(
                            card_x,
                            current_y + 1.0,
                            card_w,
                            DIFF_COMMENT_HEIGHT - 2.0,
                            bg_color,
                        ));
                        // Left accent bar
                        quads.extend_from_slice(&build_quad_vertices(
                            card_x,
                            current_y + 1.0,
                            3.0,
                            DIFF_COMMENT_HEIGHT - 2.0,
                            c.tag.color(),
                        ));
                        // Resolve button
                        quads.extend_from_slice(&build_quad_vertices(
                            card_x + card_w - 42.0,
                            current_y + 3.0,
                            18.0,
                            16.0,
                            [0.12, 0.14, 0.18, 0.9],
                        ));
                        // Delete button
                        quads.extend_from_slice(&build_quad_vertices(
                            card_x + card_w - 20.0,
                            current_y + 3.0,
                            16.0,
                            16.0,
                            [0.12, 0.14, 0.18, 0.9],
                        ));
                    }

                    current_y += DIFF_COMMENT_HEIGHT;
                }
            }
        }
    }

    quads
}

/// Builds text labels for the diff header, review toolbar, line numbers, code, and comments.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::too_many_lines,
    clippy::cast_sign_loss,
    clippy::if_not_else
)]
pub fn build_diff_view_labels(
    diff_x: f32,
    diff_y: f32,
    diff_width: f32,
    diff_height: f32,
    state: &DiffViewState,
) -> Vec<TabLabel> {
    let mut labels = Vec::new();

    // 1. File Title
    let tab_text = format!("{} {}", state.file_name, state.path_info);
    labels.push(TabLabel {
        text: tab_text,
        x: diff_x + 10.0,
        y: diff_y + 8.0,
        color: GlyphonColor::rgb(240, 245, 255),
        bold: true,
        max_width: 160.0,
    });

    let btn_y = diff_y + 9.0;

    // Diff / Review mode toggle labels
    labels.push(TabLabel {
        text: "⇄ Diff".to_string(),
        x: diff_x + 185.0,
        y: btn_y,
        color: if !state.review_mode {
            GlyphonColor::rgb(240, 245, 255)
        } else {
            GlyphonColor::rgb(140, 145, 160)
        },
        bold: !state.review_mode,
        max_width: 45.0,
    });

    let review_text = format!("💬 Review ({})", state.comment_count());
    labels.push(TabLabel {
        text: review_text,
        x: diff_x + 240.0,
        y: btn_y,
        color: if state.review_mode {
            GlyphonColor::rgb(240, 245, 255)
        } else {
            GlyphonColor::rgb(140, 145, 160)
        },
        bold: state.review_mode,
        max_width: 70.0,
    });

    // Review status and actions if review mode active
    if state.review_mode {
        let v_color = state.review_verdict.badge_color();
        let vr = (v_color[0] * 255.0).round() as u8;
        let vg = (v_color[1] * 255.0).round() as u8;
        let vb = (v_color[2] * 255.0).round() as u8;

        labels.push(TabLabel {
            text: state.review_verdict.badge_text().to_string(),
            x: diff_x + 322.0,
            y: btn_y,
            color: GlyphonColor::rgb(vr, vg, vb),
            bold: true,
            max_width: 82.0,
        });

        // [✓ Approve]
        labels.push(TabLabel {
            text: "✓ Approve".to_string(),
            x: diff_x + diff_width - 332.0,
            y: btn_y,
            color: GlyphonColor::rgb(130, 230, 160),
            bold: true,
            max_width: 55.0,
        });

        // [⚠ Changes]
        labels.push(TabLabel {
            text: "⚠ Changes".to_string(),
            x: diff_x + diff_width - 267.0,
            y: btn_y,
            color: GlyphonColor::rgb(255, 140, 150),
            bold: true,
            max_width: 58.0,
        });

        // [📋 Export]
        labels.push(TabLabel {
            text: "📋 Export".to_string(),
            x: diff_x + diff_width - 200.0,
            y: btn_y,
            color: GlyphonColor::rgb(210, 215, 230),
            bold: false,
            max_width: 50.0,
        });
    }

    // Stage button
    let stage_text = if state.staged { "Unstage" } else { "Stage" };
    labels.push(TabLabel {
        text: stage_text.to_string(),
        x: diff_x + diff_width - 140.0,
        y: btn_y,
        color: GlyphonColor::rgb(220, 225, 240),
        bold: false,
        max_width: 42.0,
    });

    // Open File button
    labels.push(TabLabel {
        text: "Open File".to_string(),
        x: diff_x + diff_width - 87.0,
        y: btn_y,
        color: GlyphonColor::rgb(220, 225, 240),
        bold: false,
        max_width: 50.0,
    });

    // Close '×' button
    labels.push(TabLabel {
        text: "×".to_string(),
        x: diff_x + diff_width - 25.0,
        y: btn_y,
        color: GlyphonColor::rgb(160, 165, 180),
        bold: true,
        max_width: 18.0,
    });

    // 2. Diff Lines and Review Comments Rendering
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

            let left_line = hunk.left_lines.get(line_idx);
            let right_line = hunk.right_lines.get(line_idx);

            // Left line (Original)
            if let Some(left) = left_line {
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
                        DiffLineType::Deletion => GlyphonColor::rgb(255, 154, 161),
                        _ => GlyphonColor::rgb(190, 195, 210),
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
            if let Some(right) = right_line {
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
                        DiffLineType::Addition => GlyphonColor::rgb(141, 229, 169),
                        _ => GlyphonColor::rgb(190, 195, 210),
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

            // Render inline review comment labels
            if state.review_mode {
                let left_comments: Vec<&ReviewComment> = left_line
                    .and_then(|l| l.line_number)
                    .map(|ln| {
                        state
                            .comments
                            .iter()
                            .filter(|c| c.side == DiffSide::Original && c.line_number == ln)
                            .collect()
                    })
                    .unwrap_or_default();

                let right_comments: Vec<&ReviewComment> = right_line
                    .and_then(|r| r.line_number)
                    .map(|ln| {
                        state
                            .comments
                            .iter()
                            .filter(|c| c.side == DiffSide::Modified && c.line_number == ln)
                            .collect()
                    })
                    .unwrap_or_default();

                let max_comments = left_comments.len().max(right_comments.len());
                for c_idx in 0..max_comments {
                    if current_y + DIFF_COMMENT_HEIGHT > diff_y + diff_height {
                        break;
                    }

                    if let Some(c) = left_comments.get(c_idx) {
                        let card_x = diff_x + DIFF_GUTTER_WIDTH + 3.0;
                        let card_w = half_w - DIFF_GUTTER_WIDTH - 6.0;
                        let tc = c.tag.color();
                        let tag_color = GlyphonColor::rgb(
                            (tc[0] * 255.0).round() as u8,
                            (tc[1] * 255.0).round() as u8,
                            (tc[2] * 255.0).round() as u8,
                        );

                        // Tag label
                        labels.push(TabLabel {
                            text: format!("[{}]", c.tag.label()),
                            x: card_x + 6.0,
                            y: current_y + 3.0,
                            color: tag_color,
                            bold: true,
                            max_width: 55.0,
                        });

                        // Comment content
                        let content_text = format!("{}: {}", c.author, c.content);
                        labels.push(TabLabel {
                            text: content_text,
                            x: card_x + 64.0,
                            y: current_y + 3.0,
                            color: if c.resolved {
                                GlyphonColor::rgb(140, 145, 155)
                            } else {
                                GlyphonColor::rgb(220, 225, 240)
                            },
                            bold: false,
                            max_width: card_w - 110.0,
                        });

                        // Resolve button
                        let resolve_char = if c.resolved { "✓" } else { "○" };
                        labels.push(TabLabel {
                            text: resolve_char.to_string(),
                            x: card_x + card_w - 38.0,
                            y: current_y + 3.0,
                            color: if c.resolved {
                                GlyphonColor::rgb(50, 210, 120)
                            } else {
                                GlyphonColor::rgb(150, 155, 170)
                            },
                            bold: true,
                            max_width: 14.0,
                        });

                        // Delete button
                        labels.push(TabLabel {
                            text: "×".to_string(),
                            x: card_x + card_w - 16.0,
                            y: current_y + 3.0,
                            color: GlyphonColor::rgb(160, 165, 180),
                            bold: true,
                            max_width: 14.0,
                        });
                    }

                    if let Some(c) = right_comments.get(c_idx) {
                        let card_x = diff_x + half_w + DIFF_GUTTER_WIDTH + 3.0;
                        let card_w = half_w - DIFF_GUTTER_WIDTH - 6.0;
                        let tc = c.tag.color();
                        let tag_color = GlyphonColor::rgb(
                            (tc[0] * 255.0).round() as u8,
                            (tc[1] * 255.0).round() as u8,
                            (tc[2] * 255.0).round() as u8,
                        );

                        // Tag label
                        labels.push(TabLabel {
                            text: format!("[{}]", c.tag.label()),
                            x: card_x + 6.0,
                            y: current_y + 3.0,
                            color: tag_color,
                            bold: true,
                            max_width: 55.0,
                        });

                        // Comment content
                        let content_text = format!("{}: {}", c.author, c.content);
                        labels.push(TabLabel {
                            text: content_text,
                            x: card_x + 64.0,
                            y: current_y + 3.0,
                            color: if c.resolved {
                                GlyphonColor::rgb(140, 145, 155)
                            } else {
                                GlyphonColor::rgb(220, 225, 240)
                            },
                            bold: false,
                            max_width: card_w - 110.0,
                        });

                        // Resolve button
                        let resolve_char = if c.resolved { "✓" } else { "○" };
                        labels.push(TabLabel {
                            text: resolve_char.to_string(),
                            x: card_x + card_w - 38.0,
                            y: current_y + 3.0,
                            color: if c.resolved {
                                GlyphonColor::rgb(50, 210, 120)
                            } else {
                                GlyphonColor::rgb(150, 155, 170)
                            },
                            bold: true,
                            max_width: 14.0,
                        });

                        // Delete button
                        labels.push(TabLabel {
                            text: "×".to_string(),
                            x: card_x + card_w - 16.0,
                            y: current_y + 3.0,
                            color: GlyphonColor::rgb(160, 165, 180),
                            bold: true,
                            max_width: 14.0,
                        });
                    }

                    current_y += DIFF_COMMENT_HEIGHT;
                }
            }
        }
    }

    labels
}

/// Hit tests mouse clicks within the diff view and review toolbar.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn hit_test_diff_view(
    x: f32,
    y: f32,
    diff_x: f32,
    diff_y: f32,
    diff_width: f32,
    diff_height: f32,
    state: &DiffViewState,
) -> DiffAction {
    if x < diff_x || x > diff_x + diff_width || y < diff_y || y > diff_y + diff_height {
        return DiffAction::None;
    }

    // 1. Check header bar buttons
    if y <= diff_y + DIFF_HEADER_HEIGHT {
        let btn_y = diff_y + 5.0;
        if y >= btn_y && y <= btn_y + 24.0 {
            // Mode toggle buttons: Diff vs Review
            if x >= diff_x + 180.0 && x <= diff_x + 230.0 && state.review_mode {
                return DiffAction::ToggleReviewMode;
            }
            if x >= diff_x + 236.0 && x <= diff_x + 310.0 && !state.review_mode {
                return DiffAction::ToggleReviewMode;
            }

            // Review mode actions
            if state.review_mode {
                // Verdict badge: cycle verdict on click
                if x >= diff_x + 316.0 && x <= diff_x + 404.0 {
                    return DiffAction::CycleVerdict;
                }
                // Approve button
                if x >= diff_x + diff_width - 335.0 && x <= diff_x + diff_width - 275.0 {
                    return DiffAction::SetVerdict(ReviewVerdict::Approved);
                }
                // Request Changes button
                if x >= diff_x + diff_width - 270.0 && x <= diff_x + diff_width - 208.0 {
                    return DiffAction::SetVerdict(ReviewVerdict::ChangesRequested);
                }
                // Export summary button
                if x >= diff_x + diff_width - 203.0 && x <= diff_x + diff_width - 148.0 {
                    return DiffAction::CopyReviewSummary;
                }
            }

            // Stage button
            if x >= diff_x + diff_width - 143.0 && x <= diff_x + diff_width - 95.0 {
                return DiffAction::ToggleStage;
            }
            // Open File button
            if x >= diff_x + diff_width - 90.0 && x <= diff_x + diff_width - 35.0 {
                return DiffAction::OpenFile;
            }
            // Close button
            if x >= diff_x + diff_width - 30.0 && x <= diff_x + diff_width - 8.0 {
                return DiffAction::Close;
            }
        }
        return DiffAction::None;
    }

    // 2. Check diff lines and inline comment card clicks
    let content_y = diff_y + DIFF_HEADER_HEIGHT;
    let half_w = (diff_width / 2.0).floor();
    let mut current_y = content_y;

    for hunk in &state.hunks {
        current_y += DIFF_LINE_HEIGHT; // hunk header

        let max_lines = hunk.left_lines.len().max(hunk.right_lines.len());
        for line_idx in 0..max_lines {
            if current_y + DIFF_LINE_HEIGHT > diff_y + diff_height {
                break;
            }

            let left_line = hunk.left_lines.get(line_idx);
            let right_line = hunk.right_lines.get(line_idx);

            // Check if click was on the line row itself
            if y >= current_y && y < current_y + DIFF_LINE_HEIGHT {
                if x < diff_x + half_w {
                    if let Some(left) = left_line
                        && let Some(num) = left.line_number
                    {
                        return DiffAction::SelectLine(num, DiffSide::Original);
                    }
                } else if let Some(right) = right_line
                    && let Some(num) = right.line_number
                {
                    return DiffAction::SelectLine(num, DiffSide::Modified);
                }
            }

            current_y += DIFF_LINE_HEIGHT;

            // Check comment clicks if in review mode
            if state.review_mode {
                let left_comments: Vec<&ReviewComment> = left_line
                    .and_then(|l| l.line_number)
                    .map(|ln| {
                        state
                            .comments
                            .iter()
                            .filter(|c| c.side == DiffSide::Original && c.line_number == ln)
                            .collect()
                    })
                    .unwrap_or_default();

                let right_comments: Vec<&ReviewComment> = right_line
                    .and_then(|r| r.line_number)
                    .map(|ln| {
                        state
                            .comments
                            .iter()
                            .filter(|c| c.side == DiffSide::Modified && c.line_number == ln)
                            .collect()
                    })
                    .unwrap_or_default();

                let max_comments = left_comments.len().max(right_comments.len());
                for c_idx in 0..max_comments {
                    if current_y + DIFF_COMMENT_HEIGHT > diff_y + diff_height {
                        break;
                    }

                    if y >= current_y && y < current_y + DIFF_COMMENT_HEIGHT {
                        // Left column comments
                        if x < diff_x + half_w
                            && let Some(c) = left_comments.get(c_idx)
                        {
                            let card_x = diff_x + DIFF_GUTTER_WIDTH + 3.0;
                            let card_w = half_w - DIFF_GUTTER_WIDTH - 6.0;
                            if x >= card_x + card_w - 42.0 && x <= card_x + card_w - 24.0 {
                                return DiffAction::ToggleResolve(c.id);
                            }
                            if x >= card_x + card_w - 20.0 && x <= card_x + card_w - 4.0 {
                                return DiffAction::DeleteComment(c.id);
                            }
                        }
                        // Right column comments
                        if x >= diff_x + half_w
                            && let Some(c) = right_comments.get(c_idx)
                        {
                            let card_x = diff_x + half_w + DIFF_GUTTER_WIDTH + 3.0;
                            let card_w = half_w - DIFF_GUTTER_WIDTH - 6.0;
                            if x >= card_x + card_w - 42.0 && x <= card_x + card_w - 24.0 {
                                return DiffAction::ToggleResolve(c.id);
                            }
                            if x >= card_x + card_w - 20.0 && x <= card_x + card_w - 4.0 {
                                return DiffAction::DeleteComment(c.id);
                            }
                        }
                    }

                    current_y += DIFF_COMMENT_HEIGHT;
                }
            }
        }
    }

    DiffAction::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_diff_view_state_has_hunk_and_comments() {
        let state = DiffViewState::default();
        assert_eq!(state.file_name, "sw.js");
        assert_eq!(state.hunks.len(), 1);
        assert!(state.review_mode);
        assert_eq!(state.review_verdict, ReviewVerdict::InReview);
        assert_eq!(state.comments.len(), 2);
    }

    #[test]
    fn comment_management_and_resolution() {
        let mut state = DiffViewState::default();
        let initial_count = state.comment_count();
        let new_id = state.add_comment(
            3,
            DiffSide::Modified,
            ReviewTag::Nit,
            "Check trailing comma".to_string(),
            "Akash".to_string(),
        );
        assert_eq!(state.comment_count(), initial_count + 1);

        let initial_resolved = state.resolved_comment_count();
        state.toggle_resolve(new_id);
        assert_eq!(state.resolved_comment_count(), initial_resolved + 1);

        state.delete_comment(new_id);
        assert_eq!(state.comment_count(), initial_count);
    }

    #[test]
    fn generate_review_markdown_includes_file_and_comments() {
        let state = DiffViewState::default();
        let md = state.generate_review_markdown();
        assert!(md.contains("# Code Review: sw.js"));
        assert!(md.contains("Consider pinning release"));
        assert!(md.contains("`SUGGEST`"));
        assert!(md.contains("[OPEN]"));
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
        assert!(labels.iter().any(|l| l.text.contains("Approve")));
        assert!(labels.iter().any(|l| l.text.contains("Review")));
    }

    #[test]
    fn hit_test_diff_view_detects_buttons() {
        let state = DiffViewState::default();
        let x = 200.0;
        let y = 36.0;
        let w = 800.0;
        let h = 500.0;

        assert_eq!(
            hit_test_diff_view(x + 200.0, y + 10.0, x, y, w, h, &state),
            DiffAction::ToggleReviewMode
        );
        assert_eq!(
            hit_test_diff_view(x + w - 300.0, y + 10.0, x, y, w, h, &state),
            DiffAction::SetVerdict(ReviewVerdict::Approved)
        );
        assert_eq!(
            hit_test_diff_view(x + w - 120.0, y + 10.0, x, y, w, h, &state),
            DiffAction::ToggleStage
        );
        assert_eq!(
            hit_test_diff_view(x + w - 70.0, y + 10.0, x, y, w, h, &state),
            DiffAction::OpenFile
        );
        assert_eq!(
            hit_test_diff_view(x + w - 20.0, y + 10.0, x, y, w, h, &state),
            DiffAction::Close
        );
    }
}
