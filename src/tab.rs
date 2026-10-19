//! Multi-tab terminal management and tab-bar geometry.

use crate::input::{GridPoint, Selection};
use crate::parser::Parser;
use crate::pty::{PtySize, PtyWriter};
use crate::renderer::draw::{QuadVertex, build_quad_vertices};
use crate::terminal::Terminal;
use std::sync::{Arc, Mutex};

/// Height of the top tab bar in physical / logical pixels.
pub const TAB_BAR_HEIGHT: f32 = 34.0;
/// Preferred width of a single tab header.
pub const TAB_PREFERRED_WIDTH: f32 = 180.0;
/// Minimum width of a single tab header before truncation.
pub const TAB_MIN_WIDTH: f32 = 90.0;
/// Width/height of the new-tab '+' and close '×' action buttons.
pub const TAB_BUTTON_SIZE: f32 = 20.0;

/// Action triggered by clicking within the tab bar area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabBarAction {
    /// Switch active view to tab at the given index.
    SelectTab(usize),
    /// Close the tab at the given index.
    CloseTab(usize),
    /// Create and switch to a new tab.
    NewTab,
    /// Click fell on empty tab-bar background.
    None,
}

/// A single terminal tab instance owning its shell, VT parser, grid state, and selection.
pub struct Tab {
    /// Unique identifier for this tab.
    pub id: usize,
    /// Title displayed in the tab strip.
    pub title: String,
    /// Terminal state containing grid, cursor, modes, and scrollback.
    pub terminal: Terminal,
    /// VT escape sequence parser.
    pub parser: Parser,
    /// PTY master writer for sending input and resize signals.
    pub pty_writer: Option<Arc<Mutex<PtyWriter>>>,
    /// Whether the underlying shell process has exited.
    pub pty_exited: bool,
    /// Child process exit code if terminated.
    pub pty_exit_code: Option<i32>,
    /// Text selection range within this tab's grid.
    pub selection: Option<Selection>,
    /// Mouse drag start coordinate when selecting text in this tab.
    pub mouse_down_pos: Option<GridPoint>,
}

impl Tab {
    /// Creates a new `Tab` with initial terminal dimensions and default state.
    #[must_use]
    pub fn new(
        id: usize,
        title: String,
        terminal: Terminal,
        pty_writer: Option<Arc<Mutex<PtyWriter>>>,
    ) -> Self {
        Self {
            id,
            title,
            terminal,
            parser: Parser::new(),
            pty_writer,
            pty_exited: false,
            pty_exit_code: None,
            selection: None,
            mouse_down_pos: None,
        }
    }

    /// Feeds incoming PTY bytes into this tab's VT parser and updates terminal state.
    pub fn advance_parser(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.terminal, bytes);
        if !self.terminal.title.is_empty() {
            self.title = self.terminal.title.clone();
        }
    }

    /// Resizes the terminal grid and sends resize notification to the PTY.
    pub fn resize(&mut self, cols: usize, rows: usize, pty_size: PtySize) {
        self.terminal.resize(cols, rows);
        if let Some(ref writer) = self.pty_writer
            && let Ok(mut guard) = writer.lock()
        {
            let _ = guard.resize(pty_size);
        }
    }
}

/// Bounding box layout for a tab in the top tab bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabRect {
    /// X coordinate of tab top-left corner.
    pub x: f32,
    /// Y coordinate of tab top-left corner.
    pub y: f32,
    /// Total width of the tab header rectangle.
    pub width: f32,
    /// Height of the tab header rectangle.
    pub height: f32,
    /// X coordinate of the close '×' button.
    pub close_btn_x: f32,
    /// Y coordinate of the close '×' button.
    pub close_btn_y: f32,
    /// Size of the close '×' button.
    pub close_btn_size: f32,
}

/// Computes layout rectangles for all tabs and the plus button given total window width.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn compute_tab_layouts(
    count: usize,
    window_width: f32,
) -> (Vec<TabRect>, (f32, f32, f32, f32)) {
    let mut rects = Vec::with_capacity(count);
    let default_plus = (8.0, 7.0, TAB_BUTTON_SIZE, TAB_BUTTON_SIZE);
    if count == 0 {
        return (rects, default_plus);
    }

    let available_w = (window_width - 48.0).max(TAB_MIN_WIDTH);
    let tab_w = (available_w / count as f32).clamp(TAB_MIN_WIDTH, TAB_PREFERRED_WIDTH);
    let tab_h = TAB_BAR_HEIGHT - 6.0;

    for i in 0..count {
        let x = 6.0 + i as f32 * (tab_w + 4.0);
        let y = 4.0;
        let close_btn_x = x + tab_w - TAB_BUTTON_SIZE - 4.0;
        let close_btn_y = y + (tab_h - TAB_BUTTON_SIZE) / 2.0;

        rects.push(TabRect {
            x,
            y,
            width: tab_w,
            height: tab_h,
            close_btn_x,
            close_btn_y,
            close_btn_size: TAB_BUTTON_SIZE,
        });
    }

    let last_x = rects.last().map_or(6.0, |r| r.x + r.width + 6.0);
    let plus_btn = (last_x, 7.0, TAB_BUTTON_SIZE, TAB_BUTTON_SIZE);
    (rects, plus_btn)
}

/// Determines which tab or action button was clicked at pixel coordinates (x, y).
#[must_use]
pub fn hit_test(x: f32, y: f32, count: usize, window_width: f32) -> TabBarAction {
    if y > TAB_BAR_HEIGHT || y < 0.0 || count == 0 {
        return TabBarAction::None;
    }

    let (layouts, plus_btn) = compute_tab_layouts(count, window_width);

    // Check plus button
    if x >= plus_btn.0
        && x <= plus_btn.0 + plus_btn.2
        && y >= plus_btn.1
        && y <= plus_btn.1 + plus_btn.3
    {
        return TabBarAction::NewTab;
    }

    for (idx, layout) in layouts.iter().enumerate() {
        if x >= layout.x
            && x <= layout.x + layout.width
            && y >= layout.y
            && y <= layout.y + layout.height
        {
            // Check if close button clicked
            if x >= layout.close_btn_x
                && x <= layout.close_btn_x + layout.close_btn_size
                && y >= layout.close_btn_y
                && y <= layout.close_btn_y + layout.close_btn_size
            {
                return TabBarAction::CloseTab(idx);
            }
            return TabBarAction::SelectTab(idx);
        }
    }

    TabBarAction::None
}

/// Builds the vertices for rendering the tab bar background, tabs, active highlight, and dividers.
#[must_use]
pub fn assemble_tab_bar_quads(
    tab_count: usize,
    active_index: usize,
    window_width: f32,
    accent_color: [f32; 4],
) -> Vec<QuadVertex> {
    let mut quads = Vec::new();

    // 1. Tab bar background (Deep retro obsidian #050507)
    quads.extend_from_slice(&build_quad_vertices(
        0.0,
        0.0,
        window_width,
        TAB_BAR_HEIGHT,
        [0.02, 0.02, 0.028, 1.0],
    ));

    // 2. Bottom 1px divider (#14151D)
    quads.extend_from_slice(&build_quad_vertices(
        0.0,
        TAB_BAR_HEIGHT - 1.0,
        window_width,
        1.0,
        [0.08, 0.085, 0.115, 1.0],
    ));

    if tab_count == 0 {
        return quads;
    }

    let (layouts, plus_btn) = compute_tab_layouts(tab_count, window_width);

    for (idx, layout) in layouts.iter().enumerate() {
        let is_active = idx == active_index;
        let tab_bg = if is_active {
            [0.0, 0.0, 0.0, 1.0] // Pure pitch black active tab merges into terminal grid
        } else {
            [0.045, 0.048, 0.065, 1.0] // Recessed dark card #0B0C10
        };

        // Tab body quad
        quads.extend_from_slice(&build_quad_vertices(
            layout.x,
            layout.y,
            layout.width,
            layout.height,
            tab_bg,
        ));

        // Active tab indicator accent line at bottom
        if is_active {
            quads.extend_from_slice(&build_quad_vertices(
                layout.x,
                layout.y + layout.height - 2.5,
                layout.width,
                2.5,
                accent_color,
            ));
        }

        // Close button '×' background hover quad
        quads.extend_from_slice(&build_quad_vertices(
            layout.close_btn_x,
            layout.close_btn_y,
            layout.close_btn_size,
            layout.close_btn_size,
            [0.12, 0.13, 0.18, 0.6],
        ));
    }

    // Plus button '+' background quad
    quads.extend_from_slice(&build_quad_vertices(
        plus_btn.0,
        plus_btn.1,
        plus_btn.2,
        plus_btn.3,
        [0.06, 0.065, 0.09, 0.9],
    ));

    quads
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_tab_layouts_scales_with_count() {
        let (layouts, plus_btn) = compute_tab_layouts(3, 800.0);
        assert_eq!(layouts.len(), 3);
        assert_eq!(layouts[0].x, 6.0);
        assert!(layouts[1].x > layouts[0].x);
        assert!(layouts[2].x > layouts[1].x);
        assert!(plus_btn.0 > layouts[2].x + layouts[2].width);
    }

    #[test]
    fn hit_test_detects_tab_selection_close_and_new_tab() {
        let (layouts, plus_btn) = compute_tab_layouts(2, 800.0);

        // Click inside tab 0 body
        let action = hit_test(layouts[0].x + 10.0, layouts[0].y + 5.0, 2, 800.0);
        assert_eq!(action, TabBarAction::SelectTab(0));

        // Click close button on tab 0
        let close_action = hit_test(
            layouts[0].close_btn_x + 2.0,
            layouts[0].close_btn_y + 2.0,
            2,
            800.0,
        );
        assert_eq!(close_action, TabBarAction::CloseTab(0));

        // Click new tab button
        let new_tab_action = hit_test(plus_btn.0 + 2.0, plus_btn.1 + 2.0, 2, 800.0);
        assert_eq!(new_tab_action, TabBarAction::NewTab);

        // Click below tab bar -> None
        let below_action = hit_test(100.0, TAB_BAR_HEIGHT + 10.0, 2, 800.0);
        assert_eq!(below_action, TabBarAction::None);
    }

    #[test]
    fn assemble_tab_bar_quads_generates_correct_quad_count() {
        let quads = assemble_tab_bar_quads(2, 0, 800.0, [0.4, 0.6, 1.0, 1.0]);
        // Base bar (1) + bottom divider (1) + tab 0 body (1) + tab 0 indicator (1) + tab 0 close (1)
        // + tab 1 body (1) + tab 1 close (1) + plus btn (1) = 8 quads = 48 vertices
        assert_eq!(quads.len(), 8 * 6);
    }
}
