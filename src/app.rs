//! Application event loop wiring and state management.

use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use crate::input::{
    GridPoint, InputAction, KeyEvent, Selection, check_input_action, encode_key, encode_paste,
};
use crate::parser::Parser;
use crate::pty::{PtyReader, PtyWriter};
use crate::renderer::draw::{
    DrawContext, GlyphPipeline, QuadPipeline, TabLabel,
    build_cell_background_vertices_with_offsets, build_cursor_vertices_with_offsets,
    build_quad_vertices, build_selection_vertices_with_offsets,
    build_text_decoration_vertices_with_offsets,
};
use crate::renderer::font::CellMetrics;
use crate::renderer::gpu::GpuContext;
use crate::renderer::window::{
    DEFAULT_TITLE, create_window, window_size_to_grid_dimensions, window_size_to_pty_size,
};
use crate::tab::{
    TAB_BAR_HEIGHT, Tab, TabBarAction, assemble_tab_bar_quads, compute_tab_layouts, hit_test,
};
use crate::terminal::Terminal;
use winit::dpi::PhysicalSize;
use winit::keyboard::ModifiersState;

/// Cross-thread events delivered via `winit`'s `EventLoopProxy`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    /// Raw output bytes received from the PTY reader thread for tab 0 / active tab.
    PtyOutput(Vec<u8>),
    /// Child process exited with an optional exit code.
    PtyExited(Option<i32>),
    /// Raw output bytes received from a specific tab's PTY reader thread.
    TabPtyOutput {
        /// Tab ID receiving this output.
        tab_id: usize,
        /// Raw output bytes.
        bytes: Vec<u8>,
    },
    /// Tab child process exited with an optional exit code.
    TabPtyExited {
        /// Tab ID that terminated.
        tab_id: usize,
        /// Child exit status code if known.
        code: Option<i32>,
    },
    /// Configuration file was modified on disk and reloaded.
    ConfigReloaded,
    /// Active theme was switched to the specified theme name.
    ThemeChanged(String),
}

/// Maximum byte batch size forwarded in a single `AppEvent::PtyOutput` to optimize large-output throughput.
pub const PTY_BATCH_BYTE_LIMIT: usize = 65_536;

/// Spawns a background thread that forwards PTY reader output chunks for tab 0 to the event loop.
pub fn spawn_pty_forwarder(
    reader: PtyReader,
    writer: Arc<Mutex<PtyWriter>>,
    proxy: EventLoopProxy<AppEvent>,
) -> JoinHandle<()> {
    spawn_tab_pty_forwarder(0, reader, writer, proxy)
}

/// Spawns a background thread that forwards PTY reader output chunks for the specified tab to the event loop.
pub fn spawn_tab_pty_forwarder(
    tab_id: usize,
    mut reader: PtyReader,
    writer: Arc<Mutex<PtyWriter>>,
    proxy: EventLoopProxy<AppEvent>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name(format!("pty-forwarder-tab-{tab_id}"))
        .spawn(move || {
            while let Some(mut bytes) = reader.recv() {
                // Coalesce immediately available pending chunks to maximize throughput on bursts (e.g. cat/seq)
                while bytes.len() < PTY_BATCH_BYTE_LIMIT
                    && let Some(more) = reader.try_recv()
                {
                    bytes.extend_from_slice(&more);
                }

                if proxy
                    .send_event(AppEvent::TabPtyOutput { tab_id, bytes })
                    .is_err()
                {
                    return;
                }
            }

            // Reader channel closed; poll try_wait() under a short-lived lock
            // without holding the mutex across sleeps to keep handle_resize responsive.
            let mut exit_code = None;
            loop {
                let status = if let Ok(mut guard) = writer.lock() {
                    guard.try_wait().ok()
                } else {
                    break;
                };

                match status {
                    Some(Some(code)) => {
                        exit_code = i32::try_from(code).ok();
                        break;
                    }
                    Some(None) => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    None => break,
                }
            }

            let _ = proxy.send_event(AppEvent::TabPtyExited {
                tab_id,
                code: exit_code,
            });
        })
        .expect("failed to spawn PTY event forwarder thread")
}

/// Core application state bound to the `winit` event loop.
#[allow(clippy::struct_excessive_bools)]
pub struct App {
    /// Optional event loop proxy for sending events from background threads.
    pub proxy: Option<EventLoopProxy<AppEvent>>,
    /// Managed window instance once resumed.
    pub window: Option<Arc<Window>>,
    /// Current window title.
    pub title: String,
    /// Whether the PTY child process has exited.
    pub pty_exited: bool,
    /// Exit code of the child process if available.
    pub pty_exit_code: Option<i32>,
    /// Terminal state containing grid, modes, cursor, and damage tracking.
    pub terminal: Terminal,
    /// Collection of open tabs.
    pub tabs: Vec<Tab>,
    /// Index of currently active tab.
    pub active_tab_index: usize,
    /// Next tab ID to assign.
    pub next_tab_id: usize,
    /// Base configured font size in points/pixels before DPI scaling or zoom.
    pub base_font_size: f32,
    /// Current display `HiDPI` scale factor.
    pub scale_factor: f64,
    /// Measured cell dimensions for font layout and sizing calculations.
    pub cell_metrics: CellMetrics,
    /// PTY master writer for resizing and keyboard/mouse input forwarding.
    pub pty_writer: Option<Arc<Mutex<PtyWriter>>>,
    /// GPU rendering context (surface, device, queue) once window is initialized.
    pub gpu: Option<GpuContext>,
    /// Quad pipeline for rendering background colors and cursor.
    pub quad_pipeline: Option<QuadPipeline>,
    /// Glyph pipeline for rendering text glyphs.
    pub glyph_pipeline: Option<GlyphPipeline>,
    /// VT parser that translates escape sequence streams into Terminal calls.
    pub parser: Parser,
    /// Currently pressed keyboard modifier state (Shift, Ctrl, Alt, Super).
    pub modifiers: ModifiersState,
    /// Active text selection range if user is selecting text with the mouse.
    pub selection: Option<Selection>,
    /// Mouse drag start coordinate when selecting text.
    pub mouse_down_pos: Option<GridPoint>,
    /// Last recorded cursor position in window pixel coordinates.
    pub cursor_pixel_pos: (f64, f64),
    /// System clipboard interface for copying and pasting text.
    pub clipboard: Option<arboard::Clipboard>,
    /// Timestamp of last cursor blink phase toggle.
    pub last_blink: std::time::Instant,
    /// Whether the cursor is currently in its visible blink phase.
    pub cursor_blink_visible: bool,
    /// Whether the window currently has keyboard focus.
    pub has_focus: bool,
    /// Last seen bell count from the terminal.
    pub last_bell_count: u64,
    /// Timestamp when visual bell was triggered, if currently active.
    pub bell_triggered_at: Option<std::time::Instant>,
    /// Active application configuration.
    pub config: crate::config::Config,
    /// Background file watcher keeping track of config.toml modifications.
    pub config_watcher: Option<notify::RecommendedWatcher>,
    /// Whether the left collapsible workspace and sessions sidebar is expanded.
    pub sidebar_open: bool,
    /// Cached workspace name (folder name of current directory).
    pub workspace_name: String,
    /// Detected git branch if running inside a git repository.
    pub git_branch: Option<String>,
    /// Sidebar persistent multi-workspace and chat state.
    pub sidebar_state: crate::sidebar::SidebarState,
    /// Whether the right changes / git review panel is open.
    pub changes_panel_open: bool,
    /// State representation for the right git panel.
    pub changes_panel_state: crate::changes_panel::ChangesPanelState,
    /// Whether the central side-by-side diff review workspace is open.
    pub diff_view_open: bool,
    /// State representation for the side-by-side diff view.
    pub diff_view_state: crate::diff_view::DiffViewState,
    /// Whether the theme dropdown menu in the status bar ribbon is open.
    pub theme_menu_open: bool,
}

impl App {
    /// Default base font size in points/pixels.
    pub const DEFAULT_FONT_SIZE: f32 = 14.0;
    /// Minimum allowed base font size.
    pub const MIN_FONT_SIZE: f32 = 6.0;
    /// Maximum allowed base font size.
    pub const MAX_FONT_SIZE: f32 = 48.0;
    /// Base font size change per zoom step.
    pub const FONT_SIZE_STEP: f32 = 1.0;
    /// Duration of the visual bell flash.
    pub const VISUAL_BELL_DURATION: Duration = Duration::from_millis(100);

    /// Creates a new `App` instance with the given event loop proxy, terminal, and optional PTY writer.
    #[allow(clippy::too_many_lines)]
    pub fn new(
        proxy: Option<EventLoopProxy<AppEvent>>,
        terminal: Terminal,
        pty_writer: Option<Arc<Mutex<PtyWriter>>>,
    ) -> Self {
        let base_font_size = Self::DEFAULT_FONT_SIZE;
        let scale_factor = 1.0;
        let mut font_system = glyphon::FontSystem::new();
        let cell_metrics = CellMetrics::measure(&mut font_system, base_font_size, 1.2);
        let last_bell_count = terminal.bell_count;

        let mut config_watcher = None;
        if let (Some(prx), Some(cfg_path)) = (&proxy, crate::config::config_path()) {
            let prx_clone = prx.clone();
            config_watcher = crate::config::watch_config(&cfg_path, move || {
                let _ = prx_clone.send_event(AppEvent::ConfigReloaded);
            })
            .ok();
        }

        let initial_tab = Tab::new(0, "Tab 1".to_string(), terminal.clone(), pty_writer.clone());
        let tabs = vec![initial_tab];
        let active_tab_index = 0;
        let next_tab_id = 1;

        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let proj_info = crate::git::detect_project_type(&cwd);
        let workspace_name = proj_info.name.clone();
        let git_branch = crate::git::detect_git_branch(&cwd);
        let changed_files = crate::git::query_git_status(&cwd);
        let total_adds: usize = changed_files.iter().map(|f| f.additions).sum();
        let total_dels: usize = changed_files.iter().map(|f| f.deletions).sum();

        let sidebar_open = true;
        let sidebar_state = crate::sidebar::SidebarState {
            workspace_name: workspace_name.clone(),
            git_branch: git_branch.clone(),
            git_status: Some(if changed_files.is_empty() {
                "clean".to_string()
            } else {
                "modified".to_string()
            }),
            tab_titles: vec!["Tab 1".to_string()],
            active_tab_index: 0,
            workspaces: vec![crate::sidebar::WorkspaceEntry {
                name: format!("{} ({})", proj_info.name, proj_info.kind),
                branch: git_branch.clone().unwrap_or_else(|| "main".to_string()),
                sessions_count: 1,
                modified_count: changed_files.len(),
                diff_stat: Some((total_adds, total_dels)),
                is_active: true,
            }],
            chats: vec![
                crate::sidebar::ChatEntry {
                    title: "Terminal Session 1".to_string(),
                    status: crate::sidebar::ChatStatus::Idle,
                    model: "Local Shell".to_string(),
                },
                crate::sidebar::ChatEntry {
                    title: "AI Review Engine".to_string(),
                    status: crate::sidebar::ChatStatus::Done,
                    model: "Antigravity".to_string(),
                },
            ],
        };

        let selected_file_index = if changed_files.is_empty() {
            None
        } else {
            Some(0)
        };
        let changes_panel_state = crate::changes_panel::ChangesPanelState {
            active_tab: crate::changes_panel::ChangesTab::Changes,
            changed_files: changed_files.clone(),
            selected_file_index,
            commit_message: "Commit Tracked".to_string(),
            git_branch: git_branch.clone().unwrap_or_else(|| "main".to_string()),
        };

        let diff_view_state = if let Some(first_file) = changed_files.first() {
            let rel_path = if first_file.path_info == "." {
                first_file.filename.clone()
            } else {
                format!(
                    "{}/{}",
                    first_file.path_info.trim_end_matches('/'),
                    first_file.filename
                )
            };
            let (fname, pinfo, hunks) = crate::git::query_file_diff(&cwd, &rel_path);
            crate::diff_view::DiffViewState {
                file_name: fname,
                path_info: pinfo,
                hunks,
                scroll_offset: 0,
                staged: first_file.staged,
            }
        } else {
            crate::diff_view::DiffViewState::default()
        };

        Self {
            proxy,
            window: None,
            title: DEFAULT_TITLE.to_string(),
            pty_exited: false,
            pty_exit_code: None,
            terminal,
            tabs,
            active_tab_index,
            next_tab_id,
            base_font_size,
            scale_factor,
            cell_metrics,
            pty_writer,
            gpu: None,
            quad_pipeline: None,
            glyph_pipeline: None,
            parser: Parser::new(),
            modifiers: ModifiersState::empty(),
            selection: None,
            mouse_down_pos: None,
            cursor_pixel_pos: (0.0, 0.0),
            clipboard: arboard::Clipboard::new().ok(),
            last_blink: std::time::Instant::now(),
            cursor_blink_visible: true,
            has_focus: true,
            last_bell_count,
            bell_triggered_at: None,
            config: crate::config::Config::default(),
            config_watcher,
            sidebar_open,
            workspace_name,
            git_branch,
            sidebar_state,
            changes_panel_open: true,
            changes_panel_state,
            diff_view_open: true,
            diff_view_state,
            theme_menu_open: false,
        }
    }

    /// Converts window pixel coordinates to a clamped terminal grid point, accounting for tab bar height and window padding.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn pixel_to_grid_point(&self, pos: (f64, f64)) -> GridPoint {
        let pad_x = f64::from((self.config.padding_x * self.scale_factor as f32).round());
        let pad_y = f64::from((self.config.padding_y * self.scale_factor as f32).round());
        let tab_bar = f64::from((TAB_BAR_HEIGHT * self.scale_factor as f32).round());
        let sidebar_w = if self.sidebar_open {
            f64::from((crate::sidebar::SIDEBAR_WIDTH * self.scale_factor as f32).round())
        } else {
            0.0
        };
        let x_content = (pos.0 - sidebar_w - pad_x).max(0.0);
        let col = (x_content / f64::from(self.cell_metrics.cell_width)).max(0.0) as usize;
        let y_content = (pos.1 - tab_bar - pad_y).max(0.0);
        let row = (y_content / f64::from(self.cell_metrics.cell_height)).max(0.0) as usize;
        GridPoint::new(
            col.min(self.terminal.grid.cols.saturating_sub(1)),
            row.min(self.terminal.grid.rows.saturating_sub(1)),
        )
    }

    #[allow(clippy::too_many_lines)]
    fn handle_keyboard_input(&mut self, key_ev: &winit::event::KeyEvent, win: &Window) {
        self.cursor_blink_visible = true;
        self.last_blink = std::time::Instant::now();

        // Multi-tab keyboard shortcuts
        if key_ev.state == winit::event::ElementState::Pressed && self.modifiers.control_key() {
            use winit::keyboard::{Key, NamedKey};
            match &key_ev.logical_key {
                Key::Character(c) if c.eq_ignore_ascii_case("t") && self.modifiers.shift_key() => {
                    self.create_tab(None);
                    win.request_redraw();
                    return;
                }
                Key::Character(c) if c.eq_ignore_ascii_case("d") && self.modifiers.shift_key() => {
                    let dup_title = self
                        .tabs
                        .get(self.active_tab_index)
                        .map(|t| format!("{} (copy)", t.title));
                    self.create_tab(dup_title);
                    win.request_redraw();
                    return;
                }
                Key::Character(c) if c.eq_ignore_ascii_case("w") && self.modifiers.shift_key() => {
                    self.close_tab(self.active_tab_index);
                    win.request_redraw();
                    return;
                }
                Key::Character(c) if c.eq_ignore_ascii_case("b") => {
                    self.sidebar_open = !self.sidebar_open;
                    self.handle_resize(win.inner_size());
                    self.terminal.mark_all_dirty();
                    win.request_redraw();
                    return;
                }
                Key::Character(c) if c.eq_ignore_ascii_case("g") => {
                    self.changes_panel_open = !self.changes_panel_open;
                    self.handle_resize(win.inner_size());
                    self.terminal.mark_all_dirty();
                    win.request_redraw();
                    return;
                }
                Key::Character(c) if c.eq_ignore_ascii_case("d") && !self.modifiers.shift_key() => {
                    self.diff_view_open = !self.diff_view_open;
                    self.handle_resize(win.inner_size());
                    self.terminal.mark_all_dirty();
                    win.request_redraw();
                    return;
                }
                Key::Named(NamedKey::Tab) => {
                    if self.modifiers.shift_key() {
                        let prev = if self.active_tab_index == 0 {
                            self.tabs.len().saturating_sub(1)
                        } else {
                            self.active_tab_index - 1
                        };
                        self.select_tab(prev);
                    } else {
                        let next = (self.active_tab_index + 1) % self.tabs.len().max(1);
                        self.select_tab(next);
                    }
                    win.request_redraw();
                    return;
                }
                Key::Character(c)
                    if c.len() == 1
                        && c.chars().next().is_some_and(|ch| ('1'..='9').contains(&ch)) =>
                {
                    let digit = (c.as_bytes()[0] - b'1') as usize;
                    if digit < self.tabs.len() {
                        self.select_tab(digit);
                        win.request_redraw();
                        return;
                    }
                }
                _ => {}
            }
        }

        let key_event = KeyEvent::new(
            key_ev.logical_key.clone(),
            key_ev.text.as_ref().map(ToString::to_string),
            self.modifiers,
            key_ev.state,
        );

        let action = self
            .config
            .keybindings
            .check_action(&key_event)
            .or_else(|| check_input_action(&key_event));

        if let Some(action) = action {
            match action {
                InputAction::ScrollUp(lines) => {
                    let delta = isize::try_from(lines).unwrap_or(12);
                    self.terminal.grid.scroll_display(delta);
                    self.terminal.mark_all_dirty();
                    win.request_redraw();
                }
                InputAction::ScrollDown(lines) => {
                    let delta = isize::try_from(lines).unwrap_or(12);
                    self.terminal.grid.scroll_display(-delta);
                    self.terminal.mark_all_dirty();
                    win.request_redraw();
                }
                InputAction::ScrollToTop => {
                    self.terminal.grid.scroll_to_top();
                    self.terminal.mark_all_dirty();
                    win.request_redraw();
                }
                InputAction::ScrollToBottom => {
                    self.terminal.grid.scroll_to_bottom();
                    self.terminal.mark_all_dirty();
                    win.request_redraw();
                }
                InputAction::Copy => {
                    self.copy_selection();
                }
                InputAction::Paste => {
                    self.paste_clipboard(win);
                }
                InputAction::ZoomIn => {
                    self.zoom_in();
                }
                InputAction::ZoomOut => {
                    self.zoom_out();
                }
                InputAction::ZoomReset => {
                    self.zoom_reset();
                }
                InputAction::CycleTheme => {
                    self.cycle_theme(win);
                }
                InputAction::ToggleStatusBar => {
                    self.config.status_bar = !self.config.status_bar;
                    self.handle_resize(win.inner_size());
                    self.terminal.mark_all_dirty();
                    win.request_redraw();
                }
            }
            return;
        }

        if let Some(bytes) = encode_key(&key_event, &self.terminal.modes) {
            if self.terminal.grid.display_offset > 0 {
                self.terminal.grid.scroll_to_bottom();
                self.terminal.mark_all_dirty();
                win.request_redraw();
            }
            if let Some(ref writer) = self.pty_writer
                && let Ok(mut guard) = writer.lock()
            {
                let _ = guard.write(&bytes);
            }
        }
    }

    fn handle_mouse_wheel(&mut self, delta: winit::event::MouseScrollDelta, win: &Window) {
        let lines: isize = match delta {
            winit::event::MouseScrollDelta::LineDelta(_, y) => {
                if y > 0.0 {
                    3
                } else if y < 0.0 {
                    -3
                } else {
                    0
                }
            }
            winit::event::MouseScrollDelta::PixelDelta(pos) => {
                if pos.y > 0.0 {
                    3
                } else if pos.y < 0.0 {
                    -3
                } else {
                    0
                }
            }
        };
        if lines != 0 {
            self.terminal.grid.scroll_display(lines);
            self.terminal.mark_all_dirty();
            win.request_redraw();
        }
    }

    /// Copies currently selected text to the system clipboard.
    pub fn copy_selection(&mut self) {
        if let Some(ref sel) = self.selection {
            let text = sel.extract_text(&self.terminal.grid);
            if !text.is_empty()
                && let Some(ref mut clip) = self.clipboard
            {
                let _ = clip.set_text(text);
            }
        }
    }

    /// Pastes text from the system clipboard to the PTY.
    pub fn paste_clipboard(&mut self, win: &Window) {
        let text = self.clipboard.as_mut().and_then(|c| c.get_text().ok());
        if let Some(text) = text
            && !text.is_empty()
        {
            let bytes = encode_paste(&text, &self.terminal.modes);
            if self.terminal.grid.display_offset > 0 {
                self.terminal.grid.scroll_to_bottom();
                win.request_redraw();
            }
            if let Some(ref writer) = self.pty_writer
                && let Ok(mut guard) = writer.lock()
            {
                let _ = guard.write(&bytes);
            }
        }
    }

    /// Handles window resize event, strictly following the order:
    /// 1. Window / GPU surface resize
    /// 2. Terminal grid resize
    /// 3. PTY master resize
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn handle_resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }

        // 1. Window / GPU surface
        if let Some(ref mut gpu) = self.gpu {
            gpu.resize(size);
        }

        // 2. Terminal grid (content area between tab bar and status bar, with padding)
        let effective_pad_x = (self.config.padding_x * self.scale_factor as f32).round();
        let effective_pad_y = (self.config.padding_y * self.scale_factor as f32).round();
        let tab_bar_px = (TAB_BAR_HEIGHT * self.scale_factor as f32).round() as u32;
        let status_bar_px = if self.config.status_bar {
            (crate::status_bar::STATUS_BAR_HEIGHT * self.scale_factor as f32).round() as u32
        } else {
            0
        };
        let sidebar_px = if self.sidebar_open {
            (crate::sidebar::SIDEBAR_WIDTH * self.scale_factor as f32).round() as u32
        } else {
            0
        };
        let changes_panel_px = if self.changes_panel_open {
            (crate::changes_panel::CHANGES_PANEL_WIDTH * self.scale_factor as f32).round() as u32
        } else {
            0
        };

        let avail_middle = size
            .width
            .saturating_sub(sidebar_px)
            .saturating_sub(changes_panel_px);
        let term_width = if self.diff_view_open {
            (avail_middle * 2 / 5).clamp(240, 520)
        } else {
            avail_middle
        };

        let content_width = term_width
            .saturating_sub((effective_pad_x * 2.0) as u32)
            .max(1);
        let content_height = size
            .height
            .saturating_sub(tab_bar_px)
            .saturating_sub(status_bar_px)
            .saturating_sub((effective_pad_y * 2.0) as u32)
            .max(1);
        let content_size = PhysicalSize::new(content_width, content_height);

        let (cols, rows) = window_size_to_grid_dimensions(content_size, &self.cell_metrics);
        let pty_size = window_size_to_pty_size(content_size, &self.cell_metrics);

        for tab in &mut self.tabs {
            tab.resize(cols as usize, rows as usize, pty_size);
        }
        self.terminal.resize(cols as usize, rows as usize);

        // 3. PTY master
        if let Some(ref writer) = self.pty_writer
            && let Ok(mut guard) = writer.lock()
        {
            let _ = guard.resize(pty_size);
        }
    }

    /// Recalculates cell metrics based on the base font size and scale factor,
    /// invalidates shaped glyph caches, and resizes grid and PTY.
    #[allow(clippy::cast_possible_truncation)]
    pub fn update_scale(&mut self) {
        let effective_size = (self.base_font_size * (self.scale_factor as f32)).max(4.0);
        let mut font_system = glyphon::FontSystem::new();
        self.cell_metrics = CellMetrics::measure(&mut font_system, effective_size, 1.2);
        if let Some(ref mut pipeline) = self.glyph_pipeline {
            pipeline.glyph_cache.clear();
        }
        self.terminal.mark_all_dirty();
        if let Some(win) = self.window.clone() {
            self.handle_resize(win.inner_size());
            win.request_redraw();
        }
    }

    /// Handles changes to display `HiDPI` scale factor.
    pub fn handle_scale_factor_changed(&mut self, scale_factor: f64) {
        if (self.scale_factor - scale_factor).abs() > f64::EPSILON {
            self.scale_factor = scale_factor;
            self.update_scale();
        }
    }

    /// Increases base font size by one zoom step.
    pub fn zoom_in(&mut self) {
        self.base_font_size = (self.base_font_size + Self::FONT_SIZE_STEP).min(Self::MAX_FONT_SIZE);
        self.update_scale();
    }

    /// Decreases base font size by one zoom step.
    pub fn zoom_out(&mut self) {
        self.base_font_size = (self.base_font_size - Self::FONT_SIZE_STEP).max(Self::MIN_FONT_SIZE);
        self.update_scale();
    }

    /// Resets base font size to default.
    pub fn zoom_reset(&mut self) {
        self.base_font_size = Self::DEFAULT_FONT_SIZE;
        self.update_scale();
    }

    /// Cycles to the next available color theme.
    pub fn cycle_theme(&mut self, win: &Window) {
        let themes = crate::config::available_themes();
        if themes.is_empty() {
            return;
        }
        let current_index = themes
            .iter()
            .position(|name| name == &self.config.theme.name)
            .unwrap_or(0);
        let next_index = (current_index + 1) % themes.len();
        let next_theme_name = &themes[next_index];
        if let Some(next_theme) = crate::config::load_theme_by_name(next_theme_name) {
            log::info!("Switched theme to '{}'", next_theme.name);
            self.config.theme = next_theme;
            self.terminal.mark_all_dirty();
            win.request_redraw();
        }
    }

    /// Builds formatted text labels for tab titles, close buttons, and new-tab button.
    #[must_use]
    pub fn build_tab_labels(&self, window_width: f32) -> Vec<TabLabel> {
        let count = self.tabs.len();
        if count == 0 {
            return Vec::new();
        }
        let (layouts, plus_btn, sidebar_btn) = compute_tab_layouts(count, window_width);
        let (split_btn, max_btn) = crate::tab::compute_chrome_buttons(window_width);
        let mut labels = Vec::new();

        // Sidebar toggle button label [◨]
        labels.push(TabLabel {
            text: "◨".to_string(),
            x: sidebar_btn.0 + 3.0,
            y: sidebar_btn.1 + 1.0,
            color: glyphon::Color::rgb(180, 185, 200),
            bold: true,
            max_width: crate::tab::TAB_BUTTON_SIZE,
        });

        for (idx, layout) in layouts.iter().enumerate() {
            let is_active = idx == self.active_tab_index;
            let tab = &self.tabs[idx];
            let agent = tab.detect_agent();
            let (agent_icon, agent_name) = agent.display_badge();

            let color = if is_active {
                if agent == crate::tab::AgentKind::Shell {
                    glyphon::Color::rgb(240, 242, 250)
                } else {
                    let [r, g, b] = agent.brand_color();
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    glyphon::Color::rgb(
                        (r * 255.0).round() as u8,
                        (g * 255.0).round() as u8,
                        (b * 255.0).round() as u8,
                    )
                }
            } else {
                glyphon::Color::rgb(140, 145, 160)
            };

            // Tab title with agent badge icon
            let display_title = if agent != crate::tab::AgentKind::Shell {
                format!("{agent_icon} {agent_name}")
            } else if tab.title.starts_with("Tab ") {
                format!("⌁ {}", tab.title)
            } else if !tab.title.starts_with('⌁') && !tab.title.starts_with('❯') {
                format!("❯ {}", tab.title)
            } else {
                tab.title.clone()
            };

            let max_text_width = (layout.width - crate::tab::TAB_BUTTON_SIZE - 12.0).max(10.0);
            labels.push(TabLabel {
                text: display_title,
                x: layout.x + 8.0,
                y: layout.y + 4.0,
                color,
                bold: is_active,
                max_width: max_text_width,
            });

            // Close button '×'
            labels.push(TabLabel {
                text: "×".to_string(),
                x: layout.close_btn_x + 5.0,
                y: layout.close_btn_y + 1.0,
                color: if is_active {
                    glyphon::Color::rgb(200, 205, 220)
                } else {
                    glyphon::Color::rgb(120, 125, 140)
                },
                bold: true,
                max_width: crate::tab::TAB_BUTTON_SIZE,
            });
        }

        // Plus button '+'
        labels.push(TabLabel {
            text: "+".to_string(),
            x: plus_btn.0 + 5.0,
            y: plus_btn.1 + 1.0,
            color: glyphon::Color::rgb(190, 195, 210),
            bold: true,
            max_width: crate::tab::TAB_BUTTON_SIZE,
        });

        // Split pane button [◫]
        labels.push(TabLabel {
            text: "◫".to_string(),
            x: split_btn.0 + 3.0,
            y: split_btn.1 + 1.0,
            color: glyphon::Color::rgb(180, 185, 200),
            bold: true,
            max_width: crate::tab::TAB_BUTTON_SIZE,
        });

        // Maximize button [⤢]
        labels.push(TabLabel {
            text: "⤢".to_string(),
            x: max_btn.0 + 3.0,
            y: max_btn.1 + 1.0,
            color: glyphon::Color::rgb(180, 185, 200),
            bold: true,
            max_width: crate::tab::TAB_BUTTON_SIZE,
        });

        labels
    }

    /// Builds formatted text labels for the bottom status bar.
    #[must_use]
    pub fn build_status_bar_labels(&self, window_width: f32, window_height: f32) -> Vec<TabLabel> {
        let shell_name = if let Some(ref sh) = self.config.shell {
            std::path::Path::new(sh)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("shell")
                .to_string()
        } else {
            #[cfg(windows)]
            {
                "pwsh".to_string()
            }
            #[cfg(not(windows))]
            {
                "sh".to_string()
            }
        };

        let is_alt_screen = self.terminal.modes.alternate_screen;
        let cursor_row = self.terminal.grid.cursor.row + 1;
        let cursor_col = self.terminal.grid.cursor.col + 1;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let zoom_pct = (self.base_font_size / Self::DEFAULT_FONT_SIZE * 100.0).round() as usize;
        let theme_name = self.config.theme.name.clone();
        let tab_current = self.active_tab_index + 1;
        let tab_total = self.tabs.len();

        let state = crate::status_bar::StatusBarState {
            shell_name,
            is_alt_screen,
            cursor_row,
            cursor_col,
            zoom_pct,
            theme_name,
            tab_current,
            tab_total,
            git_branch: self.git_branch.clone(),
            theme_menu_open: self.theme_menu_open,
        };

        crate::status_bar::build_status_bar_labels(&state, window_width, window_height)
    }

    /// Creates and switches to a new tab, spawning a shell PTY if proxy is available.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    pub fn create_tab(&mut self, title: Option<String>) {
        let tab_id = self.next_tab_id;
        self.next_tab_id += 1;

        let cols = self.terminal.grid.cols;
        let rows = self.terminal.grid.rows;
        let new_terminal = Terminal::new(cols, rows);

        let mut pty_writer = None;
        if let Some(ref proxy) = self.proxy {
            let cmd = if let Some(ref custom_shell) = self.config.shell {
                crate::pty::ShellCommand::with_args(
                    custom_shell.clone(),
                    self.config.shell_args.clone(),
                )
            } else {
                crate::pty::ShellCommand::default_shell()
            };

            let pty_size = crate::pty::PtySize {
                rows: u16::try_from(rows).unwrap_or(24),
                cols: u16::try_from(cols).unwrap_or(80),
                px_w: (cols as f32 * self.cell_metrics.cell_width) as u16,
                px_h: (rows as f32 * self.cell_metrics.cell_height) as u16,
            };

            if let Ok((writer, reader)) = crate::pty::spawn(cmd, pty_size) {
                let writer = Arc::new(Mutex::new(writer));
                spawn_tab_pty_forwarder(tab_id, reader, Arc::clone(&writer), proxy.clone());
                pty_writer = Some(writer);
            }
        }

        let tab_title = title.unwrap_or_else(|| format!("Tab {}", tab_id + 1));
        let tab = Tab::new(tab_id, tab_title, new_terminal, pty_writer);
        self.tabs.push(tab);
        self.select_tab(self.tabs.len() - 1);
    }

    /// Switches the active view to the tab at `index`.
    pub fn select_tab(&mut self, index: usize) {
        if index >= self.tabs.len() || index == self.active_tab_index {
            return;
        }

        // Save active tab state
        if let Some(active_tab) = self.tabs.get_mut(self.active_tab_index) {
            active_tab.terminal = self.terminal.clone();
            active_tab.selection = self.selection;
            active_tab.mouse_down_pos = self.mouse_down_pos;
        }

        self.active_tab_index = index;

        // Restore new active tab state
        let new_tab = &self.tabs[index];
        self.terminal = new_tab.terminal.clone();
        self.pty_writer = new_tab.pty_writer.clone();
        self.selection = new_tab.selection;
        self.mouse_down_pos = new_tab.mouse_down_pos;
        self.pty_exited = new_tab.pty_exited;
        self.pty_exit_code = new_tab.pty_exit_code;
        self.title = new_tab.title.clone();

        if let Some(ref win) = self.window {
            win.set_title(&self.title);
            win.request_redraw();
        }
        self.terminal.mark_all_dirty();
    }

    /// Refreshes git status, changes panel list, and diff view from repository.
    pub fn refresh_git_status(&mut self) {
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let proj_info = crate::git::detect_project_type(&cwd);
        self.git_branch = crate::git::detect_git_branch(&cwd);
        let changed_files = crate::git::query_git_status(&cwd);
        let total_adds: usize = changed_files.iter().map(|f| f.additions).sum();
        let total_dels: usize = changed_files.iter().map(|f| f.deletions).sum();

        self.changes_panel_state
            .changed_files
            .clone_from(&changed_files);
        if let Some(ref branch) = self.git_branch {
            self.changes_panel_state.git_branch.clone_from(branch);
        }

        if let Some(ws) = self.sidebar_state.workspaces.first_mut() {
            ws.name = format!("{} ({})", proj_info.name, proj_info.kind);
            ws.modified_count = changed_files.len();
            ws.diff_stat = Some((total_adds, total_dels));
            if let Some(ref branch) = self.git_branch {
                ws.branch.clone_from(branch);
            }
        }

        if let Some(idx) = self.changes_panel_state.selected_file_index
            && let Some(file) = changed_files.get(idx)
        {
            let rel_path = if file.path_info == "." {
                file.filename.clone()
            } else {
                format!("{}/{}", file.path_info.trim_end_matches('/'), file.filename)
            };
            let (fname, pinfo, hunks) = crate::git::query_file_diff(&cwd, &rel_path);
            self.diff_view_state.file_name = fname;
            self.diff_view_state.path_info = pinfo;
            self.diff_view_state.hunks = hunks;
            self.diff_view_state.staged = file.staged;
        }
    }

    /// Closes the tab at `index`. If it's the only tab, it remains open.
    pub fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() || self.tabs.len() <= 1 {
            return;
        }

        let removed = self.tabs.remove(index);
        drop(removed);

        if self.active_tab_index >= self.tabs.len() {
            self.active_tab_index = self.tabs.len() - 1;
        } else if self.active_tab_index > index {
            self.active_tab_index -= 1;
        }

        let new_active = &self.tabs[self.active_tab_index];
        self.terminal = new_active.terminal.clone();
        self.pty_writer = new_active.pty_writer.clone();
        self.selection = new_active.selection;
        self.mouse_down_pos = new_active.mouse_down_pos;
        self.pty_exited = new_active.pty_exited;
        self.pty_exit_code = new_active.pty_exit_code;
        self.title = new_active.title.clone();

        if let Some(ref win) = self.window {
            win.set_title(&self.title);
            win.request_redraw();
        }
        self.terminal.mark_all_dirty();
    }

    /// Feeds PTY output bytes to the active tab's VT parser and requests a redraw if damaged.
    pub fn handle_pty_output(&mut self, bytes: &[u8]) {
        let active_id = self.tabs.get(self.active_tab_index).map_or(0, |t| t.id);
        self.handle_tab_pty_output(active_id, bytes);
    }

    /// Feeds PTY output bytes to a specific tab's parser and terminal state.
    pub fn handle_tab_pty_output(&mut self, tab_id: usize, bytes: &[u8]) {
        let is_active = self
            .tabs
            .get(self.active_tab_index)
            .is_some_and(|t| t.id == tab_id);

        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            // Respond to DSR (Device Status Report: \x1b[6n) if requested
            if bytes.windows(4).any(|w| w == b"\x1b[6n")
                && let Some(ref writer) = tab.pty_writer
                && let Ok(mut guard) = writer.lock()
            {
                let row = tab.terminal.grid.cursor.row + 1;
                let col = tab.terminal.grid.cursor.col + 1;
                let response = format!("\x1b[{row};{col}R");
                let _ = guard.write(response.as_bytes());
            }

            tab.advance_parser(bytes);

            if is_active {
                self.terminal = tab.terminal.clone();
                if !tab.title.is_empty() && tab.title != self.title {
                    self.title = tab.title.clone();
                    if let Some(ref win) = self.window {
                        win.set_title(&self.title);
                    }
                }

                if self.terminal.bell_count != self.last_bell_count {
                    self.last_bell_count = self.terminal.bell_count;
                    self.bell_triggered_at = Some(std::time::Instant::now());
                    if let Some(ref win) = self.window {
                        win.request_redraw();
                    }
                }

                if self.terminal.has_damage()
                    && let Some(ref win) = self.window
                {
                    win.request_redraw();
                }
            } else if let Some(ref win) = self.window {
                win.request_redraw();
            }
        }
    }

    /// Handles child process exit notification for a specific tab.
    pub fn handle_tab_pty_exited(&mut self, tab_id: usize, code: Option<i32>) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.pty_exited = true;
            tab.pty_exit_code = code;
        }

        let is_active = self
            .tabs
            .get(self.active_tab_index)
            .is_some_and(|t| t.id == tab_id);
        if is_active {
            self.pty_exited = true;
            self.pty_exit_code = code;
            let exit_msg = match code {
                Some(c) => format!("{} [Process exited - {c}]", self.title),
                None => format!("{} [Process exited]", self.title),
            };
            if let Some(ref win) = self.window {
                win.set_title(&exit_msg);
                win.request_redraw();
            }
        }
    }

    #[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
    pub fn assemble_quad_vertices(
        &self,
        bell_active: bool,
        screen_size: [f32; 2],
    ) -> Vec<crate::renderer::draw::QuadVertex> {
        let palette = self.config.theme.to_f32_palette();
        let cursor_color = self.config.theme.cursor.to_rgba_array(1.0);
        let selection_color = self.config.theme.selection_bg.to_rgba_array(1.0);
        let accent_color = palette[12]; // Vivid cyan/blue accent for active tab

        let effective_pad_x = (self.config.padding_x * self.scale_factor as f32).round();
        let effective_pad_y = (self.config.padding_y * self.scale_factor as f32).round();
        let tab_bar_px = (TAB_BAR_HEIGHT * self.scale_factor as f32).round();
        let status_bar_px = if self.config.status_bar {
            (crate::status_bar::STATUS_BAR_HEIGHT * self.scale_factor as f32).round()
        } else {
            0.0
        };
        let sidebar_w = if self.sidebar_open {
            (crate::sidebar::SIDEBAR_WIDTH * self.scale_factor as f32).round()
        } else {
            0.0
        };
        let x_offset = sidebar_w + effective_pad_x;
        let y_offset = tab_bar_px + effective_pad_y;

        let mut quad_vertices = Vec::new();

        // 1. Tab bar quads (background, tab bodies, active indicator, dividers)
        quad_vertices.extend_from_slice(&assemble_tab_bar_quads(
            self.tabs.len(),
            self.active_tab_index,
            screen_size[0],
            accent_color,
        ));

        // 1.5. Sidebar quads (if expanded)
        if self.sidebar_open {
            let sidebar_h = (screen_size[1] - tab_bar_px - status_bar_px).max(0.0);
            quad_vertices.extend_from_slice(&crate::sidebar::assemble_sidebar_quads(
                tab_bar_px,
                sidebar_h,
                accent_color,
                self.active_tab_index,
                self.tabs.len(),
            ));
        }

        // 2. Cell backgrounds offset below the tab bar with padding
        quad_vertices.extend_from_slice(&build_cell_background_vertices_with_offsets(
            &self.terminal.grid,
            self.cell_metrics.cell_width,
            self.cell_metrics.cell_height,
            &palette,
            x_offset,
            y_offset,
        ));

        // 3. Text decorations offset below the tab bar with padding
        quad_vertices.extend_from_slice(&build_text_decoration_vertices_with_offsets(
            &self.terminal.grid,
            self.cell_metrics.cell_width,
            self.cell_metrics.cell_height,
            &palette,
            x_offset,
            y_offset,
        ));

        // 4. Cursor offset below the tab bar with padding
        let should_draw_cursor = self.terminal.grid.display_offset == 0
            && self.terminal.grid.cursor.visible
            && (!self.terminal.cursor_blinking || self.cursor_blink_visible || !self.has_focus);

        if should_draw_cursor {
            let cursor_width = self
                .terminal
                .grid
                .get(self.terminal.grid.cursor.row, self.terminal.grid.cursor.col)
                .map_or(1, |c| c.width.max(1));
            let cursor_vertices = build_cursor_vertices_with_offsets(
                &self.terminal.grid.cursor,
                &self.cell_metrics,
                cursor_color,
                cursor_width,
                x_offset,
                y_offset,
            );
            quad_vertices.extend_from_slice(&cursor_vertices);
        }

        // 5. Selection highlight offset below the tab bar with padding
        quad_vertices.extend_from_slice(&build_selection_vertices_with_offsets(
            &self.terminal.grid,
            self.selection.as_ref(),
            self.cell_metrics.cell_width,
            self.cell_metrics.cell_height,
            selection_color,
            x_offset,
            y_offset,
        ));

        // 5.5. Central Side-by-side Diff View workspace quads (if open)
        let changes_panel_w = if self.changes_panel_open {
            (crate::changes_panel::CHANGES_PANEL_WIDTH * self.scale_factor as f32).round()
        } else {
            0.0
        };
        let avail_middle = (screen_size[0] - sidebar_w - changes_panel_w).max(0.0);
        let term_w = if self.diff_view_open {
            (avail_middle * 0.4).clamp(240.0, 520.0)
        } else {
            avail_middle
        };
        let diff_w = if self.diff_view_open {
            (avail_middle - term_w).max(0.0)
        } else {
            0.0
        };
        let diff_x = sidebar_w + term_w;
        let panel_x = screen_size[0] - changes_panel_w;

        if self.diff_view_open && diff_w > 10.0 {
            let diff_h = (screen_size[1] - tab_bar_px - status_bar_px).max(0.0);
            quad_vertices.extend_from_slice(&crate::diff_view::assemble_diff_view_quads(
                diff_x,
                tab_bar_px,
                diff_w,
                diff_h,
                accent_color,
                &self.diff_view_state,
            ));
        }

        // 5.6. Right Changes & Git Panel quads (if open)
        if self.changes_panel_open && changes_panel_w > 10.0 {
            let panel_h = (screen_size[1] - tab_bar_px - status_bar_px).max(0.0);
            quad_vertices.extend_from_slice(&crate::changes_panel::assemble_changes_panel_quads(
                panel_x,
                tab_bar_px,
                changes_panel_w,
                panel_h,
                accent_color,
                &self.changes_panel_state,
            ));
        }

        // 6. Status bar quads (if enabled)
        if self.config.status_bar {
            quad_vertices.extend_from_slice(&crate::status_bar::assemble_status_bar_quads(
                screen_size[0],
                screen_size[1],
                self.terminal.modes.alternate_screen,
                accent_color,
            ));

            if self.theme_menu_open {
                let state = crate::status_bar::StatusBarState {
                    theme_name: self.config.theme.name.clone(),
                    theme_menu_open: true,
                    ..crate::status_bar::StatusBarState::default()
                };
                quad_vertices.extend_from_slice(&crate::status_bar::assemble_theme_menu_quads(
                    screen_size[0],
                    screen_size[1],
                    accent_color,
                    &state,
                ));
            }
        }

        // 7. Scrollbar indicator (rendered when viewing history / display_offset > 0)
        quad_vertices.extend_from_slice(&self.assemble_scrollbar_quads(
            screen_size,
            y_offset,
            effective_pad_y,
            accent_color,
        ));

        // 8. Visual bell overlay
        if bell_active {
            quad_vertices.extend_from_slice(&build_quad_vertices(
                0.0,
                0.0,
                screen_size[0],
                screen_size[1],
                [1.0, 1.0, 1.0, 0.25],
            ));
        }

        quad_vertices
    }

    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    fn assemble_scrollbar_quads(
        &self,
        screen_size: [f32; 2],
        y_offset: f32,
        effective_pad_y: f32,
        accent_color: [f32; 4],
    ) -> Vec<crate::renderer::draw::QuadVertex> {
        if self.terminal.grid.display_offset == 0 || self.terminal.grid.scrollback.is_empty() {
            return Vec::new();
        }

        let status_bar_px = if self.config.status_bar {
            (crate::status_bar::STATUS_BAR_HEIGHT * self.scale_factor as f32).round()
        } else {
            0.0
        };
        let track_top = y_offset;
        let track_height = (screen_size[1] - track_top - status_bar_px - effective_pad_y).max(10.0);
        let scrollbar_width = 4.0;
        let track_x = screen_size[0] - scrollbar_width - 2.0;

        let mut quads = Vec::with_capacity(12);

        // Track background quad (subtle translucent bar)
        quads.extend_from_slice(&build_quad_vertices(
            track_x,
            track_top,
            scrollbar_width,
            track_height,
            [0.05, 0.055, 0.08, 0.5],
        ));

        // Thumb quad (neon phosphor indicator)
        let total_lines = (self.terminal.grid.scrollback.len() + self.terminal.grid.rows) as f32;
        let thumb_height = ((self.terminal.grid.rows as f32 / total_lines) * track_height)
            .clamp(16.0, track_height);
        let max_offset = self.terminal.grid.scrollback.len() as f32;
        let offset_ratio =
            1.0 - (self.terminal.grid.display_offset as f32 / max_offset).clamp(0.0, 1.0);
        let thumb_y = track_top + offset_ratio * (track_height - thumb_height);

        quads.extend_from_slice(&build_quad_vertices(
            track_x,
            thumb_y,
            scrollbar_width,
            thumb_height,
            [accent_color[0], accent_color[1], accent_color[2], 0.85],
        ));

        quads
    }

    /// Renders the terminal grid, cell backgrounds, glyphs, and cursor to the window surface.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::too_many_lines
    )]
    pub fn render(&mut self) {
        let is_bell_active = self
            .bell_triggered_at
            .is_some_and(|t| t.elapsed() < Self::VISUAL_BELL_DURATION);

        let screen_size = match self.gpu {
            Some(ref gpu) => [gpu.config.width as f32, gpu.config.height as f32],
            None => return,
        };
        let quad_vertices = self.assemble_quad_vertices(is_bell_active, screen_size);
        let mut tab_labels = self.build_tab_labels(screen_size[0]);
        let tab_bar_px = (TAB_BAR_HEIGHT * self.scale_factor as f32).round();
        let status_bar_px = if self.config.status_bar {
            (crate::status_bar::STATUS_BAR_HEIGHT * self.scale_factor as f32).round()
        } else {
            0.0
        };
        let sidebar_w = if self.sidebar_open {
            (crate::sidebar::SIDEBAR_WIDTH * self.scale_factor as f32).round()
        } else {
            0.0
        };
        let changes_panel_w = if self.changes_panel_open {
            (crate::changes_panel::CHANGES_PANEL_WIDTH * self.scale_factor as f32).round()
        } else {
            0.0
        };
        let avail_middle = (screen_size[0] - sidebar_w - changes_panel_w).max(0.0);
        let term_w = if self.diff_view_open {
            (avail_middle * 0.4).clamp(240.0, 520.0)
        } else {
            avail_middle
        };
        let diff_w = if self.diff_view_open {
            (avail_middle - term_w).max(0.0)
        } else {
            0.0
        };
        let diff_x = sidebar_w + term_w;
        let panel_x = screen_size[0] - changes_panel_w;

        if self.sidebar_open {
            let sidebar_state = crate::sidebar::SidebarState {
                workspace_name: self.workspace_name.clone(),
                git_branch: self.git_branch.clone(),
                tab_titles: self.tabs.iter().map(|t| t.title.clone()).collect(),
                active_tab_index: self.active_tab_index,
                workspaces: self.sidebar_state.workspaces.clone(),
                chats: self.sidebar_state.chats.clone(),
                git_status: self.sidebar_state.git_status.clone(),
            };
            tab_labels.extend(crate::sidebar::build_sidebar_labels(
                &sidebar_state,
                tab_bar_px,
            ));
        }

        if self.diff_view_open && diff_w > 10.0 {
            let diff_h = (screen_size[1] - tab_bar_px - status_bar_px).max(0.0);
            tab_labels.extend(crate::diff_view::build_diff_view_labels(
                diff_x,
                tab_bar_px,
                diff_w,
                diff_h,
                &self.diff_view_state,
            ));
        }

        if self.changes_panel_open && changes_panel_w > 10.0 {
            let panel_h = (screen_size[1] - tab_bar_px - status_bar_px).max(0.0);
            tab_labels.extend(crate::changes_panel::build_changes_panel_labels(
                panel_x,
                tab_bar_px,
                changes_panel_w,
                panel_h,
                &self.changes_panel_state,
            ));
        }

        if self.config.status_bar {
            tab_labels.extend(self.build_status_bar_labels(screen_size[0], screen_size[1]));
        }

        let palette = self.config.theme.to_f32_palette();

        let (Some(gpu), Some(quad_pipeline), Some(glyph_pipeline)) = (
            &mut self.gpu,
            &mut self.quad_pipeline,
            &mut self.glyph_pipeline,
        ) else {
            return;
        };

        let output = match gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                eprintln!("GPU surface validation error");
                return;
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render-encoder"),
            });

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("terminal-render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            let screen_size = [gpu.config.width as f32, gpu.config.height as f32];
            quad_pipeline.draw(
                &gpu.device,
                &gpu.queue,
                &mut rpass,
                screen_size,
                &quad_vertices,
            );

            let effective_pad_x = (self.config.padding_x * self.scale_factor as f32).round();
            let effective_pad_y = (self.config.padding_y * self.scale_factor as f32).round();
            let x_offset = sidebar_w + effective_pad_x;
            let y_offset = tab_bar_px + effective_pad_y;

            let draw_ctx = DrawContext {
                device: &gpu.device,
                queue: &gpu.queue,
                screen_size: [gpu.config.width, gpu.config.height],
                metrics: &self.cell_metrics,
                palette: &palette,
                dirty_rows: Some(&self.terminal.dirty_rows),
                x_offset,
                y_offset,
                tab_labels: &tab_labels,
            };
            let _ = glyph_pipeline.draw(&draw_ctx, &mut rpass, &self.terminal.grid);
        }

        gpu.queue.submit(std::iter::once(encoder.finish()));
        gpu.queue.present(output);
        self.terminal.clear_damage();
    }
}

impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            match create_window(event_loop, &self.title) {
                Ok(win) => {
                    self.scale_factor = win.scale_factor();
                    match GpuContext::new(Arc::clone(&win)) {
                        Ok(gpu) => {
                            let quad_pipeline = QuadPipeline::new(&gpu.device, gpu.config.format);
                            let glyph_pipeline =
                                GlyphPipeline::new(&gpu.device, &gpu.queue, gpu.config.format);
                            self.gpu = Some(gpu);
                            self.quad_pipeline = Some(quad_pipeline);
                            self.glyph_pipeline = Some(glyph_pipeline);
                        }
                        Err(err) => eprintln!("Failed to initialize GPU context: {err}"),
                    }
                    self.window = Some(win.clone());
                    self.update_scale();
                    win.request_redraw();
                }
                Err(err) => eprintln!("Failed to create window: {err}"),
            }
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::PtyOutput(bytes) => {
                self.handle_pty_output(&bytes);
            }
            AppEvent::TabPtyOutput { tab_id, bytes } => {
                self.handle_tab_pty_output(tab_id, &bytes);
            }
            AppEvent::PtyExited(code) => {
                let active_id = self.tabs.get(self.active_tab_index).map_or(0, |t| t.id);
                self.handle_tab_pty_exited(active_id, code);
            }
            AppEvent::TabPtyExited { tab_id, code } => {
                self.handle_tab_pty_exited(tab_id, code);
            }
            AppEvent::ConfigReloaded => {
                log::info!("Configuration file reloaded from disk");
                if let Some(path) = crate::config::config_path() {
                    match crate::config::Config::load_from_path(&path) {
                        Ok(new_config) => {
                            self.config = new_config;
                            self.base_font_size = self.config.font_size;
                            self.update_scale();
                            self.terminal.mark_all_dirty();
                            if let Some(ref win) = self.window {
                                win.request_redraw();
                            }
                        }
                        Err(err) => {
                            log::warn!("Failed to reload config from '{}': {err}", path.display());
                        }
                    }
                }
            }
            AppEvent::ThemeChanged(theme_name) => {
                if let Some(new_theme) = crate::config::load_theme_by_name(&theme_name) {
                    self.config.theme = new_theme;
                    self.terminal.mark_all_dirty();
                    if let Some(ref win) = self.window {
                        win.request_redraw();
                    }
                }
            }
        }
    }

    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::too_many_lines
    )]
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(win) = self.window.clone() else {
            return;
        };
        if win.id() != window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                self.handle_resize(size);
                win.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.handle_scale_factor_changed(scale_factor);
            }
            WindowEvent::RedrawRequested => {
                self.render();
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
            }
            WindowEvent::KeyboardInput { event: key_ev, .. } => {
                self.handle_keyboard_input(&key_ev, &win);
            }
            WindowEvent::MouseWheel { delta, .. } if !self.terminal.modes.mouse_reporting => {
                self.handle_mouse_wheel(delta, &win);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_pixel_pos = (position.x, position.y);
                if let Some(start) = self.mouse_down_pos {
                    let current = self.pixel_to_grid_point(self.cursor_pixel_pos);
                    self.selection = Some(Selection::new(start, current));
                    win.request_redraw();
                }
            }
            WindowEvent::MouseInput {
                state,
                button: winit::event::MouseButton::Left,
                ..
            } if !self.terminal.modes.mouse_reporting => {
                if self.cursor_pixel_pos.1 <= f64::from(TAB_BAR_HEIGHT) {
                    if state == winit::event::ElementState::Pressed {
                        let win_w = win.inner_size().width as f32;
                        let action = hit_test(
                            self.cursor_pixel_pos.0 as f32,
                            self.cursor_pixel_pos.1 as f32,
                            self.tabs.len(),
                            win_w,
                        );
                        match action {
                            TabBarAction::SelectTab(idx) => {
                                self.select_tab(idx);
                            }
                            TabBarAction::CloseTab(idx) => {
                                self.close_tab(idx);
                            }
                            TabBarAction::NewTab => {
                                self.create_tab(None);
                            }
                            TabBarAction::ToggleSidebar => {
                                self.sidebar_open = !self.sidebar_open;
                                self.handle_resize(win.inner_size());
                                self.terminal.mark_all_dirty();
                            }
                            TabBarAction::ToggleSplit => {
                                self.diff_view_open = !self.diff_view_open;
                                self.handle_resize(win.inner_size());
                                self.terminal.mark_all_dirty();
                            }
                            TabBarAction::ToggleMaximize => {
                                let is_fullscreen = win.fullscreen().is_some();
                                if is_fullscreen {
                                    win.set_fullscreen(None);
                                } else {
                                    win.set_fullscreen(Some(
                                        winit::window::Fullscreen::Borderless(None),
                                    ));
                                }
                            }
                            TabBarAction::None => {}
                        }
                        win.request_redraw();
                    }
                    return;
                }

                // Check sidebar click if sidebar is open
                let sidebar_px = if self.sidebar_open {
                    f64::from((crate::sidebar::SIDEBAR_WIDTH * self.scale_factor as f32).round())
                } else {
                    0.0
                };
                if self.sidebar_open && self.cursor_pixel_pos.0 <= sidebar_px {
                    if state == winit::event::ElementState::Pressed {
                        let tab_bar_px = (TAB_BAR_HEIGHT * self.scale_factor as f32).round();
                        let status_bar_px = if self.config.status_bar {
                            (crate::status_bar::STATUS_BAR_HEIGHT * self.scale_factor as f32)
                                .round()
                        } else {
                            0.0
                        };
                        let sidebar_h =
                            (win.inner_size().height as f32 - tab_bar_px - status_bar_px).max(0.0);
                        let action = crate::sidebar::hit_test_sidebar(
                            self.cursor_pixel_pos.0 as f32,
                            self.cursor_pixel_pos.1 as f32,
                            tab_bar_px,
                            sidebar_h,
                            self.tabs.len(),
                        );
                        match action {
                            crate::sidebar::SidebarAction::SelectTab(idx) => {
                                self.select_tab(idx);
                                win.request_redraw();
                            }
                            crate::sidebar::SidebarAction::CloseTab(idx) => {
                                self.close_tab(idx);
                                win.request_redraw();
                            }
                            crate::sidebar::SidebarAction::NewSession => {
                                self.create_tab(None);
                                win.request_redraw();
                            }
                            crate::sidebar::SidebarAction::ToggleSidebar => {
                                self.sidebar_open = !self.sidebar_open;
                                self.handle_resize(win.inner_size());
                                self.terminal.mark_all_dirty();
                                win.request_redraw();
                            }
                            crate::sidebar::SidebarAction::SelectWorkspace(idx) => {
                                if let Some(ws) = self.sidebar_state.workspaces.get(idx) {
                                    self.workspace_name = ws.name.clone();
                                    self.git_branch = Some(ws.branch.clone());
                                }
                                win.request_redraw();
                            }
                            crate::sidebar::SidebarAction::SelectChat(_idx) => {
                                win.request_redraw();
                            }
                            crate::sidebar::SidebarAction::None => {}
                        }
                    }
                    return;
                }

                // Check changes panel click if changes panel is open
                let changes_panel_px = if self.changes_panel_open {
                    f64::from(
                        (crate::changes_panel::CHANGES_PANEL_WIDTH * self.scale_factor as f32)
                            .round(),
                    )
                } else {
                    0.0
                };
                let win_w_f64 = f64::from(win.inner_size().width);
                let panel_x = win_w_f64 - changes_panel_px;
                if self.changes_panel_open && self.cursor_pixel_pos.0 >= panel_x {
                    if state == winit::event::ElementState::Pressed {
                        let tab_bar_px = (TAB_BAR_HEIGHT * self.scale_factor as f32).round();
                        let status_bar_px = if self.config.status_bar {
                            (crate::status_bar::STATUS_BAR_HEIGHT * self.scale_factor as f32)
                                .round()
                        } else {
                            0.0
                        };
                        let panel_h =
                            (win.inner_size().height as f32 - tab_bar_px - status_bar_px).max(0.0);
                        let action = crate::changes_panel::hit_test_changes_panel(
                            self.cursor_pixel_pos.0 as f32,
                            self.cursor_pixel_pos.1 as f32,
                            panel_x as f32,
                            tab_bar_px,
                            changes_panel_px as f32,
                            panel_h,
                            &self.changes_panel_state,
                        );
                        match action {
                            crate::changes_panel::ChangesAction::SwitchTab(tab) => {
                                self.changes_panel_state.active_tab = tab;
                                win.request_redraw();
                            }
                            crate::changes_panel::ChangesAction::SelectFile(idx) => {
                                self.changes_panel_state.selected_file_index = Some(idx);
                                if let Some(file) = self.changes_panel_state.changed_files.get(idx)
                                {
                                    let rel_path = if file.path_info == "." {
                                        file.filename.clone()
                                    } else {
                                        format!(
                                            "{}/{}",
                                            file.path_info.trim_end_matches('/'),
                                            file.filename
                                        )
                                    };
                                    let cwd = std::env::current_dir()
                                        .unwrap_or_else(|_| std::path::PathBuf::from("."));
                                    let (fname, pinfo, hunks) =
                                        crate::git::query_file_diff(&cwd, &rel_path);
                                    self.diff_view_state.file_name = fname;
                                    self.diff_view_state.path_info = pinfo;
                                    self.diff_view_state.hunks = hunks;
                                    self.diff_view_state.staged = file.staged;
                                    self.diff_view_state.scroll_offset = 0;
                                    self.diff_view_open = true;
                                    self.handle_resize(win.inner_size());
                                    self.terminal.mark_all_dirty();
                                }
                                win.request_redraw();
                            }
                            crate::changes_panel::ChangesAction::ToggleStage(idx) => {
                                let cwd = std::env::current_dir()
                                    .unwrap_or_else(|_| std::path::PathBuf::from("."));
                                if let Some(file) = self.changes_panel_state.changed_files.get(idx)
                                {
                                    let rel_path = if file.path_info == "." {
                                        file.filename.clone()
                                    } else {
                                        format!(
                                            "{}/{}",
                                            file.path_info.trim_end_matches('/'),
                                            file.filename
                                        )
                                    };
                                    if file.staged {
                                        let _ = crate::git::unstage_file(&cwd, &rel_path);
                                    } else {
                                        let _ = crate::git::stage_file(&cwd, &rel_path);
                                    }
                                    self.refresh_git_status();
                                }
                                win.request_redraw();
                            }
                            crate::changes_panel::ChangesAction::StageAll => {
                                let cwd = std::env::current_dir()
                                    .unwrap_or_else(|_| std::path::PathBuf::from("."));
                                let _ = crate::git::stage_all(&cwd);
                                self.refresh_git_status();
                                win.request_redraw();
                            }
                            crate::changes_panel::ChangesAction::Commit => {
                                let cwd = std::env::current_dir()
                                    .unwrap_or_else(|_| std::path::PathBuf::from("."));
                                let msg =
                                    if self.changes_panel_state.commit_message.trim().is_empty() {
                                        "Commit Tracked"
                                    } else {
                                        &self.changes_panel_state.commit_message
                                    };
                                let _ = crate::git::commit_changes(&cwd, msg);
                                self.refresh_git_status();
                                win.request_redraw();
                            }
                            crate::changes_panel::ChangesAction::Fetch => {
                                log::info!("Git fetch triggered");
                                win.request_redraw();
                            }
                            crate::changes_panel::ChangesAction::None => {}
                        }
                    }
                    return;
                }

                // Check diff view click if diff view is open
                let avail_middle_f64 = (win_w_f64 - sidebar_px - changes_panel_px).max(0.0);
                let term_w_f64 = if self.diff_view_open {
                    (avail_middle_f64 * 0.4).clamp(240.0, 520.0)
                } else {
                    avail_middle_f64
                };
                let diff_pane_x = sidebar_px + term_w_f64;
                let diff_pane_width = if self.diff_view_open {
                    (avail_middle_f64 - term_w_f64).max(0.0)
                } else {
                    0.0
                };
                if self.diff_view_open
                    && self.cursor_pixel_pos.0 >= diff_pane_x
                    && self.cursor_pixel_pos.0 < diff_pane_x + diff_pane_width
                {
                    if state == winit::event::ElementState::Pressed {
                        let tab_bar_px = (TAB_BAR_HEIGHT * self.scale_factor as f32).round();
                        let status_bar_px = if self.config.status_bar {
                            (crate::status_bar::STATUS_BAR_HEIGHT * self.scale_factor as f32)
                                .round()
                        } else {
                            0.0
                        };
                        let diff_h =
                            (win.inner_size().height as f32 - tab_bar_px - status_bar_px).max(0.0);
                        let action = crate::diff_view::hit_test_diff_view(
                            self.cursor_pixel_pos.0 as f32,
                            self.cursor_pixel_pos.1 as f32,
                            diff_pane_x as f32,
                            tab_bar_px,
                            diff_pane_width as f32,
                            diff_h,
                        );
                        match action {
                            crate::diff_view::DiffAction::ToggleStage => {
                                self.diff_view_state.staged = !self.diff_view_state.staged;
                                let cwd = std::env::current_dir()
                                    .unwrap_or_else(|_| std::path::PathBuf::from("."));
                                let rel_path = if self.diff_view_state.path_info == "." {
                                    self.diff_view_state.file_name.clone()
                                } else {
                                    format!(
                                        "{}{}",
                                        self.diff_view_state.path_info,
                                        self.diff_view_state.file_name
                                    )
                                };
                                if self.diff_view_state.staged {
                                    let _ = crate::git::stage_file(&cwd, &rel_path);
                                } else {
                                    let _ = crate::git::unstage_file(&cwd, &rel_path);
                                }
                                self.refresh_git_status();
                                win.request_redraw();
                            }
                            crate::diff_view::DiffAction::Close => {
                                self.diff_view_open = false;
                                self.handle_resize(win.inner_size());
                                self.terminal.mark_all_dirty();
                                win.request_redraw();
                            }
                            crate::diff_view::DiffAction::OpenFile => {
                                log::info!("Open file action triggered");
                                win.request_redraw();
                            }
                            crate::diff_view::DiffAction::Restore => {
                                log::info!("Restore file action triggered");
                                win.request_redraw();
                            }
                            crate::diff_view::DiffAction::None => {}
                        }
                    }
                    return;
                }

                // Check bottom status bar and theme popover clicks
                if self.config.status_bar {
                    let win_w = win.inner_size().width as f32;
                    let win_h = win.inner_size().height as f32;
                    let bar_y = win_h
                        - (crate::status_bar::STATUS_BAR_HEIGHT * self.scale_factor as f32).round();
                    let state_sb = crate::status_bar::StatusBarState {
                        theme_name: self.config.theme.name.clone(),
                        theme_menu_open: self.theme_menu_open,
                        ..crate::status_bar::StatusBarState::default()
                    };

                    if self.theme_menu_open || self.cursor_pixel_pos.1 as f32 >= bar_y {
                        if state == winit::event::ElementState::Pressed {
                            let sb_action = crate::status_bar::hit_test_status_bar(
                                self.cursor_pixel_pos.0 as f32,
                                self.cursor_pixel_pos.1 as f32,
                                win_w,
                                win_h,
                                &state_sb,
                            );

                            match sb_action {
                                crate::status_bar::StatusBarAction::ToggleThemeMenu => {
                                    self.theme_menu_open = !self.theme_menu_open;
                                    win.request_redraw();
                                    return;
                                }
                                crate::status_bar::StatusBarAction::SelectTheme(theme_name) => {
                                    self.theme_menu_open = false;
                                    if let Some(new_theme) =
                                        crate::theme::Theme::preset_by_name(&theme_name)
                                    {
                                        log::info!("Status bar switched theme to '{theme_name}'");
                                        self.config.theme = new_theme;
                                        self.terminal.mark_all_dirty();
                                        win.request_redraw();
                                    }
                                    return;
                                }
                                crate::status_bar::StatusBarAction::CloseThemeMenu => {
                                    if self.theme_menu_open {
                                        self.theme_menu_open = false;
                                        win.request_redraw();
                                        return;
                                    }
                                }
                                crate::status_bar::StatusBarAction::None => {}
                            }

                            if self.cursor_pixel_pos.1 as f32 >= bar_y {
                                return;
                            }
                        } else if self.theme_menu_open {
                            return;
                        }
                    }
                }

                let pt = self.pixel_to_grid_point(self.cursor_pixel_pos);
                if state == winit::event::ElementState::Pressed {
                    self.mouse_down_pos = Some(pt);
                    self.selection = None;
                    win.request_redraw();
                } else if state == winit::event::ElementState::Released {
                    self.mouse_down_pos = None;
                    if let Some(sel) = self.selection
                        && sel.start == sel.end
                    {
                        self.selection = None;
                        win.request_redraw();
                    }
                }
            }
            WindowEvent::Focused(focused) => {
                self.has_focus = focused;
                self.cursor_blink_visible = true;
                self.last_blink = std::time::Instant::now();
                win.request_redraw();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let mut wait_duration: Option<Duration> = None;

        if let Some(triggered) = self.bell_triggered_at {
            let elapsed = triggered.elapsed();
            if elapsed >= Self::VISUAL_BELL_DURATION {
                self.bell_triggered_at = None;
                if let Some(ref win) = self.window {
                    win.request_redraw();
                }
            } else {
                let remaining = Self::VISUAL_BELL_DURATION.saturating_sub(elapsed);
                wait_duration = Some(remaining);
            }
        }

        if self.terminal.cursor_blinking && self.has_focus {
            let elapsed = self.last_blink.elapsed();
            if elapsed >= Duration::from_millis(500) {
                self.cursor_blink_visible = !self.cursor_blink_visible;
                self.last_blink = std::time::Instant::now();
                if let Some(ref win) = self.window {
                    win.request_redraw();
                }
            }
            let remaining = Duration::from_millis(500).saturating_sub(self.last_blink.elapsed());
            wait_duration = Some(wait_duration.map_or(remaining, |w| w.min(remaining)));
        }

        if let Some(duration) = wait_duration {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::wait_duration(duration));
        } else {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pty::{PtySize, ShellCommand};

    #[test]
    fn app_event_variants_equality() {
        let ev1 = AppEvent::PtyOutput(b"hello".to_vec());
        let ev2 = AppEvent::PtyOutput(b"hello".to_vec());
        assert_eq!(ev1, ev2);

        let exit_ev = AppEvent::PtyExited(Some(0));
        assert_eq!(exit_ev, AppEvent::PtyExited(Some(0)));
    }

    #[test]
    fn app_initializes_default_state() {
        let app = App::new(None, Terminal::new(80, 24), None);
        assert!(app.window.is_none());
        assert_eq!(app.title, DEFAULT_TITLE);
        assert!(!app.pty_exited);
        assert!(app.pty_exit_code.is_none());
        assert_eq!(app.terminal.grid.cols, 80);
        assert_eq!(app.terminal.grid.rows, 24);
        assert!((app.base_font_size - 14.0).abs() < f32::EPSILON);
        assert!((app.scale_factor - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn scale_factor_change_recalculates_metrics() {
        let mut app = App::new(None, Terminal::new(80, 24), None);
        let orig_width = app.cell_metrics.cell_width;
        let orig_height = app.cell_metrics.cell_height;

        app.handle_scale_factor_changed(2.0);
        assert!((app.scale_factor - 2.0).abs() < f64::EPSILON);
        assert!(app.cell_metrics.cell_width > orig_width);
        assert!(app.cell_metrics.cell_height > orig_height);
    }

    #[test]
    fn zoom_in_out_and_reset_adjusts_font_size() {
        let mut app = App::new(None, Terminal::new(80, 24), None);
        assert!((app.base_font_size - 14.0).abs() < f32::EPSILON);

        app.zoom_in();
        assert!((app.base_font_size - 15.0).abs() < f32::EPSILON);

        app.zoom_out();
        assert!((app.base_font_size - 14.0).abs() < f32::EPSILON);

        app.zoom_out();
        assert!((app.base_font_size - 13.0).abs() < f32::EPSILON);

        app.zoom_reset();
        assert!((app.base_font_size - 14.0).abs() < f32::EPSILON);
    }

    #[test]
    fn handle_resize_updates_grid_and_pty() {
        let term = Terminal::new(80, 24);
        let mut app = App::new(None, term, None);
        app.sidebar_open = false;
        app.changes_panel_open = false;
        app.diff_view_open = false;
        app.cell_metrics = CellMetrics {
            cell_width: 10.0,
            cell_height: 20.0,
            font_size: 14.0,
            line_height: 20.0,
        };

        app.handle_resize(PhysicalSize::new(1000, 800));

        assert_eq!(app.terminal.grid.cols, 99);
        assert_eq!(app.terminal.grid.rows, 36);
        assert_eq!(app.terminal.alternate_grid.cols, 99);
        assert_eq!(app.terminal.alternate_grid.rows, 36);
        assert!(app.terminal.has_damage());

        // When sidebar is opened: columns adjust to fit narrower width (240px sidebar)
        app.sidebar_open = true;
        app.handle_resize(PhysicalSize::new(1000, 800));
        assert_eq!(app.terminal.grid.cols, 75);

        // When all cockpit panes are opened: terminal fits agent pane
        app.changes_panel_open = true;
        app.diff_view_open = true;
        app.handle_resize(PhysicalSize::new(1440, 900));
        assert!(app.terminal.grid.cols > 30);
    }

    #[test]
    fn handle_resize_propagates_to_pty_writer() {
        let term = Terminal::new(80, 24);
        let cmd = ShellCommand::default_shell();
        let size = PtySize {
            rows: 24,
            cols: 80,
            px_w: 800,
            px_h: 480,
        };
        if let Ok((writer, _reader)) = crate::pty::spawn(cmd, size) {
            let writer = Arc::new(Mutex::new(writer));
            let mut app = App::new(None, term, Some(Arc::clone(&writer)));
            app.sidebar_open = false;
            app.changes_panel_open = false;
            app.diff_view_open = false;
            app.cell_metrics = CellMetrics {
                cell_width: 10.0,
                cell_height: 20.0,
                font_size: 14.0,
                line_height: 20.0,
            };

            app.handle_resize(PhysicalSize::new(1000, 800));
            assert_eq!(app.terminal.grid.cols, 99);
            assert_eq!(app.terminal.grid.rows, 36);
        }
    }

    #[test]
    fn handle_pty_output_advances_parser_and_damages_grid() {
        let term = Terminal::new(80, 24);
        let mut app = App::new(None, term, None);
        app.terminal.clear_damage();
        assert!(!app.terminal.has_damage());

        app.handle_pty_output(b"Hello Oxidite!");

        assert_eq!(app.terminal.grid[0][0].ch, 'H');
        assert_eq!(app.terminal.grid[0][1].ch, 'e');
        assert_eq!(app.terminal.grid[0][6].ch, 'O');
        assert!(app.terminal.has_damage());
    }

    #[test]
    fn handle_pty_output_applies_osc_title() {
        let term = Terminal::new(80, 24);
        let mut app = App::new(None, term, None);
        assert_eq!(app.title, DEFAULT_TITLE);

        app.handle_pty_output(b"\x1b]0;My Custom Title\x07");
        assert_eq!(app.title, "My Custom Title");
        assert_eq!(app.terminal.title, "My Custom Title");

        app.handle_pty_output(b"\x1b]2;Another Terminal Title\x07");
        assert_eq!(app.title, "Another Terminal Title");
        assert_eq!(app.terminal.title, "Another Terminal Title");
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn visual_bell_triggers_on_bel_control_char() {
        let term = Terminal::new(80, 24);
        let mut app = App::new(None, term, None);
        assert_eq!(app.terminal.bell_count, 0);
        assert!(app.bell_triggered_at.is_none());

        // Feeding \x07 (BEL) increments terminal.bell_count and activates visual bell
        app.handle_pty_output(b"\x07");
        assert_eq!(app.terminal.bell_count, 1);
        assert_eq!(app.last_bell_count, 1);
        assert!(app.bell_triggered_at.is_some());

        // Verify that assemble_quad_vertices appends the visual bell quad when bell_active is true
        let metrics = CellMetrics {
            cell_width: 10.0,
            cell_height: 20.0,
            font_size: 14.0,
            line_height: 20.0,
        };
        app.cell_metrics = metrics;
        let quads_no_bell = app.assemble_quad_vertices(false, [800.0, 480.0]);
        let quads_with_bell = app.assemble_quad_vertices(true, [800.0, 480.0]);

        // Quads with bell should have 6 extra vertices (1 extra quad covering 800x480)
        assert_eq!(quads_with_bell.len(), quads_no_bell.len() + 6);
        let bell_vertex = quads_with_bell.last().unwrap();
        assert_eq!(bell_vertex.position, [800.0, 480.0]);
        assert_eq!(bell_vertex.color, [1.0, 1.0, 1.0, 0.25]);
    }

    #[test]
    fn pty_batch_coalescing_combines_available_chunks() {
        let (sender, receiver) = std::sync::mpsc::channel();
        sender.send(b"chunk1 ".to_vec()).unwrap();
        sender.send(b"chunk2 ".to_vec()).unwrap();
        sender.send(b"chunk3".to_vec()).unwrap();

        let mut pty_reader = PtyReader::from_receiver(receiver);

        let mut combined = pty_reader.recv().unwrap();
        while combined.len() < PTY_BATCH_BYTE_LIMIT
            && let Some(more) = pty_reader.try_recv()
        {
            combined.extend_from_slice(&more);
        }

        assert_eq!(combined, b"chunk1 chunk2 chunk3");
    }

    #[test]
    fn theme_changed_updates_app_theme() {
        let mut app = App::new(None, Terminal::new(80, 24), None);
        assert_eq!(app.config.theme.name, "default-dark");
        let dracula = crate::config::load_theme_by_name("dracula").expect("dracula theme");
        app.config.theme = dracula;
        assert_eq!(app.config.theme.name, "dracula");
    }

    #[test]
    fn multi_tab_creation_selection_and_closing() {
        let mut app = App::new(None, Terminal::new(80, 24), None);
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.active_tab_index, 0);

        // Create Tab 2
        app.create_tab(Some("Claude Tab".to_string()));
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active_tab_index, 1);
        assert_eq!(app.title, "Claude Tab");

        // Create Tab 3
        app.create_tab(Some("Codex Tab".to_string()));
        assert_eq!(app.tabs.len(), 3);
        assert_eq!(app.active_tab_index, 2);
        assert_eq!(app.title, "Codex Tab");

        // Switch to Tab 0
        app.select_tab(0);
        assert_eq!(app.active_tab_index, 0);
        assert_eq!(app.title, "Tab 1");

        // Close Tab 1 (Claude Tab)
        app.close_tab(1);
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active_tab_index, 0);
        assert_eq!(app.tabs[1].title, "Codex Tab");

        // Verify closing single remaining tab does not leave 0 tabs
        app.close_tab(1);
        assert_eq!(app.tabs.len(), 1);
        app.close_tab(0);
        assert_eq!(app.tabs.len(), 1);
    }

    #[test]
    fn multi_tab_pty_output_routes_to_correct_tab() {
        let mut app = App::new(None, Terminal::new(80, 24), None);
        app.create_tab(Some("Second".to_string()));

        // Tab 1 is active, Tab 0 is in background
        assert_eq!(app.active_tab_index, 1);

        // Feed output to Tab 0 (background)
        app.handle_tab_pty_output(0, b"Hello from Tab 0");
        assert_eq!(app.tabs[0].terminal.grid[0][0].ch, 'H');
        // Active terminal (Tab 1) is not affected yet
        assert_ne!(app.terminal.grid[0][0].ch, 'H');

        // Switch to Tab 0: active terminal now receives Tab 0's grid
        app.select_tab(0);
        assert_eq!(app.terminal.grid[0][0].ch, 'H');
    }

    #[test]
    fn build_tab_labels_generates_expected_labels() {
        let mut app = App::new(None, Terminal::new(80, 24), None);
        app.create_tab(Some("Workspaces".to_string()));

        let labels = app.build_tab_labels(800.0);
        // 1 sidebar button + 2 tabs * 2 labels (title + close) + 1 plus button + 2 chrome buttons = 8 labels
        assert_eq!(labels.len(), 8);
        assert_eq!(labels[0].text, "◨");
        assert_eq!(labels[1].text, "⌁ Tab 1");
        assert_eq!(labels[2].text, "×");
        assert_eq!(labels[3].text, "❯ Workspaces");
        assert_eq!(labels[4].text, "×");
        assert_eq!(labels[5].text, "+");
        assert_eq!(labels[6].text, "◫");
        assert_eq!(labels[7].text, "⤢");

        // Verify agent detection badge formatting
        app.create_tab(Some("claude-code".to_string()));
        let agent_labels = app.build_tab_labels(800.0);
        assert!(agent_labels.iter().any(|l| l.text == "✳ claude"));
    }

    #[test]
    fn pixel_to_grid_point_accounts_for_padding_and_tab_bar() {
        let mut app = App::new(None, Terminal::new(80, 24), None);
        app.sidebar_open = false;
        app.cell_metrics = CellMetrics {
            cell_width: 10.0,
            cell_height: 20.0,
            font_size: 14.0,
            line_height: 20.0,
        };
        app.config.padding_x = 10.0;
        app.config.padding_y = 10.0;

        // Exactly at (pad_x, TAB_BAR_HEIGHT + pad_y) -> (col 0, row 0)
        let pt0 = app.pixel_to_grid_point((10.0, 36.0 + 10.0));
        assert_eq!(pt0.col, 0);
        assert_eq!(pt0.row, 0);

        // One cell over in X and Y
        let pt1 = app.pixel_to_grid_point((20.0, 36.0 + 30.0));
        assert_eq!(pt1.col, 1);
        assert_eq!(pt1.row, 1);
    }

    #[test]
    fn build_status_bar_labels_produces_valid_chrome() {
        let mut app = App::new(None, Terminal::new(80, 24), None);
        app.git_branch = None;
        let labels = app.build_status_bar_labels(800.0, 600.0);
        assert_eq!(labels.len(), 3);
        assert_eq!(labels[0].text, "NORMAL");
        assert!(labels[1].text.starts_with("⌁"));
        assert!(labels[2].text.contains("Ln 1, Col 1"));

        app.git_branch = Some("main".to_string());
        let labels_with_git = app.build_status_bar_labels(800.0, 600.0);
        assert_eq!(labels_with_git.len(), 4);
        assert_eq!(labels_with_git[2].text, " main");
    }

    #[test]
    fn scrollbar_quads_rendered_only_when_scrolled_into_history() {
        let mut app = App::new(None, Terminal::new(80, 24), None);
        let screen_size = [800.0, 600.0];
        let accent = [0.0, 1.0, 0.5, 1.0];

        // 1. Initial state: display_offset == 0 -> no scrollbar quads
        let quads_idle = app.assemble_scrollbar_quads(screen_size, 36.0, 10.0, accent);
        assert_eq!(quads_idle.len(), 0);

        // 2. Add lines so scrollback is populated and scroll viewport back
        app.handle_tab_pty_output(
            0,
            b"l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nl10\nl11\nl12\nl13\nl14\nl15\nl16\nl17\nl18\nl19\nl20\nl21\nl22\nl23\nl24\nl25\nl26\nl27\nl28\nl29\nl30\n",
        );
        app.terminal.grid.scroll_display(5);
        assert!(app.terminal.grid.display_offset > 0);
        assert!(!app.terminal.grid.scrollback.is_empty());

        // 3. Scrolled state: generates track quad (6 vertices) + thumb quad (6 vertices) = 12 vertices
        let quads_scrolled = app.assemble_scrollbar_quads(screen_size, 36.0, 10.0, accent);
        assert_eq!(quads_scrolled.len(), 12);
    }
}
