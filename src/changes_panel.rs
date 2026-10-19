//! Git and changes review panel (right rail of workspace).

use crate::renderer::draw::{QuadVertex, TabLabel, build_quad_vertices};
use glyphon::Color as GlyphonColor;

/// Default width of the right changes panel in logical pixels.
pub const CHANGES_PANEL_WIDTH: f32 = 280.0;
/// Height of the top tab header in the changes panel.
pub const CHANGES_HEADER_HEIGHT: f32 = 36.0;
/// Height of each changed file row.
pub const FILE_ROW_HEIGHT: f32 = 26.0;
/// Height of each commit history row.
pub const COMMIT_ROW_HEIGHT: f32 = 46.0;
/// Height of the bottom commit controls area.
pub const COMMIT_AREA_HEIGHT: f32 = 96.0;

/// Active tab within the right git panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangesTab {
    /// List of working-tree changes and staging controls.
    Changes,
    /// Commit log history.
    History,
}

/// Represents a modified, added, or deleted file in the working tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    /// Filename without directory prefix (e.g. "sw.js").
    pub filename: String,
    /// Parent folder / path info (e.g. "public").
    pub path_info: String,
    /// Number of lines added.
    pub additions: usize,
    /// Number of lines removed.
    pub deletions: usize,
    /// Whether this file is staged for commit.
    pub staged: bool,
}

/// State representation required to layout and render the changes panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangesPanelState {
    /// Active tab in the changes panel.
    pub active_tab: ChangesTab,
    /// List of tracked changed files.
    pub changed_files: Vec<ChangedFile>,
    /// Commit history log.
    pub history: Vec<crate::git::GitCommit>,
    /// Selected file index for diff preview.
    pub selected_file_index: Option<usize>,
    /// Selected commit index for history preview.
    pub selected_commit_index: Option<usize>,
    /// Pending commit message.
    pub commit_message: String,
    /// Current git branch name.
    pub git_branch: String,
}

impl Default for ChangesPanelState {
    fn default() -> Self {
        Self {
            active_tab: ChangesTab::Changes,
            changed_files: vec![
                ChangedFile {
                    filename: "sw.js".to_string(),
                    path_info: "public".to_string(),
                    additions: 1,
                    deletions: 1,
                    staged: true,
                },
                ChangedFile {
                    filename: "cargo.toml".to_string(),
                    path_info: ".".to_string(),
                    additions: 12,
                    deletions: 2,
                    staged: false,
                },
            ],
            history: vec![
                crate::git::GitCommit {
                    hash: "1b60625".to_string(),
                    author: "Antigravity".to_string(),
                    relative_time: "10m ago".to_string(),
                    message: "feat(diff_view): add in-built code review workspace".to_string(),
                },
                crate::git::GitCommit {
                    hash: "53e1aac".to_string(),
                    author: "Antigravity".to_string(),
                    relative_time: "1h ago".to_string(),
                    message: "feat(git): add real workspace project inspection".to_string(),
                },
            ],
            selected_file_index: Some(0),
            selected_commit_index: Some(0),
            commit_message: "Commit Tracked".to_string(),
            git_branch: "main".to_string(),
        }
    }
}

/// Interactive actions dispatched by clicking inside the changes panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangesAction {
    /// No interactive control was clicked.
    None,
    /// Switch between Changes and History tabs.
    SwitchTab(ChangesTab),
    /// Select a changed file for diff review.
    SelectFile(usize),
    /// Select a commit from history.
    SelectCommit(usize),
    /// Toggle staging checkbox for a file.
    ToggleStage(usize),
    /// Stage all modified files.
    StageAll,
    /// Commit staged changes.
    Commit,
    /// Trigger git fetch.
    Fetch,
}

/// Builds quads for rendering the changes panel: pure pitch-black background, 1px left divider,
/// tabs, checkboxes, file highlights, and commit area.
#[must_use]
#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
pub fn assemble_changes_panel_quads(
    panel_x: f32,
    panel_y: f32,
    panel_width: f32,
    panel_height: f32,
    accent_color: [f32; 4],
    state: &ChangesPanelState,
) -> Vec<QuadVertex> {
    let mut quads = Vec::new();

    // 1. Pure pitch-black background quad (#000000)
    quads.extend_from_slice(&build_quad_vertices(
        panel_x,
        panel_y,
        panel_width,
        panel_height,
        [0.0, 0.0, 0.0, 1.0],
    ));

    // 2. Left vertical border line dividing changes panel from main workspace (#181a20)
    quads.extend_from_slice(&build_quad_vertices(
        panel_x,
        panel_y,
        1.0,
        panel_height,
        [0.095, 0.105, 0.13, 1.0],
    ));

    // 3. Top tab header bottom divider line (#181a20)
    quads.extend_from_slice(&build_quad_vertices(
        panel_x,
        panel_y + CHANGES_HEADER_HEIGHT - 1.0,
        panel_width,
        1.0,
        [0.095, 0.105, 0.13, 1.0],
    ));

    // 4. Tab active indicators
    let tab_w = panel_width / 2.0;
    let (active_tab_x, active_tab_w) = match state.active_tab {
        ChangesTab::Changes => (panel_x, tab_w),
        ChangesTab::History => (panel_x + tab_w, tab_w),
    };
    // Subtle bottom highlight line on active tab
    quads.extend_from_slice(&build_quad_vertices(
        active_tab_x + 8.0,
        panel_y + CHANGES_HEADER_HEIGHT - 2.0,
        active_tab_w - 16.0,
        2.0,
        accent_color,
    ));

    // 5. Render depending on active tab: Changes vs History
    match state.active_tab {
        ChangesTab::Changes => {
            // Stage All button background quad (#101217)
            let stage_all_btn_x = panel_x + panel_width - 80.0;
            let stage_all_btn_y = panel_y + CHANGES_HEADER_HEIGHT + 6.0;
            quads.extend_from_slice(&build_quad_vertices(
                stage_all_btn_x,
                stage_all_btn_y,
                70.0,
                20.0,
                [0.06, 0.07, 0.09, 0.9],
            ));

            // Changed files rows and checkboxes
            let list_start_y = panel_y + CHANGES_HEADER_HEIGHT + 56.0;
            for (idx, file) in state.changed_files.iter().enumerate() {
                let row_y = list_start_y + (idx as f32 * FILE_ROW_HEIGHT);
                if row_y + FILE_ROW_HEIGHT > panel_y + panel_height - COMMIT_AREA_HEIGHT {
                    break;
                }

                let is_selected = state.selected_file_index == Some(idx);
                if is_selected {
                    // Selected row card highlight (#0B0C10)
                    quads.extend_from_slice(&build_quad_vertices(
                        panel_x + 6.0,
                        row_y,
                        panel_width - 12.0,
                        FILE_ROW_HEIGHT - 2.0,
                        [0.05, 0.055, 0.075, 0.95],
                    ));
                    // 2px neon leading marker
                    quads.extend_from_slice(&build_quad_vertices(
                        panel_x + 6.0,
                        row_y,
                        2.0,
                        FILE_ROW_HEIGHT - 2.0,
                        accent_color,
                    ));
                }

                // Checkbox quad (outer outline)
                let cb_x = panel_x + panel_width - 24.0;
                let cb_y = row_y + 4.0;
                let cb_color = if file.staged {
                    [accent_color[0], accent_color[1], accent_color[2], 0.9]
                } else {
                    [0.18, 0.20, 0.26, 0.8]
                };
                quads.extend_from_slice(&build_quad_vertices(cb_x, cb_y, 14.0, 14.0, cb_color));
            }

            // Commit controls area background and border
            let commit_y = panel_y + panel_height - COMMIT_AREA_HEIGHT;
            quads.extend_from_slice(&build_quad_vertices(
                panel_x,
                commit_y,
                panel_width,
                1.0,
                [0.095, 0.105, 0.13, 1.0],
            ));

            // Commit message input field quad (#08090C)
            quads.extend_from_slice(&build_quad_vertices(
                panel_x + 10.0,
                commit_y + 32.0,
                panel_width - 20.0,
                28.0,
                [0.04, 0.045, 0.06, 0.95],
            ));

            // Commit button quad (#14161F)
            quads.extend_from_slice(&build_quad_vertices(
                panel_x + panel_width - 120.0,
                commit_y + 66.0,
                110.0,
                24.0,
                [0.07, 0.08, 0.11, 0.95],
            ));
        }
        ChangesTab::History => {
            // Commit history rows
            let list_start_y = panel_y + CHANGES_HEADER_HEIGHT + 36.0;
            for (idx, _commit) in state.history.iter().enumerate() {
                let row_y = list_start_y + (idx as f32 * COMMIT_ROW_HEIGHT);
                if row_y + COMMIT_ROW_HEIGHT > panel_y + panel_height {
                    break;
                }

                let is_selected = state.selected_commit_index == Some(idx);
                if is_selected {
                    // Highlight selected commit row
                    quads.extend_from_slice(&build_quad_vertices(
                        panel_x + 4.0,
                        row_y,
                        panel_width - 8.0,
                        COMMIT_ROW_HEIGHT - 2.0,
                        [0.07, 0.09, 0.13, 0.95],
                    ));
                    // 2px neon leading marker
                    quads.extend_from_slice(&build_quad_vertices(
                        panel_x + 4.0,
                        row_y,
                        2.0,
                        COMMIT_ROW_HEIGHT - 2.0,
                        accent_color,
                    ));
                }

                // Row bottom separator line
                quads.extend_from_slice(&build_quad_vertices(
                    panel_x + 10.0,
                    row_y + COMMIT_ROW_HEIGHT - 1.0,
                    panel_width - 20.0,
                    1.0,
                    [0.06, 0.07, 0.09, 0.6],
                ));
            }
        }
    }

    quads
}

/// Builds text labels for the changes panel: tab names, file counts, diff badges, checkboxes,
/// and commit history entries.
#[must_use]
#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
pub fn build_changes_panel_labels(
    panel_x: f32,
    panel_y: f32,
    panel_width: f32,
    panel_height: f32,
    state: &ChangesPanelState,
) -> Vec<TabLabel> {
    let mut labels = Vec::new();

    // 1. Tab headers
    let tab_w = panel_width / 2.0;
    let changes_title = format!("Changes ({})", state.changed_files.len());
    let history_title = format!("History ({})", state.history.len());
    let is_changes = state.active_tab == ChangesTab::Changes;

    labels.push(TabLabel {
        text: changes_title,
        x: panel_x + 16.0,
        y: panel_y + 8.0,
        color: if is_changes {
            GlyphonColor::rgb(240, 245, 255)
        } else {
            GlyphonColor::rgb(130, 135, 150)
        },
        bold: is_changes,
        max_width: tab_w - 20.0,
    });

    labels.push(TabLabel {
        text: history_title,
        x: panel_x + tab_w + 16.0,
        y: panel_y + 8.0,
        color: if is_changes {
            GlyphonColor::rgb(130, 135, 150)
        } else {
            GlyphonColor::rgb(240, 245, 255)
        },
        bold: !is_changes,
        max_width: tab_w - 20.0,
    });

    // 2. Render depending on active tab
    match state.active_tab {
        ChangesTab::Changes => {
            // Summary & Stage All header
            let subheader_y = panel_y + CHANGES_HEADER_HEIGHT + 8.0;
            let total_add: usize = state.changed_files.iter().map(|f| f.additions).sum();
            let total_del: usize = state.changed_files.iter().map(|f| f.deletions).sum();
            let diff_stat_text = format!("Diff +{total_add} -{total_del}");

            labels.push(TabLabel {
                text: diff_stat_text,
                x: panel_x + 10.0,
                y: subheader_y,
                color: GlyphonColor::rgb(140, 145, 160),
                bold: false,
                max_width: 170.0,
            });

            labels.push(TabLabel {
                text: "Stage All".to_string(),
                x: panel_x + panel_width - 74.0,
                y: subheader_y,
                color: GlyphonColor::rgb(220, 225, 240),
                bold: true,
                max_width: 60.0,
            });

            // Section title
            labels.push(TabLabel {
                text: "Tracked Files".to_string(),
                x: panel_x + 10.0,
                y: subheader_y + 24.0,
                color: GlyphonColor::rgb(110, 115, 130),
                bold: true,
                max_width: 120.0,
            });

            // Changed file rows
            let list_start_y = panel_y + CHANGES_HEADER_HEIGHT + 56.0;
            for (idx, file) in state.changed_files.iter().enumerate() {
                let row_y = list_start_y + (idx as f32 * FILE_ROW_HEIGHT);
                if row_y + FILE_ROW_HEIGHT > panel_y + panel_height - COMMIT_AREA_HEIGHT {
                    break;
                }

                let is_selected = state.selected_file_index == Some(idx);
                let name_text = format!("{} {}", file.filename, file.path_info);
                labels.push(TabLabel {
                    text: name_text,
                    x: panel_x + 14.0,
                    y: row_y + 2.0,
                    color: if is_selected {
                        GlyphonColor::rgb(245, 248, 255)
                    } else {
                        GlyphonColor::rgb(175, 180, 195)
                    },
                    bold: is_selected,
                    max_width: panel_width - 90.0,
                });

                // Additions / Deletions badge e.g. +1 -1
                let stat_text = if file.additions > 0 || file.deletions > 0 {
                    format!("+{} -{}", file.additions, file.deletions)
                } else {
                    "±0".to_string()
                };
                labels.push(TabLabel {
                    text: stat_text,
                    x: panel_x + panel_width - 80.0,
                    y: row_y + 2.0,
                    color: GlyphonColor::rgb(53, 208, 127), // Success green
                    bold: false,
                    max_width: 50.0,
                });

                // Checkbox checkmark symbol
                if file.staged {
                    labels.push(TabLabel {
                        text: "✓".to_string(),
                        x: panel_x + panel_width - 23.0,
                        y: row_y + 2.0,
                        color: GlyphonColor::rgb(0, 0, 0),
                        bold: true,
                        max_width: 14.0,
                    });
                }
            }

            // Commit Area controls
            let commit_y = panel_y + panel_height - COMMIT_AREA_HEIGHT;
            let branch_text = format!(" {} · Fetch", state.git_branch);
            labels.push(TabLabel {
                text: branch_text,
                x: panel_x + 10.0,
                y: commit_y + 8.0,
                color: GlyphonColor::rgb(0, 240, 255), // Retro electric cyan
                bold: true,
                max_width: panel_width - 20.0,
            });

            // Commit placeholder
            labels.push(TabLabel {
                text: "Enter commit message".to_string(),
                x: panel_x + 18.0,
                y: commit_y + 37.0,
                color: GlyphonColor::rgb(105, 110, 125),
                bold: false,
                max_width: panel_width - 40.0,
            });

            // Commit button text
            labels.push(TabLabel {
                text: "Commit Tracked".to_string(),
                x: panel_x + panel_width - 110.0,
                y: commit_y + 70.0,
                color: GlyphonColor::rgb(220, 225, 240),
                bold: true,
                max_width: 100.0,
            });
        }
        ChangesTab::History => {
            // History subheader
            let subheader_y = panel_y + CHANGES_HEADER_HEIGHT + 8.0;
            labels.push(TabLabel {
                text: format!("Commit History ({} commits)", state.history.len()),
                x: panel_x + 10.0,
                y: subheader_y,
                color: GlyphonColor::rgb(140, 145, 160),
                bold: false,
                max_width: panel_width - 20.0,
            });

            // Render commit rows
            let list_start_y = panel_y + CHANGES_HEADER_HEIGHT + 36.0;
            for (idx, commit) in state.history.iter().enumerate() {
                let row_y = list_start_y + (idx as f32 * COMMIT_ROW_HEIGHT);
                if row_y + COMMIT_ROW_HEIGHT > panel_y + panel_height {
                    break;
                }

                // Line 1: short hash (cyan), relative time, author
                labels.push(TabLabel {
                    text: commit.hash.clone(),
                    x: panel_x + 12.0,
                    y: row_y + 3.0,
                    color: GlyphonColor::rgb(0, 240, 255), // Electric cyan
                    bold: true,
                    max_width: 60.0,
                });

                let meta_text = format!("{} · {}", commit.relative_time, commit.author);
                labels.push(TabLabel {
                    text: meta_text,
                    x: panel_x + 76.0,
                    y: row_y + 3.0,
                    color: GlyphonColor::rgb(130, 140, 160),
                    bold: false,
                    max_width: panel_width - 86.0,
                });

                // Line 2: commit subject message
                labels.push(TabLabel {
                    text: commit.message.clone(),
                    x: panel_x + 12.0,
                    y: row_y + 22.0,
                    color: GlyphonColor::rgb(220, 225, 240),
                    bold: false,
                    max_width: panel_width - 24.0,
                });
            }
        }
    }

    labels
}

/// Determines which control was clicked in the changes panel at (x, y).
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn hit_test_changes_panel(
    x: f32,
    y: f32,
    panel_x: f32,
    panel_y: f32,
    panel_width: f32,
    panel_height: f32,
    state: &ChangesPanelState,
) -> ChangesAction {
    if x < panel_x || x > panel_x + panel_width || y < panel_y || y > panel_y + panel_height {
        return ChangesAction::None;
    }

    // 1. Tab switching
    if y <= panel_y + CHANGES_HEADER_HEIGHT {
        let tab_w = panel_width / 2.0;
        if x < panel_x + tab_w {
            return ChangesAction::SwitchTab(ChangesTab::Changes);
        }
        return ChangesAction::SwitchTab(ChangesTab::History);
    }

    // 2. Tab content clicks
    match state.active_tab {
        ChangesTab::Changes => {
            // Stage All button
            let subheader_y = panel_y + CHANGES_HEADER_HEIGHT + 6.0;
            if y >= subheader_y && y <= subheader_y + 24.0 && x >= panel_x + panel_width - 80.0 {
                return ChangesAction::StageAll;
            }

            // Changed file rows & checkboxes
            let list_start_y = panel_y + CHANGES_HEADER_HEIGHT + 56.0;
            let commit_y = panel_y + panel_height - COMMIT_AREA_HEIGHT;
            if y >= list_start_y && y < commit_y {
                let relative_y = y - list_start_y;
                let index = (relative_y / FILE_ROW_HEIGHT) as usize;
                if index < state.changed_files.len() {
                    // Check if checkbox was clicked (right 30px)
                    if x >= panel_x + panel_width - 30.0 {
                        return ChangesAction::ToggleStage(index);
                    }
                    return ChangesAction::SelectFile(index);
                }
            }

            // Commit button
            if y >= commit_y + 66.0 && y <= commit_y + 90.0 && x >= panel_x + panel_width - 120.0 {
                return ChangesAction::Commit;
            }

            // Branch line / fetch button
            if y >= commit_y + 4.0 && y <= commit_y + 24.0 {
                return ChangesAction::Fetch;
            }
        }
        ChangesTab::History => {
            let list_start_y = panel_y + CHANGES_HEADER_HEIGHT + 36.0;
            if y >= list_start_y {
                let relative_y = y - list_start_y;
                let index = (relative_y / COMMIT_ROW_HEIGHT) as usize;
                if index < state.history.len() {
                    return ChangesAction::SelectCommit(index);
                }
            }
        }
    }

    ChangesAction::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_changes_panel_state_has_files_and_history() {
        let state = ChangesPanelState::default();
        assert_eq!(state.active_tab, ChangesTab::Changes);
        assert_ne!(state.changed_files.len(), 0);
        assert_ne!(state.history.len(), 0);
        assert_eq!(state.git_branch, "main");
    }

    #[test]
    fn assemble_changes_panel_quads_generates_quads() {
        let state = ChangesPanelState::default();
        let quads = assemble_changes_panel_quads(
            500.0,
            36.0,
            CHANGES_PANEL_WIDTH,
            600.0,
            [0.0, 1.0, 0.5, 1.0],
            &state,
        );
        assert_ne!(quads.len(), 0);
    }

    #[test]
    fn build_changes_panel_labels_contains_files_and_buttons() {
        let state = ChangesPanelState::default();
        let labels = build_changes_panel_labels(500.0, 36.0, CHANGES_PANEL_WIDTH, 600.0, &state);
        assert_ne!(labels.len(), 0);
        assert!(labels.iter().any(|l| l.text.contains("Changes")));
        assert!(labels.iter().any(|l| l.text.contains("History")));
        assert!(labels.iter().any(|l| l.text.contains("sw.js")));
        assert!(labels.iter().any(|l| l.text.contains("Stage All")));
        assert!(labels.iter().any(|l| l.text.contains("Commit Tracked")));
    }

    #[test]
    fn build_changes_panel_history_labels_renders_commit_hashes() {
        let state = ChangesPanelState {
            active_tab: ChangesTab::History,
            ..ChangesPanelState::default()
        };
        let labels = build_changes_panel_labels(500.0, 36.0, CHANGES_PANEL_WIDTH, 600.0, &state);
        assert_ne!(labels.len(), 0);
        assert!(labels.iter().any(|l| l.text.contains("1b60625")));
        assert!(labels.iter().any(|l| l.text.contains("53e1aac")));
    }

    #[test]
    fn hit_test_changes_panel_detects_tabs_and_rows() {
        let state = ChangesPanelState::default();
        let panel_x = 500.0;
        let panel_y = 36.0;
        let w = CHANGES_PANEL_WIDTH;
        let h = 600.0;

        // Click History tab
        let tab_action = hit_test_changes_panel(
            panel_x + w - 20.0,
            panel_y + 10.0,
            panel_x,
            panel_y,
            w,
            h,
            &state,
        );
        assert_eq!(tab_action, ChangesAction::SwitchTab(ChangesTab::History));

        // Click file row 0
        let list_y = panel_y + CHANGES_HEADER_HEIGHT + 56.0 + 5.0;
        let row_action =
            hit_test_changes_panel(panel_x + 20.0, list_y, panel_x, panel_y, w, h, &state);
        assert_eq!(row_action, ChangesAction::SelectFile(0));

        // Click checkbox for row 0
        let cb_action =
            hit_test_changes_panel(panel_x + w - 10.0, list_y, panel_x, panel_y, w, h, &state);
        assert_eq!(cb_action, ChangesAction::ToggleStage(0));
    }
}
