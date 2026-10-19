//! Status bar component providing layout geometry, badges, and quad rendering.

use crate::renderer::draw::{QuadVertex, TabLabel, build_quad_vertices};
use glyphon::Color as GlyphonColor;

/// Fixed height of the bottom status bar in logical pixels.
pub const STATUS_BAR_HEIGHT: f32 = 24.0;

/// Information displayed inside the bottom status bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusBarState {
    /// Active shell or process name (e.g. "pwsh", "bash").
    pub shell_name: String,
    /// Whether the terminal is currently in alternate screen buffer mode.
    pub is_alt_screen: bool,
    /// 1-based cursor row coordinate.
    pub cursor_row: usize,
    /// 1-based cursor column coordinate.
    pub cursor_col: usize,
    /// Zoom level percentage (e.g. 100 for 100%).
    pub zoom_pct: usize,
    /// Active theme name.
    pub theme_name: String,
    /// 1-based active tab index.
    pub tab_current: usize,
    /// Total number of open tabs.
    pub tab_total: usize,
    /// Detected git branch (e.g. "main"), if inside a git repository.
    pub git_branch: Option<String>,
}

impl Default for StatusBarState {
    fn default() -> Self {
        Self {
            shell_name: "shell".to_string(),
            is_alt_screen: false,
            cursor_row: 1,
            cursor_col: 1,
            zoom_pct: 100,
            theme_name: "default-dark".to_string(),
            tab_current: 1,
            tab_total: 1,
            git_branch: None,
        }
    }
}

/// Generates background and badge quad vertices for the bottom status bar.
#[must_use]
pub fn assemble_status_bar_quads(
    window_width: f32,
    window_height: f32,
    is_alt_screen: bool,
    accent_color: [f32; 4],
) -> Vec<QuadVertex> {
    let mut quads = Vec::new();
    let bar_y = (window_height - STATUS_BAR_HEIGHT).max(0.0);

    // 1. Status bar background quad (Pure pitch black #000000)
    quads.extend_from_slice(&build_quad_vertices(
        0.0,
        bar_y,
        window_width,
        STATUS_BAR_HEIGHT,
        [0.0, 0.0, 0.0, 1.0],
    ));

    // 2. Top 1px border line (#181a20)
    quads.extend_from_slice(&build_quad_vertices(
        0.0,
        bar_y,
        window_width,
        1.0,
        [0.095, 0.105, 0.13, 1.0],
    ));

    // 3. Mode badge pill background
    let badge_bg = if is_alt_screen {
        [
            accent_color[0] * 0.4,
            accent_color[1] * 0.4,
            accent_color[2] * 0.4,
            0.9,
        ]
    } else {
        [0.05, 0.055, 0.075, 0.9]
    };

    quads.extend_from_slice(&build_quad_vertices(
        6.0,
        bar_y + 3.0,
        64.0,
        STATUS_BAR_HEIGHT - 6.0,
        badge_bg,
    ));

    quads
}

/// Builds formatted text labels for the bottom status bar.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn build_status_bar_labels(
    state: &StatusBarState,
    window_width: f32,
    window_height: f32,
) -> Vec<TabLabel> {
    let mut labels = Vec::new();
    let bar_y = (window_height - STATUS_BAR_HEIGHT).max(0.0);
    let text_y = bar_y + 4.0;

    // Mode badge text ("NORMAL" or "ALT-BUF")
    let (mode_text, mode_color) = if state.is_alt_screen {
        ("ALT-BUF", GlyphonColor::rgb(210, 160, 255))
    } else {
        ("NORMAL", GlyphonColor::rgb(61, 220, 151))
    };

    labels.push(TabLabel {
        text: mode_text.to_string(),
        x: 12.0,
        y: text_y,
        color: mode_color,
        bold: true,
        max_width: 56.0,
    });

    // Shell name badge
    let shell_label = format!("⌁ {}", state.shell_name);
    labels.push(TabLabel {
        text: shell_label,
        x: 78.0,
        y: text_y,
        color: GlyphonColor::rgb(138, 144, 162),
        bold: false,
        max_width: 140.0,
    });

    // Optional git branch badge (e.g. " main")
    if let Some(ref branch) = state.git_branch {
        labels.push(TabLabel {
            text: format!(" {branch}"),
            x: 160.0,
            y: text_y,
            color: GlyphonColor::rgb(0, 240, 255), // Retro electric cyan
            bold: false,
            max_width: 140.0,
        });
    }

    // Right-aligned status items (Cursor coordinates, Zoom, Theme, Tab count)
    if window_width > 420.0 {
        let right_info = format!(
            "Ln {}, Col {}  │  {}%  │  {}  │  {}/{}",
            state.cursor_row,
            state.cursor_col,
            state.zoom_pct,
            state.theme_name,
            state.tab_current,
            state.tab_total
        );
        let approx_w = (right_info.len() as f32 * 7.5).min(window_width * 0.6);
        let min_x = if state.git_branch.is_some() {
            280.0
        } else {
            220.0
        };
        let right_x = (window_width - approx_w - 12.0).max(min_x);

        labels.push(TabLabel {
            text: right_info,
            x: right_x,
            y: text_y,
            color: GlyphonColor::rgb(155, 160, 175),
            bold: false,
            max_width: approx_w + 12.0,
        });
    }

    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assemble_status_bar_quads_generates_three_quads() {
        let quads = assemble_status_bar_quads(800.0, 600.0, false, [0.4, 0.6, 1.0, 1.0]);
        // 3 quads: background, top border, badge pill = 3 * 6 = 18 vertices
        assert_eq!(quads.len(), 18);
    }

    #[test]
    fn build_status_bar_labels_contains_mode_shell_and_info() {
        let state = StatusBarState {
            shell_name: "pwsh".to_string(),
            is_alt_screen: false,
            cursor_row: 5,
            cursor_col: 10,
            zoom_pct: 120,
            theme_name: "dracula".to_string(),
            tab_current: 2,
            tab_total: 4,
            git_branch: None,
        };

        let labels = build_status_bar_labels(&state, 800.0, 600.0);
        assert_eq!(labels.len(), 3);
        assert_eq!(labels[0].text, "NORMAL");
        assert_eq!(labels[1].text, "⌁ pwsh");
        assert!(labels[2].text.contains("Ln 5, Col 10"));
        assert!(labels[2].text.contains("120%"));
        assert!(labels[2].text.contains("dracula"));
        assert!(labels[2].text.contains("2/4"));
    }

    #[test]
    fn build_status_bar_labels_narrow_window_skips_right_info() {
        let state = StatusBarState::default();
        let labels = build_status_bar_labels(&state, 300.0, 600.0);
        assert_eq!(labels.len(), 2);
    }

    #[test]
    fn build_status_bar_labels_with_git_branch_renders_branch_label() {
        let state = StatusBarState {
            git_branch: Some("feat/ui".to_string()),
            ..StatusBarState::default()
        };
        let labels = build_status_bar_labels(&state, 800.0, 600.0);
        assert_eq!(labels.len(), 4);
        assert_eq!(labels[2].text, " feat/ui");
    }
}
