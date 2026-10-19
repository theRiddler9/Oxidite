//! Settings modal component providing an interactive overlay for themes, fonts, cursor, and layout.

use crate::renderer::draw::{QuadVertex, TabLabel, build_quad_vertices};
use glyphon::Color as GlyphonColor;

/// Available cursor shape options in settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorShapeChoice {
    Block,
    Underline,
    Beam,
}

impl CursorShapeChoice {
    /// Converts this choice into the terminal grid's `CursorShape`.
    #[must_use]
    pub const fn to_cursor_shape(self) -> crate::grid::CursorShape {
        match self {
            Self::Block => crate::grid::CursorShape::Block,
            Self::Underline => crate::grid::CursorShape::Underline,
            Self::Beam => crate::grid::CursorShape::Bar,
        }
    }
}

/// Available window padding options in settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaddingChoice {
    Compact,
    Normal,
    Roomy,
}

impl PaddingChoice {
    /// Returns the pixel padding values for `(pad_x, pad_y)`.
    #[must_use]
    pub const fn pixels(self) -> (f32, f32) {
        match self {
            Self::Compact => (4.0, 4.0),
            Self::Normal => (8.0, 8.0),
            Self::Roomy => (16.0, 16.0),
        }
    }
}

/// Information rendered inside the settings modal dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsModalState {
    pub is_open: bool,
    pub active_theme: String,
    pub font_size: f32,
    pub zoom_pct: usize,
    pub cursor_shape: CursorShapeChoice,
    pub cursor_blink: bool,
    pub padding: PaddingChoice,
    pub status_bar_visible: bool,
}

impl Default for SettingsModalState {
    fn default() -> Self {
        Self {
            is_open: false,
            active_theme: "pitch-black".to_string(),
            font_size: 14.0,
            zoom_pct: 100,
            cursor_shape: CursorShapeChoice::Block,
            cursor_blink: true,
            padding: PaddingChoice::Normal,
            status_bar_visible: true,
        }
    }
}

/// Action dispatched by interacting with the settings modal overlay.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingsAction {
    None,
    SelectTheme(String),
    DecreaseFontSize,
    IncreaseFontSize,
    ResetFontSize,
    SetCursorShape(CursorShapeChoice),
    ToggleCursorBlink,
    SetPadding(PaddingChoice),
    ToggleStatusBar,
    Close,
}

/// Available preset themes with their user-friendly display labels and system identifiers.
pub const PRESET_THEMES: &[(&str, &str)] = &[
    ("pitch-black", "Pitch Black Retro"),
    ("bright-white", "Bright White"),
    ("dracula", "Dracula"),
    ("solarized-dark", "Solarized Dark"),
    ("solarized-light", "Solarized Light"),
    ("vampiric", "Vampiric (Red/Black)"),
    ("graphite", "Graphite"),
    ("sunset", "Sunset"),
    ("nord", "Nord"),
    ("one-dark", "One Dark"),
    ("gruvbox-dark", "Gruvbox Dark"),
];

/// Modal dimensions in logical pixels.
pub const MODAL_WIDTH: f32 = 560.0;
pub const MODAL_HEIGHT: f32 = 470.0;

/// Precomputed layout bounding boxes for settings modal buttons and chips.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsLayout {
    pub modal_rect: (f32, f32, f32, f32),
    pub close_btn_rect: (f32, f32, f32, f32),
    pub theme_chips: Vec<(String, (f32, f32, f32, f32))>,
    pub font_dec_rect: (f32, f32, f32, f32),
    pub font_inc_rect: (f32, f32, f32, f32),
    pub font_reset_rect: (f32, f32, f32, f32),
    pub cursor_shape_block: (f32, f32, f32, f32),
    pub cursor_shape_under: (f32, f32, f32, f32),
    pub cursor_shape_beam: (f32, f32, f32, f32),
    pub cursor_blink_rect: (f32, f32, f32, f32),
    pub pad_compact_rect: (f32, f32, f32, f32),
    pub pad_normal_rect: (f32, f32, f32, f32),
    pub pad_roomy_rect: (f32, f32, f32, f32),
    pub status_bar_toggle_rect: (f32, f32, f32, f32),
}

impl SettingsLayout {
    #[must_use]
    #[allow(clippy::similar_names, clippy::cast_precision_loss)]
    pub fn compute(screen_w: f32, screen_h: f32) -> Self {
        let modal_w = MODAL_WIDTH.min(screen_w - 20.0);
        let modal_h = MODAL_HEIGHT.min(screen_h - 20.0);
        let modal_x = ((screen_w - modal_w) / 2.0).max(10.0);
        let modal_y = ((screen_h - modal_h) / 2.0).max(10.0);

        let close_w = 26.0;
        let close_h = 24.0;
        let close_btn_rect = (
            modal_x + modal_w - close_w - 12.0,
            modal_y + 10.0,
            close_w,
            close_h,
        );

        // Themes section: 3 columns of chips
        let mut theme_chips = Vec::with_capacity(PRESET_THEMES.len());
        let theme_start_y = modal_y + 70.0;
        let chip_w = (modal_w - 48.0) / 3.0;
        let chip_h = 26.0;
        for (i, (id, _label)) in PRESET_THEMES.iter().enumerate() {
            let col = (i % 3) as f32;
            let row = (i / 3) as f32;
            let cx = modal_x + 18.0 + col * (chip_w + 6.0);
            let cy = theme_start_y + row * (chip_h + 6.0);
            theme_chips.push(((*id).to_string(), (cx, cy, chip_w, chip_h)));
        }

        // Typography section
        let font_y = theme_start_y + 4.0 * (chip_h + 6.0) + 38.0;
        let font_btn_w = 34.0;
        let font_btn_h = 24.0;
        let font_dec_rect = (modal_x + 240.0, font_y - 4.0, font_btn_w, font_btn_h);
        let font_inc_rect = (modal_x + 280.0, font_y - 4.0, font_btn_w, font_btn_h);
        let font_reset_rect = (modal_x + 322.0, font_y - 4.0, 54.0, font_btn_h);

        // Cursor & Layout section
        let cursor_y = font_y + 54.0;
        let c_chip_w = 82.0;
        let c_chip_h = 25.0;
        let cursor_shape_block = (modal_x + 18.0, cursor_y, c_chip_w, c_chip_h);
        let cursor_shape_under = (
            modal_x + 18.0 + c_chip_w + 6.0,
            cursor_y,
            c_chip_w + 14.0,
            c_chip_h,
        );
        let cursor_shape_beam = (
            modal_x + 18.0 + (c_chip_w * 2.0) + 26.0,
            cursor_y,
            c_chip_w,
            c_chip_h,
        );
        let cursor_blink_rect = (
            modal_x + 18.0 + (c_chip_w * 3.0) + 38.0,
            cursor_y,
            110.0,
            c_chip_h,
        );

        // Padding & Status Bar section
        let layout_y = cursor_y + 54.0;
        let p_chip_w = 100.0;
        let pad_compact_rect = (modal_x + 18.0, layout_y, p_chip_w, c_chip_h);
        let pad_normal_rect = (
            modal_x + 18.0 + p_chip_w + 6.0,
            layout_y,
            p_chip_w,
            c_chip_h,
        );
        let pad_roomy_rect = (
            modal_x + 18.0 + (p_chip_w * 2.0) + 12.0,
            layout_y,
            p_chip_w,
            c_chip_h,
        );
        let status_bar_toggle_rect = (
            modal_x + 18.0 + (p_chip_w * 3.0) + 18.0,
            layout_y,
            115.0,
            c_chip_h,
        );

        Self {
            modal_rect: (modal_x, modal_y, modal_w, modal_h),
            close_btn_rect,
            theme_chips,
            font_dec_rect,
            font_inc_rect,
            font_reset_rect,
            cursor_shape_block,
            cursor_shape_under,
            cursor_shape_beam,
            cursor_blink_rect,
            pad_compact_rect,
            pad_normal_rect,
            pad_roomy_rect,
            status_bar_toggle_rect,
        }
    }
}

/// Assembles colored quads for the semi-transparent backdrop, modal dialog card, section dividers,
/// and interactive theme/setting chips.
#[must_use]
#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
pub fn assemble_settings_modal_quads(
    screen_w: f32,
    screen_h: f32,
    state: &SettingsModalState,
    accent_color: [f32; 4],
) -> Vec<QuadVertex> {
    if !state.is_open {
        return Vec::new();
    }

    let mut quads = Vec::new();
    let layout = SettingsLayout::compute(screen_w, screen_h);
    let (mx, my, mw, mh) = layout.modal_rect;

    // 1. Semi-transparent dark background backdrop
    quads.extend_from_slice(&build_quad_vertices(
        0.0,
        0.0,
        screen_w,
        screen_h,
        [0.0, 0.0, 0.0, 0.65],
    ));

    // 2. Modal card background (#07080b)
    quads.extend_from_slice(&build_quad_vertices(
        mx,
        my,
        mw,
        mh,
        [0.028, 0.032, 0.042, 0.98],
    ));

    // Outer border (1px #1d212a)
    quads.extend_from_slice(&build_quad_vertices(
        mx,
        my,
        mw,
        1.0,
        [0.15, 0.17, 0.22, 1.0],
    ));
    quads.extend_from_slice(&build_quad_vertices(
        mx,
        my + mh - 1.0,
        mw,
        1.0,
        [0.15, 0.17, 0.22, 1.0],
    ));
    quads.extend_from_slice(&build_quad_vertices(
        mx,
        my,
        1.0,
        mh,
        [0.15, 0.17, 0.22, 1.0],
    ));
    quads.extend_from_slice(&build_quad_vertices(
        mx + mw - 1.0,
        my,
        1.0,
        mh,
        [0.15, 0.17, 0.22, 1.0],
    ));

    // Header top accent strip (2px)
    quads.extend_from_slice(&build_quad_vertices(mx, my, mw, 2.5, accent_color));

    // Header bottom separator line
    quads.extend_from_slice(&build_quad_vertices(
        mx,
        my + 44.0,
        mw,
        1.0,
        [0.11, 0.13, 0.18, 0.9],
    ));

    // Close button box
    let cb = layout.close_btn_rect;
    quads.extend_from_slice(&build_quad_vertices(
        cb.0,
        cb.1,
        cb.2,
        cb.3,
        [0.08, 0.10, 0.14, 0.9],
    ));

    // 3. Theme chips
    for (id, (cx, cy, cw, ch)) in &layout.theme_chips {
        let is_active = state.active_theme.eq_ignore_ascii_case(id);
        let bg_color = if is_active {
            [
                accent_color[0] * 0.22,
                accent_color[1] * 0.22,
                accent_color[2] * 0.22,
                0.9,
            ]
        } else {
            [0.06, 0.07, 0.10, 0.85]
        };
        quads.extend_from_slice(&build_quad_vertices(*cx, *cy, *cw, *ch, bg_color));

        if is_active {
            // Left active accent indicator (3px)
            quads.extend_from_slice(&build_quad_vertices(*cx, *cy, 3.0, *ch, accent_color));
        }
    }

    // 4. Typography controls
    let fdec = layout.font_dec_rect;
    quads.extend_from_slice(&build_quad_vertices(
        fdec.0,
        fdec.1,
        fdec.2,
        fdec.3,
        [0.08, 0.10, 0.14, 0.9],
    ));
    let finc = layout.font_inc_rect;
    quads.extend_from_slice(&build_quad_vertices(
        finc.0,
        finc.1,
        finc.2,
        finc.3,
        [0.08, 0.10, 0.14, 0.9],
    ));
    let freset = layout.font_reset_rect;
    quads.extend_from_slice(&build_quad_vertices(
        freset.0,
        freset.1,
        freset.2,
        freset.3,
        [0.08, 0.10, 0.14, 0.9],
    ));

    // 5. Cursor Shape chips
    let render_chip = |rect: (f32, f32, f32, f32), active: bool, q: &mut Vec<QuadVertex>| {
        let bg = if active {
            [
                accent_color[0] * 0.22,
                accent_color[1] * 0.22,
                accent_color[2] * 0.22,
                0.9,
            ]
        } else {
            [0.06, 0.07, 0.10, 0.85]
        };
        q.extend_from_slice(&build_quad_vertices(rect.0, rect.1, rect.2, rect.3, bg));
        if active {
            q.extend_from_slice(&build_quad_vertices(
                rect.0,
                rect.1,
                3.0,
                rect.3,
                accent_color,
            ));
        }
    };

    render_chip(
        layout.cursor_shape_block,
        state.cursor_shape == CursorShapeChoice::Block,
        &mut quads,
    );
    render_chip(
        layout.cursor_shape_under,
        state.cursor_shape == CursorShapeChoice::Underline,
        &mut quads,
    );
    render_chip(
        layout.cursor_shape_beam,
        state.cursor_shape == CursorShapeChoice::Beam,
        &mut quads,
    );
    render_chip(layout.cursor_blink_rect, state.cursor_blink, &mut quads);

    // 6. Padding & Status Bar chips
    render_chip(
        layout.pad_compact_rect,
        state.padding == PaddingChoice::Compact,
        &mut quads,
    );
    render_chip(
        layout.pad_normal_rect,
        state.padding == PaddingChoice::Normal,
        &mut quads,
    );
    render_chip(
        layout.pad_roomy_rect,
        state.padding == PaddingChoice::Roomy,
        &mut quads,
    );
    render_chip(
        layout.status_bar_toggle_rect,
        state.status_bar_visible,
        &mut quads,
    );

    quads
}

/// Builds text labels for the settings modal dialog.
#[must_use]
#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
pub fn build_settings_modal_labels(
    screen_w: f32,
    screen_h: f32,
    state: &SettingsModalState,
) -> Vec<TabLabel> {
    if !state.is_open {
        return Vec::new();
    }

    let mut labels = Vec::new();
    let layout = SettingsLayout::compute(screen_w, screen_h);
    let (mx, my, mw, _) = layout.modal_rect;

    // Header Title
    labels.push(TabLabel {
        text: "⚙ Oxidite Settings & Appearance".to_string(),
        x: mx + 18.0,
        y: my + 14.0,
        color: GlyphonColor::rgb(240, 245, 255),
        bold: true,
        max_width: mw - 70.0,
    });

    // Close Button [ × ]
    labels.push(TabLabel {
        text: "×".to_string(),
        x: layout.close_btn_rect.0 + 8.0,
        y: layout.close_btn_rect.1 + 4.0,
        color: GlyphonColor::rgb(180, 185, 200),
        bold: true,
        max_width: 20.0,
    });

    // Section 1: Themes
    labels.push(TabLabel {
        text: "COLOR THEMES (CLICK TO SWITCH)".to_string(),
        x: mx + 18.0,
        y: my + 52.0,
        color: GlyphonColor::rgb(120, 140, 180),
        bold: true,
        max_width: mw - 36.0,
    });

    for (id, (_cx, _cy, _cw, _ch)) in &layout.theme_chips {
        if let Some((_, (cx, cy, cw, _))) = layout.theme_chips.iter().find(|(cid, _)| cid == id) {
            let is_active = state.active_theme.eq_ignore_ascii_case(id);
            let display_name = PRESET_THEMES
                .iter()
                .find(|(tid, _)| tid == id)
                .map_or(id.as_str(), |(_, name)| *name);

            let label_text = if is_active {
                format!("✓ {display_name}")
            } else {
                display_name.to_string()
            };

            labels.push(TabLabel {
                text: label_text,
                x: cx + 6.0,
                y: cy + 5.0,
                color: if is_active {
                    GlyphonColor::rgb(255, 255, 255)
                } else {
                    GlyphonColor::rgb(180, 190, 210)
                },
                bold: is_active,
                max_width: cw - 10.0,
            });
        }
    }

    // Section 2: Typography & Zoom
    let font_y = layout.font_dec_rect.1;
    labels.push(TabLabel {
        text: format!(
            "FONT SIZE: {:.1} pt  ({}%)",
            state.font_size, state.zoom_pct
        ),
        x: mx + 18.0,
        y: font_y + 4.0,
        color: GlyphonColor::rgb(220, 225, 240),
        bold: true,
        max_width: 210.0,
    });
    labels.push(TabLabel {
        text: "−".to_string(),
        x: layout.font_dec_rect.0 + 12.0,
        y: font_y + 4.0,
        color: GlyphonColor::rgb(240, 245, 255),
        bold: true,
        max_width: 20.0,
    });
    labels.push(TabLabel {
        text: "+".to_string(),
        x: layout.font_inc_rect.0 + 11.0,
        y: font_y + 4.0,
        color: GlyphonColor::rgb(240, 245, 255),
        bold: true,
        max_width: 20.0,
    });
    labels.push(TabLabel {
        text: "Reset".to_string(),
        x: layout.font_reset_rect.0 + 8.0,
        y: font_y + 4.0,
        color: GlyphonColor::rgb(180, 190, 210),
        bold: false,
        max_width: 44.0,
    });

    // Section 3: Cursor Shape
    let cursor_y = layout.cursor_shape_block.1 - 18.0;
    labels.push(TabLabel {
        text: "CURSOR SHAPE & BLINK".to_string(),
        x: mx + 18.0,
        y: cursor_y,
        color: GlyphonColor::rgb(120, 140, 180),
        bold: true,
        max_width: mw - 36.0,
    });

    let render_chip_label =
        |text: &str, rect: (f32, f32, f32, f32), active: bool, l: &mut Vec<TabLabel>| {
            l.push(TabLabel {
                text: text.to_string(),
                x: rect.0 + 8.0,
                y: rect.1 + 5.0,
                color: if active {
                    GlyphonColor::rgb(255, 255, 255)
                } else {
                    GlyphonColor::rgb(170, 180, 200)
                },
                bold: active,
                max_width: rect.2 - 12.0,
            });
        };

    render_chip_label(
        "Block █",
        layout.cursor_shape_block,
        state.cursor_shape == CursorShapeChoice::Block,
        &mut labels,
    );
    render_chip_label(
        "Underline _",
        layout.cursor_shape_under,
        state.cursor_shape == CursorShapeChoice::Underline,
        &mut labels,
    );
    render_chip_label(
        "Beam ▏",
        layout.cursor_shape_beam,
        state.cursor_shape == CursorShapeChoice::Beam,
        &mut labels,
    );
    render_chip_label(
        if state.cursor_blink {
            "Blink: ON"
        } else {
            "Blink: OFF"
        },
        layout.cursor_blink_rect,
        state.cursor_blink,
        &mut labels,
    );

    // Section 4: Window Layout & Padding
    let layout_y = layout.pad_compact_rect.1 - 18.0;
    labels.push(TabLabel {
        text: "PADDING & STATUS BAR".to_string(),
        x: mx + 18.0,
        y: layout_y,
        color: GlyphonColor::rgb(120, 140, 180),
        bold: true,
        max_width: mw - 36.0,
    });

    render_chip_label(
        "Compact (4px)",
        layout.pad_compact_rect,
        state.padding == PaddingChoice::Compact,
        &mut labels,
    );
    render_chip_label(
        "Normal (8px)",
        layout.pad_normal_rect,
        state.padding == PaddingChoice::Normal,
        &mut labels,
    );
    render_chip_label(
        "Roomy (16px)",
        layout.pad_roomy_rect,
        state.padding == PaddingChoice::Roomy,
        &mut labels,
    );
    render_chip_label(
        if state.status_bar_visible {
            "Status: ON"
        } else {
            "Status: OFF"
        },
        layout.status_bar_toggle_rect,
        state.status_bar_visible,
        &mut labels,
    );

    labels
}

/// Hit tests mouse clicks against the settings modal dialog and its interactive chips.
#[must_use]
pub fn hit_test_settings_modal(
    x: f32,
    y: f32,
    screen_w: f32,
    screen_h: f32,
    state: &SettingsModalState,
) -> SettingsAction {
    if !state.is_open {
        return SettingsAction::None;
    }

    let layout = SettingsLayout::compute(screen_w, screen_h);
    let (mx, my, mw, mh) = layout.modal_rect;

    // Click outside modal dismisses it
    if x < mx || x > mx + mw || y < my || y > my + mh {
        return SettingsAction::Close;
    }

    let in_rect =
        |r: (f32, f32, f32, f32)| x >= r.0 && x <= r.0 + r.2 && y >= r.1 && y <= r.1 + r.3;

    if in_rect(layout.close_btn_rect) {
        return SettingsAction::Close;
    }

    for (id, rect) in &layout.theme_chips {
        if in_rect(*rect) {
            return SettingsAction::SelectTheme(id.clone());
        }
    }

    if in_rect(layout.font_dec_rect) {
        return SettingsAction::DecreaseFontSize;
    }
    if in_rect(layout.font_inc_rect) {
        return SettingsAction::IncreaseFontSize;
    }
    if in_rect(layout.font_reset_rect) {
        return SettingsAction::ResetFontSize;
    }

    if in_rect(layout.cursor_shape_block) {
        return SettingsAction::SetCursorShape(CursorShapeChoice::Block);
    }
    if in_rect(layout.cursor_shape_under) {
        return SettingsAction::SetCursorShape(CursorShapeChoice::Underline);
    }
    if in_rect(layout.cursor_shape_beam) {
        return SettingsAction::SetCursorShape(CursorShapeChoice::Beam);
    }
    if in_rect(layout.cursor_blink_rect) {
        return SettingsAction::ToggleCursorBlink;
    }

    if in_rect(layout.pad_compact_rect) {
        return SettingsAction::SetPadding(PaddingChoice::Compact);
    }
    if in_rect(layout.pad_normal_rect) {
        return SettingsAction::SetPadding(PaddingChoice::Normal);
    }
    if in_rect(layout.pad_roomy_rect) {
        return SettingsAction::SetPadding(PaddingChoice::Roomy);
    }
    if in_rect(layout.status_bar_toggle_rect) {
        return SettingsAction::ToggleStatusBar;
    }

    SettingsAction::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_modal_state_defaults() {
        let state = SettingsModalState::default();
        assert!(!state.is_open);
        assert_eq!(state.active_theme, "pitch-black");
    }

    #[test]
    fn settings_layout_and_hit_testing() {
        let state = SettingsModalState {
            is_open: true,
            ..Default::default()
        };

        let action = hit_test_settings_modal(5.0, 5.0, 1000.0, 800.0, &state);
        assert_eq!(action, SettingsAction::Close);

        let layout = SettingsLayout::compute(1000.0, 800.0);
        let cb = layout.close_btn_rect;
        let close_action = hit_test_settings_modal(cb.0 + 5.0, cb.1 + 5.0, 1000.0, 800.0, &state);
        assert_eq!(close_action, SettingsAction::Close);

        let (first_id, first_rect) = &layout.theme_chips[0];
        let theme_action = hit_test_settings_modal(
            first_rect.0 + 5.0,
            first_rect.1 + 5.0,
            1000.0,
            800.0,
            &state,
        );
        assert_eq!(theme_action, SettingsAction::SelectTheme(first_id.clone()));
    }
}
