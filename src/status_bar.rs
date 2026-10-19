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
    /// Whether the interactive theme selection popover dropdown is open.
    pub theme_menu_open: bool,
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
            theme_menu_open: false,
        }
    }
}

/// Action dispatched by clicking within the status bar area or its popovers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusBarAction {
    /// No interactive control was activated.
    None,
    /// Toggle open/close status of the theme selection dropdown.
    ToggleThemeMenu,
    /// User selected a new theme from the dropdown menu.
    SelectTheme(String),
    /// User clicked outside, dismissing the open theme dropdown.
    CloseThemeMenu,
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

    // Theme dropdown popover item labels
    if state.theme_menu_open {
        let (menu_x, menu_y, menu_w, _) =
            compute_theme_menu_rect(window_width, window_height, state);
        let presets = crate::theme::Theme::builtin_presets();
        let row_h = 24.0;

        for (i, name) in presets.iter().enumerate() {
            let row_y = menu_y + 5.0 + (i as f32 * row_h);
            let is_active = *name == state.theme_name;
            let display_text = if is_active {
                format!("● {name}")
            } else {
                format!("  {name}")
            };
            let color = if is_active {
                GlyphonColor::rgb(255, 255, 255)
            } else {
                GlyphonColor::rgb(175, 180, 195)
            };

            labels.push(TabLabel {
                text: display_text,
                x: menu_x + 8.0,
                y: row_y + 4.0,
                color,
                bold: is_active,
                max_width: menu_w - 16.0,
            });
        }
    }

    labels
}

/// Rect dimensions `(x, y, width, height)` of the clickable theme label in the status bar ribbon.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn compute_theme_button_rect(
    window_width: f32,
    window_height: f32,
    state: &StatusBarState,
) -> (f32, f32, f32, f32) {
    let bar_y = (window_height - STATUS_BAR_HEIGHT).max(0.0);
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

    let prefix = format!(
        "Ln {}, Col {}  │  {}%  │  ",
        state.cursor_row, state.cursor_col, state.zoom_pct
    );
    let prefix_w = prefix.len() as f32 * 7.5;
    let theme_w = (state.theme_name.len() as f32 * 7.5) + 12.0;
    let theme_x = right_x + prefix_w - 4.0;
    (theme_x, bar_y + 2.0, theme_w, STATUS_BAR_HEIGHT - 4.0)
}

/// Rect dimensions `(x, y, width, height)` for the floating theme dropdown popover.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn compute_theme_menu_rect(
    window_width: f32,
    window_height: f32,
    state: &StatusBarState,
) -> (f32, f32, f32, f32) {
    let presets = crate::theme::Theme::builtin_presets();
    let row_height = 24.0;
    let menu_w = 175.0;
    let menu_h = (presets.len() as f32 * row_height) + 10.0;
    let bar_y = (window_height - STATUS_BAR_HEIGHT).max(0.0);

    let (btn_x, _, btn_w, _) = compute_theme_button_rect(window_width, window_height, state);
    let menu_x = (btn_x + btn_w - menu_w).clamp(8.0, (window_width - menu_w - 8.0).max(8.0));
    let menu_y = (bar_y - menu_h - 4.0).max(8.0);

    (menu_x, menu_y, menu_w, menu_h)
}

/// Assembles vertices for rendering the floating theme selection popover dropdown.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn assemble_theme_menu_quads(
    window_width: f32,
    window_height: f32,
    accent_color: [f32; 4],
    state: &StatusBarState,
) -> Vec<QuadVertex> {
    if !state.theme_menu_open {
        return Vec::new();
    }

    let mut quads = Vec::new();
    let (menu_x, menu_y, menu_w, menu_h) =
        compute_theme_menu_rect(window_width, window_height, state);
    let presets = crate::theme::Theme::builtin_presets();
    let row_h = 24.0;

    // 1. Menu popover background (pitch-black obsidian #060709)
    quads.extend_from_slice(&build_quad_vertices(
        menu_x,
        menu_y,
        menu_w,
        menu_h,
        [0.025, 0.028, 0.035, 0.98],
    ));

    // 2. Menu popover 1px border (#2d3240)
    quads.extend_from_slice(&build_quad_vertices(
        menu_x,
        menu_y,
        menu_w,
        1.0,
        [0.18, 0.20, 0.26, 1.0],
    ));
    quads.extend_from_slice(&build_quad_vertices(
        menu_x,
        menu_y + menu_h - 1.0,
        menu_w,
        1.0,
        [0.18, 0.20, 0.26, 1.0],
    ));
    quads.extend_from_slice(&build_quad_vertices(
        menu_x,
        menu_y,
        1.0,
        menu_h,
        [0.18, 0.20, 0.26, 1.0],
    ));
    quads.extend_from_slice(&build_quad_vertices(
        menu_x + menu_w - 1.0,
        menu_y,
        1.0,
        menu_h,
        [0.18, 0.20, 0.26, 1.0],
    ));

    // 3. Highlight quad for the active preset row
    for (i, name) in presets.iter().enumerate() {
        if *name == state.theme_name {
            let row_y = menu_y + 5.0 + (i as f32 * row_h);
            quads.extend_from_slice(&build_quad_vertices(
                menu_x + 4.0,
                row_y,
                menu_w - 8.0,
                row_h,
                [
                    accent_color[0] * 0.35,
                    accent_color[1] * 0.35,
                    accent_color[2] * 0.35,
                    0.85,
                ],
            ));
            break;
        }
    }

    quads
}

/// Hit tests clicking within the status bar area or its open theme dropdown popover.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn hit_test_status_bar(
    x: f32,
    y: f32,
    window_width: f32,
    window_height: f32,
    state: &StatusBarState,
) -> StatusBarAction {
    let presets = crate::theme::Theme::builtin_presets();
    let (btn_x, btn_y, btn_w, btn_h) =
        compute_theme_button_rect(window_width, window_height, state);

    if state.theme_menu_open {
        let (menu_x, menu_y, menu_w, menu_h) =
            compute_theme_menu_rect(window_width, window_height, state);

        if x >= menu_x && x <= menu_x + menu_w && y >= menu_y && y <= menu_y + menu_h {
            let offset_y = y - menu_y - 5.0;
            if offset_y >= 0.0 {
                let idx = (offset_y / 24.0).floor() as usize;
                if idx < presets.len() {
                    return StatusBarAction::SelectTheme(presets[idx].to_string());
                }
            }
            return StatusBarAction::None;
        }

        if x >= btn_x && x <= btn_x + btn_w && y >= btn_y && y <= btn_y + btn_h {
            return StatusBarAction::ToggleThemeMenu;
        }

        return StatusBarAction::CloseThemeMenu;
    }

    if x >= btn_x && x <= btn_x + btn_w && y >= btn_y && y <= btn_y + btn_h {
        return StatusBarAction::ToggleThemeMenu;
    }

    StatusBarAction::None
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
            theme_menu_open: false,
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

    #[test]
    fn theme_menu_popover_and_hit_testing() {
        let mut state = StatusBarState {
            theme_name: "pitch-black".to_string(),
            theme_menu_open: false,
            ..StatusBarState::default()
        };

        // When closed, clicking button toggles open
        let (btn_x, btn_y, _btn_w, _btn_h) = compute_theme_button_rect(800.0, 600.0, &state);
        let action = hit_test_status_bar(btn_x + 5.0, btn_y + 5.0, 800.0, 600.0, &state);
        assert_eq!(action, StatusBarAction::ToggleThemeMenu);

        // When open, assemble quads returns background + 4 border lines + active highlight = 6 quads (36 vertices)
        state.theme_menu_open = true;
        let quads = assemble_theme_menu_quads(800.0, 600.0, [0.4, 0.6, 1.0, 1.0], &state);
        assert_eq!(quads.len(), 6 * 6);

        // Labels include presets
        let labels = build_status_bar_labels(&state, 800.0, 600.0);
        let presets = crate::theme::Theme::builtin_presets();
        assert_eq!(labels.len(), 3 + presets.len());

        // Hit testing item inside popover
        let (menu_x, menu_y, _, _) = compute_theme_menu_rect(800.0, 600.0, &state);
        // First item row
        let select_action = hit_test_status_bar(menu_x + 10.0, menu_y + 10.0, 800.0, 600.0, &state);
        assert_eq!(
            select_action,
            StatusBarAction::SelectTheme(presets[0].to_string())
        );

        // Hit testing outside closes menu
        let outside_action = hit_test_status_bar(10.0, 10.0, 800.0, 600.0, &state);
        assert_eq!(outside_action, StatusBarAction::CloseThemeMenu);
    }
}
