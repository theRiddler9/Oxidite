//! Multi-tab terminal management and tab-bar geometry.

use crate::input::{GridPoint, Selection};
use crate::pane::{Pane, PaneTree, SplitDirection};
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
    /// Toggle the left collapsible workspace sidebar.
    ToggleSidebar,
    /// Toggle split workspace / pane view [◫].
    ToggleSplit,
    /// Toggle fullscreen / maximize window [⤢].
    ToggleMaximize,
    /// Click fell on empty tab-bar background.
    None,
}

/// AI Coding Agent or CLI detected inside an active tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentKind {
    /// Anthropic Claude Code agent.
    Claude,
    /// `OpenAI` Codex CLI agent.
    Codex,
    /// Google `DeepMind` Antigravity CLI agent (`agy`).
    Agy,
    /// `OpenCode` open-source coding agent.
    OpenCode,
    /// GitHub Copilot CLI agent.
    Copilot,
    /// Cognition Devin AI agent.
    Devin,
    /// Standard interactive terminal shell.
    Shell,
}

impl AgentKind {
    /// Detects the running agent from tab title, current command prompt, and buffer content.
    #[must_use]
    pub fn detect(title: &str, cursor_line: &str, terminal_content_sample: &str) -> Self {
        let t = title.to_ascii_lowercase();
        let cmd = cursor_line.to_ascii_lowercase();
        let s = terminal_content_sample.to_ascii_lowercase();

        let has_word = |text: &str, word: &str| -> bool {
            text.split(|c: char| {
                c.is_whitespace()
                    || c == '-'
                    || c == '_'
                    || c == ':'
                    || c == '>'
                    || c == '$'
                    || c == '%'
                    || c == '#'
                    || c == '('
                    || c == ')'
            })
            .any(|w| w == word || w == format!("{word}.exe"))
        };

        // 1. Antigravity CLI (agy)
        let is_agy = has_word(&t, "agy")
            || t.contains("antigravity")
            || has_word(&cmd, "agy")
            || cmd.contains("antigravity")
            || s.contains("antigravity cli")
            || s.contains("agy >")
            || s.contains("agy❯")
            || s.contains("⏣ agy");

        if is_agy {
            return AgentKind::Agy;
        }

        // 2. Claude Code
        let is_claude = has_word(&t, "claude")
            || has_word(&cmd, "claude")
            || s.contains("welcome to claude code")
            || s.contains("claude >")
            || s.contains("claude❯");

        if is_claude {
            return AgentKind::Claude;
        }

        // 3. OpenAI Codex
        let is_codex = has_word(&t, "codex")
            || has_word(&cmd, "codex")
            || s.contains("openai codex")
            || s.contains("codex >")
            || s.contains("codex❯");

        if is_codex {
            return AgentKind::Codex;
        }

        // 4. OpenCode
        let is_opencode = has_word(&t, "opencode")
            || has_word(&cmd, "opencode")
            || s.contains("opencode cli")
            || s.contains("opencode >");

        if is_opencode {
            return AgentKind::OpenCode;
        }

        // 5. GitHub Copilot CLI
        let is_copilot = (has_word(&t, "copilot") || has_word(&cmd, "copilot"))
            || (cmd.contains("gh") && cmd.contains("copilot"))
            || s.contains("github copilot cli");

        if is_copilot {
            return AgentKind::Copilot;
        }

        // 6. Devin AI
        let is_devin = has_word(&t, "devin")
            || has_word(&cmd, "devin")
            || s.contains("devin cli")
            || s.contains("devin >");

        if is_devin {
            return AgentKind::Devin;
        }

        AgentKind::Shell
    }

    /// Brand icon and display title prefix for this agent kind.
    #[must_use]
    pub fn display_badge(self) -> (&'static str, &'static str) {
        match self {
            AgentKind::Claude => ("[claude]", "claude"),
            AgentKind::Codex => ("[codex]", "codex"),
            AgentKind::Agy => ("[agy]", "agy"),
            AgentKind::OpenCode => ("[opencode]", "opencode"),
            AgentKind::Copilot => ("[copilot]", "copilot"),
            AgentKind::Devin => ("[devin]", "devin"),
            AgentKind::Shell => (">", "shell"),
        }
    }

    /// Primary brand color for this agent kind in [r, g, b] normalized float.
    #[must_use]
    pub fn brand_color(self) -> [f32; 3] {
        match self {
            AgentKind::Claude => [0.85, 0.47, 0.34], // Anthropic warm terracotta
            AgentKind::Codex => [0.06, 0.72, 0.50],  // OpenAI emerald mint
            AgentKind::Agy => [0.65, 0.33, 0.97],    // Antigravity neon purple
            AgentKind::OpenCode => [0.96, 0.62, 0.04], // OpenCode electric amber
            AgentKind::Copilot => [0.23, 0.51, 0.96], // Copilot royal blue
            AgentKind::Devin => [0.02, 0.71, 0.83],  // Devin cyan teal
            AgentKind::Shell => [0.55, 0.58, 0.68],  // Muted cool slate
        }
    }
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
    /// Current working directory of this tab (from OSC 7 or detected from prompt).
    pub cwd: Option<std::path::PathBuf>,
    /// Panes currently hosted within this tab.
    pub panes: Vec<Pane>,
    /// Active pane ID currently receiving user keyboard focus.
    pub active_pane_id: usize,
    /// Binary split tree for calculating pane bounds and dividers.
    pub pane_tree: PaneTree,
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
        let initial_pane = Pane::new(id, title.clone(), terminal.clone(), pty_writer.clone());
        let panes = vec![initial_pane];
        let pane_tree = PaneTree::new(id);
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
            cwd: None,
            panes,
            active_pane_id: id,
            pane_tree,
        }
    }

    /// Returns a reference to the currently active pane.
    #[must_use]
    pub fn active_pane(&self) -> Option<&Pane> {
        self.panes.iter().find(|p| p.id == self.active_pane_id)
    }

    /// Returns a mutable reference to the currently active pane.
    pub fn active_pane_mut(&mut self) -> Option<&mut Pane> {
        self.panes.iter_mut().find(|p| p.id == self.active_pane_id)
    }

    /// Returns a reference to a pane by its ID.
    #[must_use]
    pub fn pane(&self, pane_id: usize) -> Option<&Pane> {
        self.panes.iter().find(|p| p.id == pane_id)
    }

    /// Returns a mutable reference to a pane by its ID.
    pub fn pane_mut(&mut self, pane_id: usize) -> Option<&mut Pane> {
        self.panes.iter_mut().find(|p| p.id == pane_id)
    }

    /// Syncs state from the active pane back to tab-level fields.
    pub fn sync_from_active_pane(&mut self) {
        let active_id = self.active_pane_id;
        if let Some(pane) = self.panes.iter().find(|p| p.id == active_id) {
            let terminal = pane.terminal.clone();
            let pty_writer = pane.pty_writer.clone();
            let pty_exited = pane.pty_exited;
            let pty_exit_code = pane.pty_exit_code;
            let selection = pane.selection;
            let mouse_down_pos = pane.mouse_down_pos;
            let cwd = pane.cwd.clone();
            let title = pane.title.clone();

            self.terminal = terminal;
            self.pty_writer = pty_writer;
            self.pty_exited = pty_exited;
            self.pty_exit_code = pty_exit_code;
            self.selection = selection;
            self.mouse_down_pos = mouse_down_pos;
            self.cwd = cwd;
            self.title = title;
        }
    }

    /// Syncs state from tab-level fields into the active pane.
    pub fn sync_to_active_pane(&mut self) {
        let terminal = self.terminal.clone();
        let selection = self.selection;
        let mouse_down_pos = self.mouse_down_pos;
        let cwd = self.cwd.clone();
        let title = self.title.clone();
        let pty_exited = self.pty_exited;
        let pty_exit_code = self.pty_exit_code;
        let active_id = self.active_pane_id;
        if let Some(pane) = self.panes.iter_mut().find(|p| p.id == active_id) {
            pane.terminal = terminal;
            pane.selection = selection;
            pane.mouse_down_pos = mouse_down_pos;
            pane.cwd = cwd;
            pane.title = title;
            pane.pty_exited = pty_exited;
            pane.pty_exit_code = pty_exit_code;
        }
    }

    /// Switches focus to the pane with `pane_id`.
    pub fn select_pane(&mut self, pane_id: usize) {
        if pane_id == self.active_pane_id || !self.panes.iter().any(|p| p.id == pane_id) {
            return;
        }
        self.sync_to_active_pane();
        self.active_pane_id = pane_id;
        self.sync_from_active_pane();
    }

    /// Splits the currently active pane with `new_pane` in `direction`.
    pub fn split_active_pane(&mut self, new_pane: Pane, direction: SplitDirection) -> bool {
        let new_id = new_pane.id;
        if self.pane_tree.split(self.active_pane_id, new_id, direction) {
            self.panes.push(new_pane);
            self.select_pane(new_id);
            true
        } else {
            false
        }
    }

    /// Closes the active pane. Returns `false` if only 1 pane remained.
    pub fn close_active_pane(&mut self) -> bool {
        if self.panes.len() <= 1 {
            return false;
        }
        let closing_id = self.active_pane_id;
        let next_id = self.pane_tree.next_pane(closing_id);
        self.pane_tree.close_pane(closing_id);
        self.panes.retain(|p| p.id != closing_id);
        self.active_pane_id = next_id;
        self.sync_from_active_pane();
        true
    }

    /// Closes a pane with `pane_id`. Returns `false` if only 1 pane remained.
    pub fn close_pane(&mut self, pane_id: usize) -> bool {
        if self.panes.len() <= 1 {
            return false;
        }
        if pane_id == self.active_pane_id {
            return self.close_active_pane();
        }
        self.pane_tree.close_pane(pane_id);
        self.panes.retain(|p| p.id != pane_id);
        true
    }

    /// Cycles active pane focus forward.
    pub fn cycle_pane_forward(&mut self) {
        let next = self.pane_tree.next_pane(self.active_pane_id);
        self.select_pane(next);
    }

    /// Cycles active pane focus backward.
    pub fn cycle_pane_backward(&mut self) {
        let prev = self.pane_tree.prev_pane(self.active_pane_id);
        self.select_pane(prev);
    }

    /// Feeds incoming PTY bytes into this tab's VT parser and updates terminal state.
    pub fn advance_parser(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.terminal, bytes);
        if !self.terminal.title.is_empty() {
            self.title = self.terminal.title.clone();
        }
        if let Some(ref cwd) = self.terminal.cwd {
            self.cwd = Some(cwd.clone());
        }
        let term = self.terminal.clone();
        let title = self.title.clone();
        let cwd = self.cwd.clone();
        let active_id = self.active_pane_id;
        if let Some(pane) = self.panes.iter_mut().find(|p| p.id == active_id) {
            pane.terminal = term;
            pane.title = title;
            pane.cwd = cwd;
        }
    }

    /// Feeds incoming PTY bytes into a specific pane's VT parser.
    pub fn advance_pane_parser(&mut self, pane_id: usize, bytes: &[u8]) {
        let is_active = pane_id == self.active_pane_id;
        let mut term_update = None;
        if let Some(pane) = self.panes.iter_mut().find(|p| p.id == pane_id) {
            pane.advance_parser(bytes);
            if is_active {
                term_update = Some((pane.terminal.clone(), pane.title.clone(), pane.cwd.clone()));
            }
        }
        if let Some((term, title, cwd)) = term_update {
            self.terminal = term;
            self.title = title;
            self.cwd = cwd;
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

    /// Automatically detects if an AI agent (claude, codex, agy, opencode, copilot, devin) is active.
    #[must_use]
    pub fn detect_agent(&self) -> AgentKind {
        let cursor_line = self.sample_cursor_line();
        let sample = self.sample_terminal_text();
        AgentKind::detect(&self.title, &cursor_line, &sample)
    }

    /// Samples the immediate cursor row and previous line where user commands are executed.
    #[must_use]
    pub fn sample_cursor_line(&self) -> String {
        let mut line = String::new();
        let total_rows = self.terminal.grid.rows;
        if total_rows == 0 {
            return line;
        }
        let cur_row = self
            .terminal
            .grid
            .cursor
            .row
            .min(total_rows.saturating_sub(1));
        let start_row = cur_row.saturating_sub(1);
        for row in start_row..=cur_row {
            for col in 0..self.terminal.grid.cols {
                if let Some(cell) = self.terminal.grid.get(row, col)
                    && cell.ch != ' '
                    && cell.ch != '\0'
                {
                    line.push(cell.ch);
                }
            }
            line.push(' ');
        }
        line
    }

    /// Samples key lines of terminal text (top banner, active cursor line, prompt area) for agent inspection.
    #[must_use]
    pub fn sample_terminal_text(&self) -> String {
        let mut sample = String::new();
        let total_rows = self.terminal.grid.rows;
        if total_rows == 0 {
            return sample;
        }

        let mut rows_to_sample = std::collections::BTreeSet::new();
        // Top banner lines (0..8)
        for r in 0..total_rows.min(8) {
            rows_to_sample.insert(r);
        }
        // Active cursor line and immediate neighbors
        let cur_row = self
            .terminal
            .grid
            .cursor
            .row
            .min(total_rows.saturating_sub(1));
        for r in
            cur_row.saturating_sub(2)..=cur_row.saturating_add(1).min(total_rows.saturating_sub(1))
        {
            rows_to_sample.insert(r);
        }
        // Bottom lines (interactive footer/prompt)
        for r in total_rows.saturating_sub(3)..total_rows {
            rows_to_sample.insert(r);
        }

        for row in rows_to_sample {
            for col in 0..self.terminal.grid.cols {
                if let Some(cell) = self.terminal.grid.get(row, col)
                    && cell.ch != ' '
                    && cell.ch != '\0'
                {
                    sample.push(cell.ch);
                }
            }
            sample.push(' ');
        }
        sample
    }

    /// Attempts to detect the current working directory of this tab from OSC 7 or the prompt.
    #[must_use]
    pub fn detect_cwd(&self) -> Option<std::path::PathBuf> {
        if let Some(ref cwd) = self.cwd
            && cwd.is_dir()
        {
            return Some(cwd.clone());
        }
        self.detect_cwd_from_prompt()
    }

    /// Scans the prompt row and preceding rows to extract an existing directory path.
    #[must_use]
    pub fn detect_cwd_from_prompt(&self) -> Option<std::path::PathBuf> {
        let total_rows = self.terminal.grid.rows;
        if total_rows == 0 {
            return None;
        }
        let cur_row = self
            .terminal
            .grid
            .cursor
            .row
            .min(total_rows.saturating_sub(1));
        let start_row = cur_row.saturating_sub(3);

        for row in (start_row..=cur_row).rev() {
            let mut row_text = String::new();
            for col in 0..self.terminal.grid.cols {
                if let Some(cell) = self.terminal.grid.get(row, col) {
                    if cell.ch == '\0' {
                        row_text.push(' ');
                    } else {
                        row_text.push(cell.ch);
                    }
                }
            }
            let trimmed = row_text.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Some(path) = extract_path_from_prompt_line(trimmed) {
                return Some(path);
            }
        }

        None
    }
}

/// Extracts and verifies an existing directory path from a shell prompt line.
#[must_use]
pub fn extract_path_from_prompt_line(line: &str) -> Option<std::path::PathBuf> {
    // 1. Windows PowerShell prompt: e.g. "PS C:\path\to\dir> ..."
    if let Some(ps_idx) = line.find("PS ") {
        let after_ps = &line[ps_idx + 3..];
        if let Some(gt_idx) = after_ps.find('>') {
            let candidate = after_ps[..gt_idx].trim();
            let p = std::path::Path::new(candidate);
            if p.is_dir() {
                return Some(p.to_path_buf());
            }
        }
    }

    // 2. Windows cmd prompt or standard arrow prompt: e.g. "C:\path\to\dir>"
    if let Some(gt_idx) = line.find('>') {
        let candidate = line[..gt_idx].trim();
        let p = std::path::Path::new(candidate);
        if p.is_dir() {
            return Some(p.to_path_buf());
        }
    }

    // 3. Drive letter match on Windows: e.g. "C:\something" anywhere before space or end
    #[cfg(windows)]
    {
        for (i, b) in line.as_bytes().iter().enumerate() {
            if b.is_ascii_alphabetic()
                && i + 2 < line.len()
                && line.as_bytes()[i + 1] == b':'
                && (line.as_bytes()[i + 2] == b'\\' || line.as_bytes()[i + 2] == b'/')
            {
                let rest = &line[i..];
                let end = rest
                    .find(['>', ' ', '\t', '\r', '\n'])
                    .unwrap_or(rest.len());
                let candidate = rest[..end].trim();
                let p = std::path::Path::new(candidate);
                if p.is_dir() {
                    return Some(p.to_path_buf());
                }
            }
        }
    }

    // 4. Unix prompt: e.g. "user@host:/path/to/dir$ " or "user@host:~/dir$ "
    for delim in ['$', '#', '%'] {
        if let Some(delim_idx) = line.find(delim) {
            let before = line[..delim_idx].trim();
            let candidate = if let Some(colon_idx) = before.rfind(':') {
                before[colon_idx + 1..].trim()
            } else if let Some(space_idx) = before.rfind(' ') {
                before[space_idx + 1..].trim()
            } else {
                before
            };

            if candidate.starts_with('/') {
                let p = std::path::Path::new(candidate);
                if p.is_dir() {
                    return Some(p.to_path_buf());
                }
            } else if candidate.starts_with('~')
                && let Some(home) = dirs::home_dir()
            {
                let rel = candidate.trim_start_matches('~').trim_start_matches('/');
                let expanded = home.join(rel);
                if expanded.is_dir() {
                    return Some(expanded);
                }
            }
        }
    }

    None
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

/// Rectangle dimensions `(x, y, width, height)` for an action button in the chrome.
pub type ButtonRect = (f32, f32, f32, f32);

/// Computes layout rectangles for all tabs, plus button, and sidebar toggle button given total window width.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn compute_tab_layouts(
    count: usize,
    window_width: f32,
) -> (Vec<TabRect>, ButtonRect, ButtonRect) {
    let sidebar_btn = (6.0, 7.0, TAB_BUTTON_SIZE, TAB_BUTTON_SIZE);
    let default_plus = (32.0, 7.0, TAB_BUTTON_SIZE, TAB_BUTTON_SIZE);
    if count == 0 {
        return (Vec::new(), default_plus, sidebar_btn);
    }

    let start_x = sidebar_btn.0 + sidebar_btn.2 + 8.0;
    let available_w = (window_width - start_x - 48.0).max(TAB_MIN_WIDTH);
    let tab_w = (available_w / count as f32).clamp(TAB_MIN_WIDTH, TAB_PREFERRED_WIDTH);
    let tab_h = TAB_BAR_HEIGHT - 6.0;

    let mut rects = Vec::with_capacity(count);
    for i in 0..count {
        let x = start_x + i as f32 * (tab_w + 4.0);
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

    let last_x = rects.last().map_or(start_x, |r| r.x + r.width + 6.0);
    let plus_btn = (last_x, 7.0, TAB_BUTTON_SIZE, TAB_BUTTON_SIZE);
    (rects, plus_btn, sidebar_btn)
}

/// Rect dimensions `(split_btn, max_btn)` for right-aligned top chrome action controls.
#[must_use]
pub fn compute_chrome_buttons(window_width: f32) -> (ButtonRect, ButtonRect) {
    let split_btn = (window_width - 56.0, 7.0, TAB_BUTTON_SIZE, TAB_BUTTON_SIZE);
    let max_btn = (window_width - 28.0, 7.0, TAB_BUTTON_SIZE, TAB_BUTTON_SIZE);
    (split_btn, max_btn)
}

/// Determines which tab or action button was clicked at pixel coordinates (x, y).
#[must_use]
pub fn hit_test(x: f32, y: f32, count: usize, window_width: f32) -> TabBarAction {
    if y > TAB_BAR_HEIGHT || y < 0.0 || count == 0 {
        return TabBarAction::None;
    }

    let (layouts, plus_btn, sidebar_btn) = compute_tab_layouts(count, window_width);
    let (split_btn, max_btn) = compute_chrome_buttons(window_width);

    // Check right-aligned chrome buttons [◫] and [⤢]
    if x >= split_btn.0
        && x <= split_btn.0 + split_btn.2
        && y >= split_btn.1
        && y <= split_btn.1 + split_btn.3
    {
        return TabBarAction::ToggleSplit;
    }

    if x >= max_btn.0 && x <= max_btn.0 + max_btn.2 && y >= max_btn.1 && y <= max_btn.1 + max_btn.3
    {
        return TabBarAction::ToggleMaximize;
    }

    // Check sidebar toggle button [◨]
    if x >= sidebar_btn.0
        && x <= sidebar_btn.0 + sidebar_btn.2
        && y >= sidebar_btn.1
        && y <= sidebar_btn.1 + sidebar_btn.3
    {
        return TabBarAction::ToggleSidebar;
    }

    // Check plus button [+]
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
    let (layouts, plus_btn, sidebar_btn) = compute_tab_layouts(tab_count, window_width);
    let (split_btn, max_btn) = compute_chrome_buttons(window_width);

    // 1. Tab bar background (Deep retro obsidian #050507)
    quads.extend_from_slice(&build_quad_vertices(
        0.0,
        0.0,
        window_width,
        TAB_BAR_HEIGHT,
        [0.0, 0.0, 0.0, 1.0],
    ));

    // 2. Bottom 1px divider (#181a20)
    quads.extend_from_slice(&build_quad_vertices(
        0.0,
        TAB_BAR_HEIGHT - 1.0,
        window_width,
        1.0,
        [0.095, 0.105, 0.13, 1.0],
    ));

    // 3. Sidebar toggle button quad [◨]
    quads.extend_from_slice(&build_quad_vertices(
        sidebar_btn.0,
        sidebar_btn.1,
        sidebar_btn.2,
        sidebar_btn.3,
        [0.06, 0.065, 0.09, 0.9],
    ));

    if tab_count == 0 {
        return quads;
    }

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

    // Split pane button [◫] background quad
    quads.extend_from_slice(&build_quad_vertices(
        split_btn.0,
        split_btn.1,
        split_btn.2,
        split_btn.3,
        [0.06, 0.065, 0.09, 0.9],
    ));

    // Maximize button [⤢] background quad
    quads.extend_from_slice(&build_quad_vertices(
        max_btn.0,
        max_btn.1,
        max_btn.2,
        max_btn.3,
        [0.06, 0.065, 0.09, 0.9],
    ));

    quads
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_tab_layouts_scales_with_count() {
        let (layouts, plus_btn, sidebar_btn) = compute_tab_layouts(3, 800.0);
        assert_eq!(layouts.len(), 3);
        assert_eq!(sidebar_btn.0, 6.0);
        assert!(layouts[0].x > sidebar_btn.0);
        assert!(layouts[1].x > layouts[0].x);
        assert!(layouts[2].x > layouts[1].x);
        assert!(plus_btn.0 > layouts[2].x + layouts[2].width);
    }

    #[test]
    fn hit_test_detects_tab_selection_close_and_new_tab() {
        let (layouts, plus_btn, sidebar_btn) = compute_tab_layouts(2, 800.0);
        let (split_btn, max_btn) = compute_chrome_buttons(800.0);

        // Click sidebar toggle button
        let toggle_action = hit_test(sidebar_btn.0 + 2.0, sidebar_btn.1 + 2.0, 2, 800.0);
        assert_eq!(toggle_action, TabBarAction::ToggleSidebar);

        // Click split button
        let split_action = hit_test(split_btn.0 + 2.0, split_btn.1 + 2.0, 2, 800.0);
        assert_eq!(split_action, TabBarAction::ToggleSplit);

        // Click maximize button
        let max_action = hit_test(max_btn.0 + 2.0, max_btn.1 + 2.0, 2, 800.0);
        assert_eq!(max_action, TabBarAction::ToggleMaximize);

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
        // Base bar (1) + bottom divider (1) + sidebar btn (1) + tab 0 body (1) + tab 0 indicator (1) + tab 0 close (1)
        // + tab 1 body (1) + tab 1 close (1) + plus btn (1) + split btn (1) + max btn (1) = 11 quads = 66 vertices
        assert_eq!(quads.len(), 11 * 6);
    }

    #[test]
    fn agent_detection_recognizes_claude_codex_and_agy() {
        assert_eq!(AgentKind::detect("claude", "", ""), AgentKind::Claude);
        assert_eq!(
            AgentKind::detect("claude code v1.2", "", ""),
            AgentKind::Claude
        );
        assert_eq!(AgentKind::detect("Codex CLI", "", ""), AgentKind::Codex);
        assert_eq!(AgentKind::detect("agy-agent", "", ""), AgentKind::Agy);
        assert_eq!(
            AgentKind::detect("tab", "user@host:~$ agy start", ""),
            AgentKind::Agy
        );
        assert_eq!(
            AgentKind::detect("Tab 1", "", "Welcome to Antigravity CLI"),
            AgentKind::Agy
        );
        assert_eq!(
            AgentKind::detect("OpenCode session", "", ""),
            AgentKind::OpenCode
        );
        assert_eq!(AgentKind::detect("gh copilot", "", ""), AgentKind::Copilot);
        assert_eq!(
            AgentKind::detect("Devin workspace", "", ""),
            AgentKind::Devin
        );
        assert_eq!(
            AgentKind::detect("bash", "", "normal git log containing claude text"),
            AgentKind::Shell
        );
    }

    #[test]
    fn extract_path_from_prompt_detects_real_directories() {
        let cwd = std::env::current_dir().unwrap();
        let prompt_line = format!("PS {}> cargo check", cwd.display());
        let detected = extract_path_from_prompt_line(&prompt_line);
        assert_eq!(detected, Some(cwd));

        // Non-existent directory returns None
        let invalid = "PS C:\\non_existent_folder_xyz_123> ";
        assert!(extract_path_from_prompt_line(invalid).is_none());
    }

    #[test]
    fn test_tab_split_pane_operations() {
        let term1 = Terminal::new(80, 24);
        let mut tab = Tab::new(1, "Tab 1".to_string(), term1, None);
        assert_eq!(tab.panes.len(), 1);
        assert_eq!(tab.active_pane_id, 1);

        // Split active pane
        let term2 = Terminal::new(40, 24);
        let pane2 = Pane::new(2, "Pane 2".to_string(), term2, None);
        assert!(tab.split_active_pane(pane2, SplitDirection::Vertical));
        assert_eq!(tab.panes.len(), 2);
        assert_eq!(tab.active_pane_id, 2);

        // Cycle panes
        tab.cycle_pane_forward();
        assert_eq!(tab.active_pane_id, 1);
        tab.cycle_pane_backward();
        assert_eq!(tab.active_pane_id, 2);

        // Close pane 2
        assert!(tab.close_active_pane());
        assert_eq!(tab.panes.len(), 1);
        assert_eq!(tab.active_pane_id, 1);

        // Closing the last pane returns false
        assert!(!tab.close_active_pane());
    }
}
