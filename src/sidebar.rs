//! Left collapsible workspace, repository, and session sidebar.

use crate::renderer::draw::{QuadVertex, TabLabel, build_quad_vertices};
use glyphon::Color as GlyphonColor;
use std::path::Path;

/// Width of the sidebar in window pixels when expanded (240--280px as per DESIGN.md).
pub const SIDEBAR_WIDTH: f32 = 240.0;
/// Height of the top workspace section header in the sidebar.
pub const SIDEBAR_HEADER_HEIGHT: f32 = 36.0;
/// Height of each workspace/repository item row.
pub const WORKSPACE_ROW_HEIGHT: f32 = 32.0;
/// Height of each session/tab item in the sidebar list.
pub const SIDEBAR_ITEM_HEIGHT: f32 = 26.0;
/// Height of each chat session row.
pub const CHAT_ROW_HEIGHT: f32 = 26.0;

/// Lifecycle status of an AI agent coding chat session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatStatus {
    /// Agent is currently executing tools or streaming response.
    Working,
    /// Agent session is waiting for user prompt.
    Idle,
    /// Agent session task has completed.
    Done,
}

/// A repository / workspace listed in the sidebar rail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceEntry {
    /// Name of repository or workspace folder (e.g. "Oxidite").
    pub name: String,
    /// Current git branch (e.g. "main").
    pub branch: String,
    /// Number of active terminal sessions or agents.
    pub sessions_count: usize,
    /// Number of modified files.
    pub modified_count: usize,
    /// Diff additions and deletions count (e.g. (+441, -440)).
    pub diff_stat: Option<(usize, usize)>,
    /// Whether this is the currently active workspace.
    pub is_active: bool,
}

/// A coding agent chat session listed in the lower section of the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatEntry {
    /// Concise title of the chat session.
    pub title: String,
    /// Current execution status.
    pub status: ChatStatus,
    /// Model name metadata (e.g. "Claude 3.7").
    pub model: String,
}

/// Action triggered by clicking within the sidebar area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarAction {
    /// No interactive item was clicked.
    None,
    /// Switch to the session/tab at index.
    SelectTab(usize),
    /// Close the session/tab at index.
    CloseTab(usize),
    /// Create a new session.
    NewSession,
    /// Toggle the sidebar collapsed/expanded.
    ToggleSidebar,
    /// Select a workspace at index.
    SelectWorkspace(usize),
    /// Select an agent chat session at index.
    SelectChat(usize),
}

/// State representation required to layout and render the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarState {
    /// Active workspace / folder name (e.g. "Oxidite").
    pub workspace_name: String,
    /// Detected git branch (e.g. "main"), if inside a git repository.
    pub git_branch: Option<String>,
    /// Git dirty status description (e.g. "clean" or "modified").
    pub git_status: Option<String>,
    /// List of tab/session titles.
    pub tab_titles: Vec<String>,
    /// Index of the active tab.
    pub active_tab_index: usize,
    /// Workspaces listed in the rail.
    pub workspaces: Vec<WorkspaceEntry>,
    /// Agent chat sessions listed in lower section.
    pub chats: Vec<ChatEntry>,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self {
            workspace_name: "Oxidite".to_string(),
            git_branch: Some("main".to_string()),
            git_status: Some("modified".to_string()),
            tab_titles: vec!["Tab 1".to_string()],
            active_tab_index: 0,
            workspaces: vec![WorkspaceEntry {
                name: "Oxidite (Rust)".to_string(),
                branch: "main".to_string(),
                sessions_count: 1,
                modified_count: 2,
                diff_stat: Some((171, 1)),
                is_active: true,
            }],
            chats: vec![
                ChatEntry {
                    title: "Implement pitch black UI".to_string(),
                    status: ChatStatus::Working,
                    model: "Claude 3.7".to_string(),
                },
                ChatEntry {
                    title: "Review service worker".to_string(),
                    status: ChatStatus::Done,
                    model: "Claude 3.7".to_string(),
                },
                ChatEntry {
                    title: "Wasm compilation error".to_string(),
                    status: ChatStatus::Idle,
                    model: "Claude 3.7".to_string(),
                },
            ],
        }
    }
}

/// Quickly detects the current git branch by inspecting `.git/HEAD` without blocking or spawning processes.
#[must_use]
pub fn detect_git_branch(cwd: &Path) -> Option<String> {
    let head_path = cwd.join(".git").join("HEAD");
    let content = std::fs::read_to_string(head_path).ok()?;
    let trimmed = content.trim();
    if let Some(branch) = trimmed.strip_prefix("ref: refs/heads/") {
        Some(branch.to_string())
    } else if trimmed.len() >= 7 {
        Some(trimmed[..7].to_string())
    } else {
        None
    }
}

/// Builds the quads for rendering the sidebar: pure pitch-black background (#000000), 1px border (#181a20),
/// active workspace and session highlight cards, and dividers.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn assemble_sidebar_quads(
    sidebar_y: f32,
    sidebar_height: f32,
    accent_color: [f32; 4],
    active_tab_index: usize,
    tab_count: usize,
) -> Vec<QuadVertex> {
    let mut quads = Vec::new();

    // 1. Sidebar background quad (Pure pitch black #000000)
    quads.extend_from_slice(&build_quad_vertices(
        0.0,
        sidebar_y,
        SIDEBAR_WIDTH,
        sidebar_height,
        [0.0, 0.0, 0.0, 1.0],
    ));

    // 2. Right vertical border dividing sidebar from terminal grid / agent pane (#181a20)
    quads.extend_from_slice(&build_quad_vertices(
        SIDEBAR_WIDTH - 1.0,
        sidebar_y,
        1.0,
        sidebar_height,
        [0.095, 0.105, 0.13, 1.0],
    ));

    // 3. Workspace header bottom divider line (#181a20)
    quads.extend_from_slice(&build_quad_vertices(
        0.0,
        sidebar_y + SIDEBAR_HEADER_HEIGHT - 1.0,
        SIDEBAR_WIDTH,
        1.0,
        [0.095, 0.105, 0.13, 1.0],
    ));

    // 4. Active tab session item highlight & left indicator strip
    if tab_count > 0 && active_tab_index < tab_count {
        let item_y = sidebar_y
            + SIDEBAR_HEADER_HEIGHT
            + 24.0
            + (active_tab_index as f32 * SIDEBAR_ITEM_HEIGHT);
        if item_y + SIDEBAR_ITEM_HEIGHT <= sidebar_y + sidebar_height {
            // Item background card (#0B0C10)
            quads.extend_from_slice(&build_quad_vertices(
                6.0,
                item_y,
                SIDEBAR_WIDTH - 12.0,
                SIDEBAR_ITEM_HEIGHT - 4.0,
                [0.045, 0.048, 0.065, 0.95],
            ));

            // Left neon phosphor indicator bar (2.5px wide)
            quads.extend_from_slice(&build_quad_vertices(
                6.0,
                item_y,
                2.5,
                SIDEBAR_ITEM_HEIGHT - 4.0,
                accent_color,
            ));
        }
    }

    quads
}

/// Builds formatted text labels for the sidebar workspace header, branch, sessions, and chats.
#[must_use]
#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
pub fn build_sidebar_labels(state: &SidebarState, sidebar_y: f32) -> Vec<TabLabel> {
    let mut labels = Vec::new();

    // 1. Workspace Section Header
    labels.push(TabLabel {
        text: "WORKSPACES".to_string(),
        x: 12.0,
        y: sidebar_y + 6.0,
        color: GlyphonColor::rgb(105, 110, 125),
        bold: true,
        max_width: SIDEBAR_WIDTH - 24.0,
    });

    // 2. Active Workspace Name & Branch
    let ws_text = if let Some(ref branch) = state.git_branch {
        format!("📁 {} ·  {}", state.workspace_name, branch)
    } else {
        format!("📁 {}", state.workspace_name)
    };
    labels.push(TabLabel {
        text: ws_text,
        x: 12.0,
        y: sidebar_y + 20.0,
        color: GlyphonColor::rgb(220, 225, 240),
        bold: true,
        max_width: SIDEBAR_WIDTH - 24.0,
    });

    // 3. Sessions Section Header
    let sessions_y = sidebar_y + SIDEBAR_HEADER_HEIGHT + 6.0;
    let sessions_header = format!("SESSIONS ({} active)", state.tab_titles.len());
    labels.push(TabLabel {
        text: sessions_header,
        x: 12.0,
        y: sessions_y,
        color: GlyphonColor::rgb(105, 110, 125),
        bold: true,
        max_width: SIDEBAR_WIDTH - 24.0,
    });

    // 4. Session Items
    let mut last_y = sessions_y + 18.0;
    for (idx, title) in state.tab_titles.iter().enumerate() {
        let item_y = sessions_y + 18.0 + (idx as f32 * SIDEBAR_ITEM_HEIGHT);
        last_y = item_y + SIDEBAR_ITEM_HEIGHT;
        let is_active = idx == state.active_tab_index;
        let (dot, dot_color) = if is_active {
            ("●", GlyphonColor::rgb(0, 255, 136)) // Retro neon phosphor green
        } else {
            ("○", GlyphonColor::rgb(120, 125, 140))
        };
        let text_color = if is_active {
            GlyphonColor::rgb(240, 245, 255)
        } else {
            GlyphonColor::rgb(150, 155, 170)
        };

        // Indicator dot
        labels.push(TabLabel {
            text: dot.to_string(),
            x: 14.0,
            y: item_y + 2.0,
            color: dot_color,
            bold: true,
            max_width: 16.0,
        });

        // Title
        labels.push(TabLabel {
            text: title.clone(),
            x: 28.0,
            y: item_y + 2.0,
            color: text_color,
            bold: is_active,
            max_width: SIDEBAR_WIDTH - 40.0,
        });
    }

    // 5. Chats Section (if configured)
    if !state.chats.is_empty() {
        let chats_header_y = last_y + 12.0;
        labels.push(TabLabel {
            text: format!("CHATS ({} active)", state.chats.len()),
            x: 12.0,
            y: chats_header_y,
            color: GlyphonColor::rgb(105, 110, 125),
            bold: true,
            max_width: SIDEBAR_WIDTH - 24.0,
        });

        for (c_idx, chat) in state.chats.iter().enumerate() {
            let row_y = chats_header_y + 18.0 + (c_idx as f32 * CHAT_ROW_HEIGHT);
            let (status_icon, status_color) = match chat.status {
                ChatStatus::Working => ("⚡", GlyphonColor::rgb(0, 240, 255)), // Cyan working
                ChatStatus::Idle => ("◌", GlyphonColor::rgb(140, 145, 160)),
                ChatStatus::Done => ("✓", GlyphonColor::rgb(53, 208, 127)), // Green done
            };

            // Status icon
            labels.push(TabLabel {
                text: status_icon.to_string(),
                x: 14.0,
                y: row_y + 2.0,
                color: status_color,
                bold: true,
                max_width: 16.0,
            });

            // Chat title
            labels.push(TabLabel {
                text: chat.title.clone(),
                x: 30.0,
                y: row_y + 2.0,
                color: GlyphonColor::rgb(210, 215, 230),
                bold: chat.status == ChatStatus::Working,
                max_width: SIDEBAR_WIDTH - 42.0,
            });
        }
    }

    labels
}

/// Determines which item was clicked in the sidebar at (x, y).
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn hit_test_sidebar(
    x: f32,
    y: f32,
    sidebar_y: f32,
    sidebar_height: f32,
    tab_count: usize,
) -> SidebarAction {
    if x < 0.0 || x > SIDEBAR_WIDTH || y < sidebar_y || y > sidebar_y + sidebar_height {
        return SidebarAction::None;
    }

    // Check if clicked inside sessions list
    let list_start_y = sidebar_y + SIDEBAR_HEADER_HEIGHT + 24.0;
    let list_end_y = list_start_y + (tab_count as f32 * SIDEBAR_ITEM_HEIGHT);
    if y >= list_start_y && y < list_end_y {
        let relative_y = y - list_start_y;
        let index = (relative_y / SIDEBAR_ITEM_HEIGHT) as usize;
        if index < tab_count {
            return SidebarAction::SelectTab(index);
        }
    }

    // Check if clicked inside chats section below sessions list
    let chats_start_y = list_end_y + 30.0;
    if y >= chats_start_y {
        let relative_y = y - chats_start_y;
        let chat_idx = (relative_y / CHAT_ROW_HEIGHT) as usize;
        return SidebarAction::SelectChat(chat_idx);
    }

    SidebarAction::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_git_branch_finds_current_repo_branch() {
        let cwd = std::env::current_dir().expect("valid cwd");
        let branch = detect_git_branch(&cwd);
        assert!(branch.is_some());
    }

    #[test]
    fn assemble_sidebar_quads_generates_quads() {
        let quads = assemble_sidebar_quads(36.0, 500.0, [0.0, 1.0, 0.5, 1.0], 0, 2);
        // 3 base quads (bg, right border, header divider) + 2 quads for active tab = 5 quads * 6 = 30 vertices
        assert_eq!(quads.len(), 30);
    }

    #[test]
    fn hit_test_sidebar_detects_session_selection() {
        let sidebar_y = 36.0;
        let sidebar_height = 500.0;
        let tab_count = 3;

        // Click outside width
        assert_eq!(
            hit_test_sidebar(300.0, 100.0, sidebar_y, sidebar_height, tab_count),
            SidebarAction::None
        );

        // Click item 0
        let item0_y = sidebar_y + SIDEBAR_HEADER_HEIGHT + 24.0 + 5.0;
        assert_eq!(
            hit_test_sidebar(50.0, item0_y, sidebar_y, sidebar_height, tab_count),
            SidebarAction::SelectTab(0)
        );

        // Click item 1
        let item1_y = item0_y + SIDEBAR_ITEM_HEIGHT;
        assert_eq!(
            hit_test_sidebar(50.0, item1_y, sidebar_y, sidebar_height, tab_count),
            SidebarAction::SelectTab(1)
        );
    }

    #[test]
    fn build_sidebar_labels_contains_workspace_and_sessions() {
        let state = SidebarState {
            workspace_name: "Oxidite".to_string(),
            git_branch: Some("main".to_string()),
            git_status: Some("clean".to_string()),
            tab_titles: vec!["Tab 1".to_string(), "Tab 2".to_string()],
            active_tab_index: 0,
            workspaces: Vec::new(),
            chats: Vec::new(),
        };
        let labels = build_sidebar_labels(&state, 36.0);
        assert_eq!(labels.len(), 7);
        assert_eq!(labels[0].text, "WORKSPACES");
        assert!(labels[1].text.contains("Oxidite"));
        assert!(labels[1].text.contains("main"));
        assert!(labels[2].text.contains("SESSIONS"));
    }
}
