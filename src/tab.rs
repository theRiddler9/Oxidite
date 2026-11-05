//! Multi-tab terminal management and tab-bar geometry.

use crate::input::{GridPoint, Selection};
use crate::pane::{Pane, PaneTree, SplitDirection};
use crate::parser::Parser;
use crate::pty::{PtySize, PtyWriter};
use crate::renderer::draw::{QuadVertex, TabLabel, build_quad_vertices};
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
    /// Toggle borderless fullscreen window mode [⛶].
    ToggleFullscreen,
    /// Toggle fullscreen / maximize window [⤢].
    ToggleMaximize,
    /// Open the appearance and settings modal [⚙].
    OpenSettings,
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
    /// User-specified custom title overriding automatic shell naming.
    pub custom_title: Option<String>,
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
            custom_title: None,
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

    /// Returns the active display title for this tab, honoring custom title if set.
    #[must_use]
    pub fn display_title(&self) -> &str {
        if let Some(ref custom) = self.custom_title {
            custom.as_str()
        } else if !self.terminal.title.is_empty() {
            self.terminal.title.as_str()
        } else {
            self.title.as_str()
        }
    }

    /// Sets or clears the user-specified custom title for this tab.
    pub fn set_custom_title(&mut self, title: Option<String>) {
        self.custom_title = title.filter(|s| !s.trim().is_empty());
        if self.custom_title.is_none() && !self.terminal.title.is_empty() {
            self.title = self.terminal.title.clone();
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
            let title = if self.custom_title.is_some() {
                self.title.clone()
            } else {
                pane.title.clone()
            };

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
        if !self.terminal.title.is_empty() && self.custom_title.is_none() {
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
                let title = if self.custom_title.is_some() {
                    self.title.clone()
                } else {
                    pane.title.clone()
                };
                term_update = Some((pane.terminal.clone(), title, pane.cwd.clone()));
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

/// Cleans raw executable paths (e.g. from PowerShell or shell startup) into concise, human-friendly titles.
#[must_use]
pub fn clean_shell_title(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "Shell".to_string();
    }
    let has_exe_ext = std::path::Path::new(trimmed)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"));
    let is_path = trimmed.contains('\\') || trimmed.starts_with('/') || has_exe_ext;
    if !is_path {
        return trimmed.to_string();
    }

    let filename = std::path::Path::new(trimmed)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(trimmed);

    let lower = filename.to_lowercase();
    if lower.starts_with("pwsh") || lower.starts_with("powershell") {
        "PowerShell".to_string()
    } else if lower.starts_with("cmd") {
        "Command Prompt".to_string()
    } else if lower.starts_with("bash") {
        "Bash".to_string()
    } else if lower.starts_with("zsh") {
        "Zsh".to_string()
    } else if lower.starts_with("fish") {
        "Fish".to_string()
    } else if lower.starts_with("cargo") {
        "Cargo".to_string()
    } else if lower.starts_with("git") {
        "Git".to_string()
    } else if lower.starts_with("wsl") {
        "WSL".to_string()
    } else if has_exe_ext {
        filename
            .strip_suffix(".exe")
            .or_else(|| filename.strip_suffix(".EXE"))
            .unwrap_or(filename)
            .to_string()
    } else {
        filename.to_string()
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

/// All upper ribbon action buttons on the right side of the window chrome.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChromeButtons {
    pub settings_btn: ButtonRect,
    pub split_btn: ButtonRect,
    pub fullscreen_btn: ButtonRect,
    pub max_btn: ButtonRect,
}

/// Computes positions for all right-aligned upper ribbon controls:
/// `[⚙ Settings]`, `[◫ Split]`, `[⛶ Fullscreen]`, `[⤢ Maximize]`.
#[must_use]
pub fn compute_chrome_buttons_extended(window_width: f32) -> ChromeButtons {
    let max_btn = (window_width - 26.0, 7.0, TAB_BUTTON_SIZE, TAB_BUTTON_SIZE);
    let fullscreen_btn = (window_width - 52.0, 7.0, TAB_BUTTON_SIZE, TAB_BUTTON_SIZE);
    let split_btn = (window_width - 78.0, 7.0, TAB_BUTTON_SIZE, TAB_BUTTON_SIZE);
    let settings_btn = (window_width - 104.0, 7.0, TAB_BUTTON_SIZE, TAB_BUTTON_SIZE);
    ChromeButtons {
        settings_btn,
        split_btn,
        fullscreen_btn,
        max_btn,
    }
}

/// Rect dimensions `(split_btn, max_btn)` for right-aligned top chrome action controls.
#[must_use]
pub fn compute_chrome_buttons(window_width: f32) -> (ButtonRect, ButtonRect) {
    let ext = compute_chrome_buttons_extended(window_width);
    (ext.split_btn, ext.max_btn)
}

/// Determines which tab or action button was clicked at pixel coordinates (x, y).
#[must_use]
pub fn hit_test(x: f32, y: f32, count: usize, window_width: f32) -> TabBarAction {
    if y > TAB_BAR_HEIGHT || y < 0.0 || count == 0 {
        return TabBarAction::None;
    }

    let (layouts, plus_btn, sidebar_btn) = compute_tab_layouts(count, window_width);
    let chrome = compute_chrome_buttons_extended(window_width);

    let in_box = |r: ButtonRect| x >= r.0 && x <= r.0 + r.2 && y >= r.1 && y <= r.1 + r.3;

    // Check right-aligned chrome buttons
    if in_box(chrome.settings_btn) {
        return TabBarAction::OpenSettings;
    }
    if in_box(chrome.split_btn) {
        return TabBarAction::ToggleSplit;
    }
    if in_box(chrome.fullscreen_btn) {
        return TabBarAction::ToggleFullscreen;
    }
    if in_box(chrome.max_btn) {
        return TabBarAction::ToggleMaximize;
    }

    // Check sidebar toggle button [◨]
    if in_box(sidebar_btn) {
        return TabBarAction::ToggleSidebar;
    }

    // Check plus button [+]
    if in_box(plus_btn) {
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
    assemble_tab_bar_quads_themed(
        tab_count,
        active_index,
        window_width,
        accent_color,
        [0.0, 0.0, 0.0, 1.0],
    )
}

/// Builds the vertices for rendering the tab bar with custom theme colors.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn assemble_tab_bar_quads_themed(
    tab_count: usize,
    active_index: usize,
    window_width: f32,
    accent_color: [f32; 4],
    bg_color: [f32; 4],
) -> Vec<QuadVertex> {
    let mut quads = Vec::new();
    let (layouts, plus_btn, sidebar_btn) = compute_tab_layouts(tab_count, window_width);
    let chrome = compute_chrome_buttons_extended(window_width);

    let is_light = (0.299 * bg_color[0] + 0.587 * bg_color[1] + 0.114 * bg_color[2]) > 0.51;

    let bar_bg = if is_light {
        [
            bg_color[0] * 0.90,
            bg_color[1] * 0.90,
            bg_color[2] * 0.90,
            1.0,
        ]
    } else {
        [
            (bg_color[0] * 0.85).max(0.0),
            (bg_color[1] * 0.85).max(0.0),
            (bg_color[2] * 0.85).max(0.0),
            1.0,
        ]
    };

    let border_color = if is_light {
        [0.72, 0.75, 0.78, 1.0]
    } else {
        [0.12, 0.14, 0.19, 1.0]
    };

    let btn_bg = if is_light {
        [
            bg_color[0] * 0.94,
            bg_color[1] * 0.94,
            bg_color[2] * 0.94,
            0.95,
        ]
    } else {
        [
            (bg_color[0] + 0.06).min(1.0),
            (bg_color[1] + 0.065).min(1.0),
            (bg_color[2] + 0.09).min(1.0),
            0.9,
        ]
    };

    // 1. Tab bar background
    quads.extend_from_slice(&build_quad_vertices(
        0.0,
        0.0,
        window_width,
        TAB_BAR_HEIGHT,
        bar_bg,
    ));

    // 2. Bottom 1px divider
    quads.extend_from_slice(&build_quad_vertices(
        0.0,
        TAB_BAR_HEIGHT - 1.0,
        window_width,
        1.0,
        border_color,
    ));

    // 3. Sidebar toggle button quad [◨]
    quads.extend_from_slice(&build_quad_vertices(
        sidebar_btn.0,
        sidebar_btn.1,
        sidebar_btn.2,
        sidebar_btn.3,
        btn_bg,
    ));
    quads.extend_from_slice(&build_quad_vertices(
        sidebar_btn.0,
        sidebar_btn.1,
        sidebar_btn.2,
        1.0,
        border_color,
    ));
    quads.extend_from_slice(&build_quad_vertices(
        sidebar_btn.0,
        sidebar_btn.1 + sidebar_btn.3 - 1.0,
        sidebar_btn.2,
        1.0,
        border_color,
    ));
    quads.extend_from_slice(&build_quad_vertices(
        sidebar_btn.0,
        sidebar_btn.1,
        1.0,
        sidebar_btn.3,
        border_color,
    ));
    quads.extend_from_slice(&build_quad_vertices(
        sidebar_btn.0 + sidebar_btn.2 - 1.0,
        sidebar_btn.1,
        1.0,
        sidebar_btn.3,
        border_color,
    ));

    for (idx, layout) in layouts.iter().enumerate() {
        let is_active = idx == active_index;
        let (tab_bg, tab_border) = if is_active {
            if is_light {
                (
                    [bg_color[0], bg_color[1], bg_color[2], 1.0],
                    [0.65, 0.68, 0.72, 1.0],
                )
            } else {
                (
                    [
                        (bg_color[0] + 0.08).min(1.0),
                        (bg_color[1] + 0.085).min(1.0),
                        (bg_color[2] + 0.11).min(1.0),
                        1.0,
                    ],
                    [0.22, 0.28, 0.40, 0.85],
                )
            }
        } else if is_light {
            (
                [
                    bg_color[0] * 0.95,
                    bg_color[1] * 0.95,
                    bg_color[2] * 0.95,
                    1.0,
                ],
                [0.78, 0.80, 0.84, 0.5],
            )
        } else {
            (
                [
                    (bg_color[0] + 0.02).min(1.0),
                    (bg_color[1] + 0.022).min(1.0),
                    (bg_color[2] + 0.035).min(1.0),
                    0.8,
                ],
                [0.14, 0.16, 0.22, 0.5],
            )
        };

        // Tab body quad
        quads.extend_from_slice(&build_quad_vertices(
            layout.x,
            layout.y,
            layout.width,
            layout.height,
            tab_bg,
        ));

        // Tab card borders (left, top, right)
        quads.extend_from_slice(&build_quad_vertices(
            layout.x,
            layout.y,
            1.0,
            layout.height,
            tab_border,
        ));
        quads.extend_from_slice(&build_quad_vertices(
            layout.x,
            layout.y,
            layout.width,
            1.0,
            tab_border,
        ));
        quads.extend_from_slice(&build_quad_vertices(
            layout.x + layout.width - 1.0,
            layout.y,
            1.0,
            layout.height,
            tab_border,
        ));

        // Active tab top illuminated accent line (modern IDE pill indicator)
        if is_active {
            quads.extend_from_slice(&build_quad_vertices(
                layout.x,
                layout.y,
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
            if is_light {
                [0.85, 0.85, 0.88, 0.6]
            } else {
                [0.16, 0.18, 0.24, 0.6]
            },
        ));
    }

    // Plus button '+' background quad with border
    quads.extend_from_slice(&build_quad_vertices(
        plus_btn.0, plus_btn.1, plus_btn.2, plus_btn.3, btn_bg,
    ));
    quads.extend_from_slice(&build_quad_vertices(
        plus_btn.0,
        plus_btn.1,
        plus_btn.2,
        1.0,
        border_color,
    ));
    quads.extend_from_slice(&build_quad_vertices(
        plus_btn.0,
        plus_btn.1 + plus_btn.3 - 1.0,
        plus_btn.2,
        1.0,
        border_color,
    ));
    quads.extend_from_slice(&build_quad_vertices(
        plus_btn.0,
        plus_btn.1,
        1.0,
        plus_btn.3,
        border_color,
    ));
    quads.extend_from_slice(&build_quad_vertices(
        plus_btn.0 + plus_btn.2 - 1.0,
        plus_btn.1,
        1.0,
        plus_btn.3,
        border_color,
    ));

    // Right-aligned upper ribbon buttons: unified segmented pill toolbar
    let toolbar_x = chrome.settings_btn.0 - 4.0;
    let toolbar_w = (chrome.max_btn.0 + chrome.max_btn.2) - toolbar_x + 4.0;
    let toolbar_y = chrome.settings_btn.1 - 2.0;
    let toolbar_h = chrome.settings_btn.3 + 4.0;
    let track_bg = if is_light {
        [0.84, 0.86, 0.90, 0.95]
    } else {
        [0.07, 0.08, 0.11, 0.95]
    };
    // Toolbar capsule track background
    quads.extend_from_slice(&build_quad_vertices(
        toolbar_x, toolbar_y, toolbar_w, toolbar_h, track_bg,
    ));
    // 1px border around toolbar capsule
    quads.extend_from_slice(&build_quad_vertices(
        toolbar_x,
        toolbar_y,
        toolbar_w,
        1.0,
        border_color,
    ));
    quads.extend_from_slice(&build_quad_vertices(
        toolbar_x,
        toolbar_y + toolbar_h - 1.0,
        toolbar_w,
        1.0,
        border_color,
    ));
    quads.extend_from_slice(&build_quad_vertices(
        toolbar_x,
        toolbar_y,
        1.0,
        toolbar_h,
        border_color,
    ));
    quads.extend_from_slice(&build_quad_vertices(
        toolbar_x + toolbar_w - 1.0,
        toolbar_y,
        1.0,
        toolbar_h,
        border_color,
    ));

    // Right-aligned upper ribbon buttons: Settings [⚙], Split [◫], Fullscreen [⛶], Maximize [⤢]
    let push_ribbon_btn = |quads: &mut Vec<QuadVertex>, rect: ButtonRect| {
        quads.extend_from_slice(&build_quad_vertices(rect.0, rect.1, rect.2, rect.3, btn_bg));
        quads.extend_from_slice(&build_quad_vertices(
            rect.0,
            rect.1,
            rect.2,
            1.0,
            border_color,
        ));
        quads.extend_from_slice(&build_quad_vertices(
            rect.0,
            rect.1 + rect.3 - 1.0,
            rect.2,
            1.0,
            border_color,
        ));
        quads.extend_from_slice(&build_quad_vertices(
            rect.0,
            rect.1,
            1.0,
            rect.3,
            border_color,
        ));
        quads.extend_from_slice(&build_quad_vertices(
            rect.0 + rect.2 - 1.0,
            rect.1,
            1.0,
            rect.3,
            border_color,
        ));
    };

    push_ribbon_btn(&mut quads, chrome.settings_btn);
    push_ribbon_btn(&mut quads, chrome.split_btn);
    push_ribbon_btn(&mut quads, chrome.fullscreen_btn);
    push_ribbon_btn(&mut quads, chrome.max_btn);

    quads
}

/// Computes the target drop slot index for a tab being dragged at `drag_x`.
#[must_use]
pub fn compute_tab_drop_index(drag_x: f32, count: usize, window_width: f32) -> usize {
    if count <= 1 {
        return 0;
    }
    let (layouts, _, _) = compute_tab_layouts(count, window_width);
    if layouts.is_empty() {
        return 0;
    }
    for (i, layout) in layouts.iter().enumerate() {
        let center = layout.x + layout.width * 0.5;
        if drag_x < center {
            return i;
        }
    }
    layouts.len().saturating_sub(1)
}

/// Reorders tabs by moving a tab from `from_idx` to `to_idx`, adjusting `active_index` accordingly.
pub fn reorder_tabs(
    tabs: &mut Vec<Tab>,
    active_index: &mut usize,
    from_idx: usize,
    to_idx: usize,
) -> bool {
    let count = tabs.len();
    if count <= 1 || from_idx >= count || to_idx >= count || from_idx == to_idx {
        return false;
    }
    let tab = tabs.remove(from_idx);
    tabs.insert(to_idx, tab);

    let cur = *active_index;
    if cur == from_idx {
        *active_index = to_idx;
    } else if from_idx < cur && to_idx >= cur {
        *active_index = cur.saturating_sub(1);
    } else if from_idx > cur && to_idx <= cur {
        *active_index = cur.saturating_add(1);
    }
    true
}

/// Tab drag state tracking the origin index and cursor coordinates during a drag-and-drop reorder.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabDragState {
    /// Original index of the tab being dragged.
    pub origin_idx: usize,
    /// Current mouse X coordinate.
    pub current_x: f32,
    /// Current mouse Y coordinate.
    pub current_y: f32,
    /// Initial mouse X coordinate when button was pressed.
    pub start_x: f32,
    /// Whether the drag has exceeded the minimum movement threshold.
    pub is_dragging: bool,
}

impl TabDragState {
    /// Creates a new `TabDragState` at `(x, y)` for `origin_idx`.
    #[must_use]
    pub fn new(origin_idx: usize, x: f32, y: f32) -> Self {
        Self {
            origin_idx,
            current_x: x,
            current_y: y,
            start_x: x,
            is_dragging: false,
        }
    }

    /// Updates mouse position and returns whether the movement exceeds the drag threshold (4px).
    pub fn update_pos(&mut self, x: f32, y: f32) -> bool {
        self.current_x = x;
        self.current_y = y;
        if (x - self.start_x).abs() >= 4.0 {
            self.is_dragging = true;
        }
        self.is_dragging
    }
}

/// Builds highlight and drop indicator vertices for a tab drag-and-drop operation.
#[must_use]
pub fn assemble_tab_drag_indicator_quads(
    tab_count: usize,
    dragged_idx: usize,
    target_idx: usize,
    window_width: f32,
    accent_color: [f32; 4],
) -> Vec<QuadVertex> {
    if tab_count == 0 || dragged_idx >= tab_count || target_idx >= tab_count {
        return Vec::new();
    }
    let (layouts, _, _) = compute_tab_layouts(tab_count, window_width);
    if layouts.is_empty() {
        return Vec::new();
    }

    let mut quads = Vec::new();

    // 1. Highlight the tab currently being dragged with a lifted background
    if let Some(dragged_layout) = layouts.get(dragged_idx) {
        quads.extend_from_slice(&build_quad_vertices(
            dragged_layout.x,
            dragged_layout.y,
            dragged_layout.width,
            dragged_layout.height,
            [0.18, 0.22, 0.32, 0.55],
        ));
    }

    // 2. Draw a vertical accent drop indicator bar where the tab will land
    if let Some(target_layout) = layouts.get(target_idx) {
        let indicator_x = if target_idx <= dragged_idx {
            target_layout.x - 2.0
        } else {
            target_layout.x + target_layout.width - 1.0
        };
        quads.extend_from_slice(&build_quad_vertices(
            indicator_x,
            target_layout.y,
            3.0,
            target_layout.height,
            accent_color,
        ));
    }

    quads
}

/// State for an interactive tab rename modal or overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabRenameState {
    /// Index of the tab currently being renamed.
    pub tab_index: usize,
    /// Editable text buffer.
    pub input_text: String,
    /// Character cursor position within `input_text`.
    pub cursor_pos: usize,
}

impl TabRenameState {
    /// Creates a new `TabRenameState` initialized with the tab's current title.
    #[must_use]
    pub fn new(tab_index: usize, current_title: &str) -> Self {
        let input_text = current_title.to_string();
        let cursor_pos = input_text.chars().count();
        Self {
            tab_index,
            input_text,
            cursor_pos,
        }
    }

    /// Inserts a character at the current cursor position.
    pub fn insert_char(&mut self, c: char) {
        let char_count = self.input_text.chars().count();
        let idx = self.cursor_pos.min(char_count);
        let byte_idx = self
            .input_text
            .char_indices()
            .nth(idx)
            .map_or(self.input_text.len(), |(b, _)| b);
        self.input_text.insert(byte_idx, c);
        self.cursor_pos += 1;
    }

    /// Deletes the character before the cursor (backspace).
    pub fn backspace(&mut self) {
        if self.cursor_pos > 0 {
            let idx = self.cursor_pos - 1;
            if let Some((b, ch)) = self.input_text.char_indices().nth(idx) {
                self.input_text.drain(b..b + ch.len_utf8());
                self.cursor_pos -= 1;
            }
        }
    }

    /// Deletes the character at the cursor position (delete).
    pub fn delete(&mut self) {
        let char_count = self.input_text.chars().count();
        if self.cursor_pos < char_count
            && let Some((b, ch)) = self.input_text.char_indices().nth(self.cursor_pos)
        {
            self.input_text.drain(b..b + ch.len_utf8());
        }
    }

    /// Moves cursor left by one character.
    pub fn move_cursor_left(&mut self) {
        self.cursor_pos = self.cursor_pos.saturating_sub(1);
    }

    /// Moves cursor right by one character.
    pub fn move_cursor_right(&mut self) {
        let char_count = self.input_text.chars().count();
        if self.cursor_pos < char_count {
            self.cursor_pos += 1;
        }
    }

    /// Moves cursor to beginning of line.
    pub fn move_cursor_home(&mut self) {
        self.cursor_pos = 0;
    }

    /// Moves cursor to end of line.
    pub fn move_cursor_end(&mut self) {
        self.cursor_pos = self.input_text.chars().count();
    }
}

/// Action triggered by interacting with the tab rename modal dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabRenameAction {
    /// Save the new custom title.
    Save,
    /// Cancel renaming without changes.
    Cancel,
    /// Click fell inside the editable text box.
    InputBox,
    /// Click fell outside the dialog card.
    Backdrop,
}

/// Builds quad vertices for the tab rename modal dialog.
#[must_use]
pub fn assemble_tab_rename_quads(
    window_width: f32,
    window_height: f32,
    accent_color: [f32; 4],
) -> Vec<QuadVertex> {
    let mut quads = Vec::new();
    let card_w = 400.0_f32;
    let card_h = 140.0_f32;
    let card_x = ((window_width - card_w) / 2.0).max(10.0);
    let card_y = ((window_height - card_h) / 2.0).max(10.0);

    // 1. Fullscreen dimmed backdrop
    quads.extend_from_slice(&build_quad_vertices(
        0.0,
        0.0,
        window_width,
        window_height,
        [0.0, 0.0, 0.0, 0.65],
    ));

    // 2. Card border (accent)
    quads.extend_from_slice(&build_quad_vertices(
        card_x - 1.0,
        card_y - 1.0,
        card_w + 2.0,
        card_h + 2.0,
        accent_color,
    ));

    // 3. Card background (#0b0c10)
    quads.extend_from_slice(&build_quad_vertices(
        card_x,
        card_y,
        card_w,
        card_h,
        [0.045, 0.048, 0.065, 0.98],
    ));

    // 4. Text input box border
    quads.extend_from_slice(&build_quad_vertices(
        card_x + 19.0,
        card_y + 44.0,
        card_w - 38.0,
        34.0,
        [0.22, 0.25, 0.35, 0.8],
    ));

    // 5. Text input box background (#040507)
    quads.extend_from_slice(&build_quad_vertices(
        card_x + 20.0,
        card_y + 45.0,
        card_w - 40.0,
        32.0,
        [0.02, 0.025, 0.035, 1.0],
    ));

    // 6. Save button background
    quads.extend_from_slice(&build_quad_vertices(
        card_x + card_w - 190.0,
        card_y + 92.0,
        80.0,
        28.0,
        [0.18, 0.42, 0.78, 0.9],
    ));

    // 7. Cancel button background
    quads.extend_from_slice(&build_quad_vertices(
        card_x + card_w - 95.0,
        card_y + 92.0,
        75.0,
        28.0,
        [0.14, 0.15, 0.20, 0.8],
    ));

    quads
}

/// Hit tests click coordinates within the tab rename dialog.
#[must_use]
pub fn hit_test_tab_rename(
    x: f32,
    y: f32,
    window_width: f32,
    window_height: f32,
) -> Option<TabRenameAction> {
    let card_w = 400.0_f32;
    let card_h = 140.0_f32;
    let card_x = ((window_width - card_w) / 2.0).max(10.0);
    let card_y = ((window_height - card_h) / 2.0).max(10.0);

    if x < card_x || x > card_x + card_w || y < card_y || y > card_y + card_h {
        return Some(TabRenameAction::Backdrop);
    }

    // Save button: card_x + card_w - 190.0, card_y + 92.0, 80.0, 28.0
    let save_x = card_x + card_w - 190.0;
    let save_y = card_y + 92.0;
    if x >= save_x && x <= save_x + 80.0 && y >= save_y && y <= save_y + 28.0 {
        return Some(TabRenameAction::Save);
    }

    // Cancel button: card_x + card_w - 95.0, card_y + 92.0, 75.0, 28.0
    let cancel_x = card_x + card_w - 95.0;
    let cancel_y = card_y + 92.0;
    if x >= cancel_x && x <= cancel_x + 75.0 && y >= cancel_y && y <= cancel_y + 28.0 {
        return Some(TabRenameAction::Cancel);
    }

    // Input box: card_x + 20.0, card_y + 45.0, card_w - 40.0, 32.0
    if x >= card_x + 20.0 && x <= card_x + card_w - 20.0 && y >= card_y + 45.0 && y <= card_y + 77.0
    {
        return Some(TabRenameAction::InputBox);
    }

    None
}

/// Builds text labels for rendering the tab rename modal.
#[must_use]
pub fn build_tab_rename_labels(
    state: &TabRenameState,
    window_width: f32,
    window_height: f32,
    show_cursor: bool,
) -> Vec<TabLabel> {
    let card_w = 400.0_f32;
    let card_h = 140.0_f32;
    let card_x = ((window_width - card_w) / 2.0).max(10.0);
    let card_y = ((window_height - card_h) / 2.0).max(10.0);

    let mut labels = Vec::new();

    // Dialog title
    labels.push(TabLabel {
        text: "Rename Tab".to_string(),
        x: card_x + 20.0,
        y: card_y + 14.0,
        color: glyphon::Color::rgb(240, 242, 250),
        bold: true,
        max_width: card_w - 40.0,
    });

    // Display input text with blinking cursor
    let input_display = if show_cursor {
        let char_count = state.input_text.chars().count();
        let idx = state.cursor_pos.min(char_count);
        let byte_idx = state
            .input_text
            .char_indices()
            .nth(idx)
            .map_or(state.input_text.len(), |(b, _)| b);
        let mut s = state.input_text[..byte_idx].to_string();
        s.push('|');
        s.push_str(&state.input_text[byte_idx..]);
        s
    } else {
        state.input_text.clone()
    };

    labels.push(TabLabel {
        text: input_display,
        x: card_x + 28.0,
        y: card_y + 51.0,
        color: glyphon::Color::rgb(220, 225, 240),
        bold: false,
        max_width: card_w - 56.0,
    });

    // Save button label
    labels.push(TabLabel {
        text: "Save".to_string(),
        x: card_x + card_w - 168.0,
        y: card_y + 97.0,
        color: glyphon::Color::rgb(255, 255, 255),
        bold: true,
        max_width: 60.0,
    });

    // Cancel button label
    labels.push(TabLabel {
        text: "Cancel".to_string(),
        x: card_x + card_w - 82.0,
        y: card_y + 97.0,
        color: glyphon::Color::rgb(180, 185, 200),
        bold: false,
        max_width: 60.0,
    });

    labels
}

/// State for a tab close confirmation modal dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseConfirmState {
    /// Index of the tab requested to be closed.
    pub tab_index: usize,
    /// Display title of the tab.
    pub title: String,
    /// Whether the user has selected the Confirm [Close Tab] button vs [Cancel].
    pub confirm_selected: bool,
}

impl CloseConfirmState {
    /// Creates a new `CloseConfirmState` for `tab_index` with default button selection on [Close Tab].
    #[must_use]
    pub fn new(tab_index: usize, title: String) -> Self {
        Self {
            tab_index,
            title,
            confirm_selected: true,
        }
    }

    /// Toggles the selected button between [Close Tab] and [Cancel].
    pub fn toggle_selection(&mut self) {
        self.confirm_selected = !self.confirm_selected;
    }
}

/// Action triggered by interacting with the close tab confirmation dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseConfirmAction {
    /// Confirm closing the tab.
    Confirm,
    /// Cancel and keep the tab open.
    Cancel,
    /// Click fell outside the dialog card.
    Backdrop,
}

/// Builds quad vertices for the close tab confirmation modal dialog.
#[must_use]
pub fn assemble_close_confirm_quads(
    window_width: f32,
    window_height: f32,
    confirm_selected: bool,
    _accent_color: [f32; 4],
) -> Vec<QuadVertex> {
    let mut quads = Vec::new();
    let card_w = 440.0_f32;
    let card_h = 160.0_f32;
    let card_x = ((window_width - card_w) / 2.0).max(10.0);
    let card_y = ((window_height - card_h) / 2.0).max(10.0);

    // 1. Fullscreen dimmed backdrop
    quads.extend_from_slice(&build_quad_vertices(
        0.0,
        0.0,
        window_width,
        window_height,
        [0.0, 0.0, 0.0, 0.65],
    ));

    // 2. Card border
    quads.extend_from_slice(&build_quad_vertices(
        card_x - 1.0,
        card_y - 1.0,
        card_w + 2.0,
        card_h + 2.0,
        [0.25, 0.28, 0.38, 0.9],
    ));

    // 3. Card body (#0b0c10)
    quads.extend_from_slice(&build_quad_vertices(
        card_x,
        card_y,
        card_w,
        card_h,
        [0.045, 0.048, 0.065, 0.98],
    ));

    // 4. Header warning accent line (coral red)
    quads.extend_from_slice(&build_quad_vertices(
        card_x,
        card_y,
        card_w,
        3.0,
        [0.92, 0.35, 0.35, 1.0],
    ));

    // 5. Close Tab button: card_x + card_w - 235.0, card_y + 110.0, 115.0, 32.0
    let close_bg = if confirm_selected {
        [0.85, 0.22, 0.25, 0.95] // Highlighted vibrant red
    } else {
        [0.18, 0.19, 0.26, 0.8]
    };
    quads.extend_from_slice(&build_quad_vertices(
        card_x + card_w - 235.0,
        card_y + 110.0,
        115.0,
        32.0,
        close_bg,
    ));

    // 6. Cancel button: card_x + card_w - 105.0, card_y + 110.0, 85.0, 32.0
    let cancel_bg = if confirm_selected {
        [0.14, 0.15, 0.20, 0.8]
    } else {
        [0.28, 0.32, 0.44, 0.95] // Highlighted gray
    };
    quads.extend_from_slice(&build_quad_vertices(
        card_x + card_w - 105.0,
        card_y + 110.0,
        85.0,
        32.0,
        cancel_bg,
    ));

    quads
}

/// Hit tests click coordinates within the close tab confirmation dialog.
#[must_use]
pub fn hit_test_close_confirm(
    x: f32,
    y: f32,
    window_width: f32,
    window_height: f32,
) -> Option<CloseConfirmAction> {
    let card_w = 440.0_f32;
    let card_h = 160.0_f32;
    let card_x = ((window_width - card_w) / 2.0).max(10.0);
    let card_y = ((window_height - card_h) / 2.0).max(10.0);

    if x < card_x || x > card_x + card_w || y < card_y || y > card_y + card_h {
        return Some(CloseConfirmAction::Backdrop);
    }

    // Close button: card_x + card_w - 235.0, card_y + 110.0, 115.0, 32.0
    let close_x = card_x + card_w - 235.0;
    let close_y = card_y + 110.0;
    if x >= close_x && x <= close_x + 115.0 && y >= close_y && y <= close_y + 32.0 {
        return Some(CloseConfirmAction::Confirm);
    }

    // Cancel button: card_x + card_w - 105.0, card_y + 110.0, 85.0, 32.0
    let cancel_x = card_x + card_w - 105.0;
    let cancel_y = card_y + 110.0;
    if x >= cancel_x && x <= cancel_x + 85.0 && y >= cancel_y && y <= cancel_y + 32.0 {
        return Some(CloseConfirmAction::Cancel);
    }

    None
}

/// Builds text labels for rendering the close tab confirmation modal.
#[must_use]
pub fn build_close_confirm_labels(
    state: &CloseConfirmState,
    window_width: f32,
    window_height: f32,
) -> Vec<TabLabel> {
    let card_w = 440.0_f32;
    let card_h = 160.0_f32;
    let card_x = ((window_width - card_w) / 2.0).max(10.0);
    let card_y = ((window_height - card_h) / 2.0).max(10.0);

    let mut labels = Vec::new();

    // Dialog title
    labels.push(TabLabel {
        text: "Close Active Tab?".to_string(),
        x: card_x + 20.0,
        y: card_y + 16.0,
        color: glyphon::Color::rgb(250, 100, 100),
        bold: true,
        max_width: card_w - 40.0,
    });

    // Subtitle warning
    let subtitle = format!("A terminal process is still running in '{}'.", state.title);
    labels.push(TabLabel {
        text: subtitle,
        x: card_x + 20.0,
        y: card_y + 46.0,
        color: glyphon::Color::rgb(210, 215, 230),
        bold: false,
        max_width: card_w - 40.0,
    });

    labels.push(TabLabel {
        text: "Closing this tab will terminate all child processes.".to_string(),
        x: card_x + 20.0,
        y: card_y + 68.0,
        color: glyphon::Color::rgb(140, 145, 160),
        bold: false,
        max_width: card_w - 40.0,
    });

    // Close button label
    labels.push(TabLabel {
        text: "Close Tab".to_string(),
        x: card_x + card_w - 212.0,
        y: card_y + 117.0,
        color: glyphon::Color::rgb(255, 255, 255),
        bold: true,
        max_width: 90.0,
    });

    // Cancel button label
    labels.push(TabLabel {
        text: "Cancel".to_string(),
        x: card_x + card_w - 88.0,
        y: card_y + 117.0,
        color: glyphon::Color::rgb(220, 225, 240),
        bold: true,
        max_width: 70.0,
    });

    labels
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
        let chrome = compute_chrome_buttons_extended(800.0);

        // Click sidebar toggle button
        let toggle_action = hit_test(sidebar_btn.0 + 2.0, sidebar_btn.1 + 2.0, 2, 800.0);
        assert_eq!(toggle_action, TabBarAction::ToggleSidebar);

        // Click settings button
        let settings_action = hit_test(
            chrome.settings_btn.0 + 2.0,
            chrome.settings_btn.1 + 2.0,
            2,
            800.0,
        );
        assert_eq!(settings_action, TabBarAction::OpenSettings);

        // Click split button
        let split_action = hit_test(chrome.split_btn.0 + 2.0, chrome.split_btn.1 + 2.0, 2, 800.0);
        assert_eq!(split_action, TabBarAction::ToggleSplit);

        // Click fullscreen button
        let fs_action = hit_test(
            chrome.fullscreen_btn.0 + 2.0,
            chrome.fullscreen_btn.1 + 2.0,
            2,
            800.0,
        );
        assert_eq!(fs_action, TabBarAction::ToggleFullscreen);

        // Click maximize button
        let max_action = hit_test(chrome.max_btn.0 + 2.0, chrome.max_btn.1 + 2.0, 2, 800.0);
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
        assert!(!quads.is_empty());
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

    #[test]
    fn test_custom_title_and_display() {
        let term = Terminal::new(80, 24);
        let mut tab = Tab::new(1, "Shell".to_string(), term, None);
        assert_eq!(tab.display_title(), "Shell");

        // Set custom title
        tab.set_custom_title(Some("My Project".to_string()));
        assert_eq!(tab.display_title(), "My Project");

        // Dynamic OSC title does not overwrite custom title
        tab.advance_parser(b"\x1b]0;Dynamic Shell\x07");
        assert_eq!(tab.display_title(), "My Project");

        // Clearing custom title reverts to automatic title
        tab.set_custom_title(None);
        assert_eq!(tab.display_title(), "Dynamic Shell");
    }

    #[test]
    fn test_tab_drag_and_reorder() {
        let term1 = Terminal::new(80, 24);
        let term2 = Terminal::new(80, 24);
        let term3 = Terminal::new(80, 24);
        let mut tabs = vec![
            Tab::new(1, "Tab 1".to_string(), term1, None),
            Tab::new(2, "Tab 2".to_string(), term2, None),
            Tab::new(3, "Tab 3".to_string(), term3, None),
        ];
        let mut active_idx = 0;

        // Reorder tab 0 to slot 2: [Tab 1, Tab 2, Tab 3] -> [Tab 2, Tab 3, Tab 1]
        assert!(reorder_tabs(&mut tabs, &mut active_idx, 0, 2));
        assert_eq!(tabs[0].id, 2);
        assert_eq!(tabs[1].id, 3);
        assert_eq!(tabs[2].id, 1);
        assert_eq!(active_idx, 2); // Tab 1 remains active at its new index

        // Reorder tab 2 back to 0: [Tab 2, Tab 3, Tab 1] -> [Tab 1, Tab 2, Tab 3]
        assert!(reorder_tabs(&mut tabs, &mut active_idx, 2, 0));
        assert_eq!(tabs[0].id, 1);
        assert_eq!(tabs[1].id, 2);
        assert_eq!(tabs[2].id, 3);
        assert_eq!(active_idx, 0);

        // Test drop target calculation
        let target = compute_tab_drop_index(50.0, 3, 800.0);
        assert_eq!(target, 0);

        // Test drag indicator quad generation
        let indicator_quads =
            assemble_tab_drag_indicator_quads(3, 0, 1, 800.0, [0.2, 0.6, 1.0, 1.0]);
        assert!(!indicator_quads.is_empty());
    }

    #[test]
    fn test_tab_rename_state_and_dialog() {
        let mut rename = TabRenameState::new(0, "Tab 1");
        assert_eq!(rename.input_text, "Tab 1");
        assert_eq!(rename.cursor_pos, 5);

        // Backspace
        rename.backspace();
        assert_eq!(rename.input_text, "Tab ");
        assert_eq!(rename.cursor_pos, 4);

        // Typing
        for ch in "Renamed".chars() {
            rename.insert_char(ch);
        }
        assert_eq!(rename.input_text, "Tab Renamed");

        // Cursor movement and deletion
        rename.move_cursor_home();
        assert_eq!(rename.cursor_pos, 0);
        rename.delete();
        assert_eq!(rename.input_text, "ab Renamed");

        // Hit testing
        let hit_backdrop = hit_test_tab_rename(5.0, 5.0, 800.0, 600.0);
        assert_eq!(hit_backdrop, Some(TabRenameAction::Backdrop));

        let card_x = (800.0 - 400.0) / 2.0;
        let card_y = (600.0 - 140.0) / 2.0;
        let hit_save = hit_test_tab_rename(card_x + 400.0 - 150.0, card_y + 100.0, 800.0, 600.0);
        assert_eq!(hit_save, Some(TabRenameAction::Save));

        let hit_cancel = hit_test_tab_rename(card_x + 400.0 - 50.0, card_y + 100.0, 800.0, 600.0);
        assert_eq!(hit_cancel, Some(TabRenameAction::Cancel));

        // Quads and labels
        let quads = assemble_tab_rename_quads(800.0, 600.0, [0.2, 0.6, 1.0, 1.0]);
        assert!(!quads.is_empty());
        let labels = build_tab_rename_labels(&rename, 800.0, 600.0, true);
        assert_eq!(labels.len(), 4);
    }

    #[test]
    fn test_close_confirm_state_and_dialog() {
        let mut confirm = CloseConfirmState::new(0, "Active Shell".to_string());
        assert!(confirm.confirm_selected);

        // Toggle selection
        confirm.toggle_selection();
        assert!(!confirm.confirm_selected);

        // Hit testing
        let card_x = (800.0 - 440.0) / 2.0;
        let card_y = (600.0 - 160.0) / 2.0;
        let hit_confirm =
            hit_test_close_confirm(card_x + 440.0 - 180.0, card_y + 125.0, 800.0, 600.0);
        assert_eq!(hit_confirm, Some(CloseConfirmAction::Confirm));

        let hit_cancel =
            hit_test_close_confirm(card_x + 440.0 - 50.0, card_y + 125.0, 800.0, 600.0);
        assert_eq!(hit_cancel, Some(CloseConfirmAction::Cancel));

        // Quads and labels
        let quads = assemble_close_confirm_quads(800.0, 600.0, true, [0.9, 0.3, 0.3, 1.0]);
        assert!(!quads.is_empty());
        let labels = build_close_confirm_labels(&confirm, 800.0, 600.0);
        assert_eq!(labels.len(), 5);
    }

    #[test]
    fn test_clean_shell_title_cleans_executable_paths() {
        assert_eq!(
            clean_shell_title(
                r"C:\Program Files\WindowsApps\Microsoft.PowerShell_7.6.9.0_x64_8wekyb3d8bbwe\pwsh.exe"
            ),
            "PowerShell"
        );
        assert_eq!(
            clean_shell_title(r"C:\Windows\System32\cmd.exe"),
            "Command Prompt"
        );
        assert_eq!(clean_shell_title("/bin/zsh"), "Zsh");
        assert_eq!(clean_shell_title("/usr/bin/bash"), "Bash");
        assert_eq!(clean_shell_title("Tab 1"), "Tab 1");
        assert_eq!(clean_shell_title("Workspaces"), "Workspaces");
    }
}
