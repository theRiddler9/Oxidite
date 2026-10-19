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
    /// Whether the settings modal overlay is currently open.
    pub settings_open: bool,
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
            settings_open: false,
        }
    }
}

/// Action dispatched by clicking within the status bar area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusBarAction {
    /// No interactive control was activated.
    None,
    /// Open the dedicated settings and appearance modal overlay.
    OpenSettings,
}

/// Generates background and badge quad vertices for the bottom status bar.
#[must_use]
pub fn assemble_status_bar_quads(
    window_width: f32,
    window_height: f32,
    accent_color: [f32; 4],
    state: &StatusBarState,
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
    let badge_bg = if state.is_alt_screen {
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

    // 4. Interactive Settings pill button background and border
    if window_width > 420.0 {
        let (btn_x, btn_y, btn_w, btn_h) =
            compute_settings_button_rect(window_width, window_height);
        let settings_bg = if state.settings_open {
            [
                accent_color[0] * 0.35,
                accent_color[1] * 0.35,
                accent_color[2] * 0.35,
                0.95,
            ]
        } else {
            [0.07, 0.08, 0.11, 0.9]
        };

        quads.extend_from_slice(&build_quad_vertices(
            btn_x,
            btn_y,
            btn_w,
            btn_h,
            settings_bg,
        ));
        // Top and bottom 1px borders
        quads.extend_from_slice(&build_quad_vertices(
            btn_x,
            btn_y,
            btn_w,
            1.0,
            [0.18, 0.20, 0.26, 0.8],
        ));
        quads.extend_from_slice(&build_quad_vertices(
            btn_x,
            btn_y + btn_h - 1.0,
            btn_w,
            1.0,
            [0.18, 0.20, 0.26, 0.8],
        ));
    }

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

    // Right-aligned status items (Cursor coordinates, Zoom, Settings pill, Tab count)
    if window_width > 420.0 {
        let (btn_x, btn_y, btn_w, _) = compute_settings_button_rect(window_width, window_height);

        // Coordinates & Zoom info placed to the left of the settings button
        let pos_info = format!(
            "Ln {}, Col {}  │  {}%",
            state.cursor_row, state.cursor_col, state.zoom_pct
        );
        let pos_w = pos_info.len() as f32 * 7.5;
        let pos_x = (btn_x - pos_w - 18.0).max(280.0);

        labels.push(TabLabel {
            text: pos_info,
            x: pos_x,
            y: text_y,
            color: GlyphonColor::rgb(155, 160, 175),
            bold: false,
            max_width: pos_w + 10.0,
        });

        // Clickable Settings button label
        labels.push(TabLabel {
            text: "⚙ Settings".to_string(),
            x: btn_x + 12.0,
            y: btn_y + 2.0,
            color: GlyphonColor::rgb(240, 245, 255),
            bold: true,
            max_width: btn_w - 12.0,
        });

        // Tab count on the far right
        let tab_info = format!("{}/{}", state.tab_current, state.tab_total);
        labels.push(TabLabel {
            text: tab_info,
            x: btn_x + btn_w + 10.0,
            y: text_y,
            color: GlyphonColor::rgb(140, 145, 160),
            bold: false,
            max_width: 40.0,
        });
    }

    labels
}

/// Rect dimensions `(x, y, width, height)` of the clickable Settings button in the status bar ribbon.
#[must_use]
pub fn compute_settings_button_rect(window_width: f32, window_height: f32) -> (f32, f32, f32, f32) {
    let bar_y = (window_height - STATUS_BAR_HEIGHT).max(0.0);
    let btn_w = 94.0;
    let btn_x = (window_width - btn_w - 55.0).max(220.0);
    (btn_x, bar_y + 3.0, btn_w, STATUS_BAR_HEIGHT - 6.0)
}

/// Hit tests clicking within the status bar area.
#[must_use]
pub fn hit_test_status_bar(
    x: f32,
    y: f32,
    window_width: f32,
    window_height: f32,
    _state: &StatusBarState,
) -> StatusBarAction {
    let (btn_x, btn_y, btn_w, btn_h) = compute_settings_button_rect(window_width, window_height);

    if x >= btn_x && x <= btn_x + btn_w && y >= btn_y && y <= btn_y + btn_h {
        return StatusBarAction::OpenSettings;
    }

    StatusBarAction::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assemble_status_bar_quads_generates_quads() {
        let state = StatusBarState::default();
        let quads = assemble_status_bar_quads(800.0, 600.0, [0.4, 0.6, 1.0, 1.0], &state);
        // 6 quads: background, top border, badge pill, settings pill bg, settings top border, settings bottom border
        assert_eq!(quads.len(), 36);

        // Narrow window: 3 quads (18 vertices)
        let narrow_quads = assemble_status_bar_quads(300.0, 600.0, [0.4, 0.6, 1.0, 1.0], &state);
        assert_eq!(narrow_quads.len(), 18);
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
            settings_open: false,
        };

        let labels = build_status_bar_labels(&state, 800.0, 600.0);
        assert_eq!(labels.len(), 5);
        assert_eq!(labels[0].text, "NORMAL");
        assert_eq!(labels[1].text, "⌁ pwsh");
        assert!(labels[2].text.contains("Ln 5, Col 10"));
        assert!(labels[2].text.contains("120%"));
        assert_eq!(labels[3].text, "⚙ Settings");
        assert_eq!(labels[4].text, "2/4");
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
        assert_eq!(labels.len(), 6);
        assert_eq!(labels[2].text, " feat/ui");
    }

    #[test]
    fn settings_button_and_hit_testing() {
        let state = StatusBarState::default();

        let (btn_x, btn_y, _btn_w, _btn_h) = compute_settings_button_rect(800.0, 600.0);
        let action = hit_test_status_bar(btn_x + 5.0, btn_y + 5.0, 800.0, 600.0, &state);
        assert_eq!(action, StatusBarAction::OpenSettings);

        let outside_action = hit_test_status_bar(10.0, 10.0, 800.0, 600.0, &state);
        assert_eq!(outside_action, StatusBarAction::None);
    }
}
