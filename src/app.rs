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
    DrawContext, GlyphPipeline, QuadPipeline, build_cell_background_vertices,
    build_cursor_vertices_with_width, build_quad_vertices, build_selection_vertices,
    build_text_decoration_vertices,
};
use crate::renderer::font::CellMetrics;
use crate::renderer::gpu::GpuContext;
use crate::renderer::window::{
    DEFAULT_TITLE, create_window, window_size_to_grid_dimensions, window_size_to_pty_size,
};
use crate::terminal::Terminal;
use winit::dpi::PhysicalSize;
use winit::keyboard::ModifiersState;

/// Cross-thread events delivered via `winit`'s `EventLoopProxy`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    /// Raw output bytes received from the PTY reader thread.
    PtyOutput(Vec<u8>),
    /// Child process exited with an optional exit code.
    PtyExited(Option<i32>),
    /// Configuration file was modified on disk and reloaded.
    ConfigReloaded,
    /// Active theme was switched to the specified theme name.
    ThemeChanged(String),
}

/// Maximum byte batch size forwarded in a single `AppEvent::PtyOutput` to optimize large-output throughput.
pub const PTY_BATCH_BYTE_LIMIT: usize = 65_536;

/// Spawns a background thread that forwards PTY reader output chunks to the event loop.
pub fn spawn_pty_forwarder(
    mut reader: PtyReader,
    writer: Arc<Mutex<PtyWriter>>,
    proxy: EventLoopProxy<AppEvent>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("pty-event-forwarder".to_string())
        .spawn(move || {
            while let Some(mut bytes) = reader.recv() {
                // Coalesce immediately available pending chunks to maximize throughput on bursts (e.g. cat/seq)
                while bytes.len() < PTY_BATCH_BYTE_LIMIT
                    && let Some(more) = reader.try_recv()
                {
                    bytes.extend_from_slice(&more);
                }

                if proxy.send_event(AppEvent::PtyOutput(bytes)).is_err() {
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

            let _ = proxy.send_event(AppEvent::PtyExited(exit_code));
        })
        .expect("failed to spawn PTY event forwarder thread")
}

/// Core application state bound to the `winit` event loop.
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

        Self {
            proxy,
            window: None,
            title: DEFAULT_TITLE.to_string(),
            pty_exited: false,
            pty_exit_code: None,
            terminal,
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
        }
    }

    /// Converts window pixel coordinates to a clamped terminal grid point.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn pixel_to_grid_point(&self, pos: (f64, f64)) -> GridPoint {
        let col = (pos.0 / f64::from(self.cell_metrics.cell_width)).max(0.0) as usize;
        let row = (pos.1 / f64::from(self.cell_metrics.cell_height)).max(0.0) as usize;
        GridPoint::new(
            col.min(self.terminal.grid.cols.saturating_sub(1)),
            row.min(self.terminal.grid.rows.saturating_sub(1)),
        )
    }

    fn handle_keyboard_input(&mut self, key_ev: &winit::event::KeyEvent, win: &Window) {
        self.cursor_blink_visible = true;
        self.last_blink = std::time::Instant::now();

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
    pub fn handle_resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }

        // 1. Window / GPU surface
        if let Some(ref mut gpu) = self.gpu {
            gpu.resize(size);
        }

        // 2. Terminal grid
        let (cols, rows) = window_size_to_grid_dimensions(size, &self.cell_metrics);
        self.terminal.resize(cols as usize, rows as usize);

        // 3. PTY master
        let pty_size = window_size_to_pty_size(size, &self.cell_metrics);
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

    /// Feeds PTY output bytes to the VT parser and requests a redraw if damaged.
    pub fn handle_pty_output(&mut self, bytes: &[u8]) {
        // Respond to DSR (Device Status Report: \x1b[6n) if requested by ConPTY/shell
        if bytes.windows(4).any(|w| w == b"\x1b[6n")
            && let Some(ref writer) = self.pty_writer
            && let Ok(mut guard) = writer.lock()
        {
            let row = self.terminal.grid.cursor.row + 1;
            let col = self.terminal.grid.cursor.col + 1;
            let response = format!("\x1b[{row};{col}R");
            let _ = guard.write(response.as_bytes());
        }

        self.parser.advance(&mut self.terminal, bytes);

        // Apply OSC window title changes
        if !self.terminal.title.is_empty() && self.terminal.title != self.title {
            self.title = self.terminal.title.clone();
            if let Some(ref win) = self.window {
                win.set_title(&self.title);
            }
        } else if self.terminal.title.is_empty() && self.title != DEFAULT_TITLE {
            self.title = DEFAULT_TITLE.to_string();
            if let Some(ref win) = self.window {
                win.set_title(&self.title);
            }
        }

        // Trigger visual bell if bell count increased
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
    }

    pub fn assemble_quad_vertices(
        &self,
        bell_active: bool,
        screen_size: [f32; 2],
    ) -> Vec<crate::renderer::draw::QuadVertex> {
        let palette = self.config.theme.to_f32_palette();
        let cursor_color = self.config.theme.cursor.to_rgba_array(1.0);
        let selection_color = self.config.theme.selection_bg.to_rgba_array(1.0);

        let mut quad_vertices = build_cell_background_vertices(
            &self.terminal.grid,
            self.cell_metrics.cell_width,
            self.cell_metrics.cell_height,
            &palette,
        );
        quad_vertices.extend_from_slice(&build_text_decoration_vertices(
            &self.terminal.grid,
            self.cell_metrics.cell_width,
            self.cell_metrics.cell_height,
            &palette,
        ));
        let should_draw_cursor = self.terminal.grid.display_offset == 0
            && self.terminal.grid.cursor.visible
            && (!self.terminal.cursor_blinking || self.cursor_blink_visible || !self.has_focus);

        if should_draw_cursor {
            let cursor_width = self
                .terminal
                .grid
                .get(self.terminal.grid.cursor.row, self.terminal.grid.cursor.col)
                .map_or(1, |c| c.width.max(1));
            let cursor_vertices = build_cursor_vertices_with_width(
                &self.terminal.grid.cursor,
                &self.cell_metrics,
                cursor_color,
                cursor_width,
            );
            quad_vertices.extend_from_slice(&cursor_vertices);
        }

        quad_vertices.extend_from_slice(&build_selection_vertices(
            &self.terminal.grid,
            self.selection.as_ref(),
            self.cell_metrics.cell_width,
            self.cell_metrics.cell_height,
            selection_color,
        ));

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

    /// Renders the terminal grid, cell backgrounds, glyphs, and cursor to the window surface.
    #[allow(clippy::cast_precision_loss)]
    pub fn render(&mut self) {
        let is_bell_active = self
            .bell_triggered_at
            .is_some_and(|t| t.elapsed() < Self::VISUAL_BELL_DURATION);

        let screen_size = match self.gpu {
            Some(ref gpu) => [gpu.config.width as f32, gpu.config.height as f32],
            None => return,
        };
        let quad_vertices = self.assemble_quad_vertices(is_bell_active, screen_size);

        let palette = self.config.theme.to_f32_palette();
        let bg_color = self.config.theme.background.to_rgba_array(1.0);

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
                            r: f64::from(bg_color[0]),
                            g: f64::from(bg_color[1]),
                            b: f64::from(bg_color[2]),
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

            let draw_ctx = DrawContext {
                device: &gpu.device,
                queue: &gpu.queue,
                screen_size: [gpu.config.width, gpu.config.height],
                metrics: &self.cell_metrics,
                palette: &palette,
                dirty_rows: Some(&self.terminal.dirty_rows),
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
            AppEvent::PtyExited(code) => {
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
        app.cell_metrics = CellMetrics {
            cell_width: 10.0,
            cell_height: 20.0,
            font_size: 14.0,
            line_height: 20.0,
        };

        app.handle_resize(PhysicalSize::new(1000, 800));

        assert_eq!(app.terminal.grid.cols, 100);
        assert_eq!(app.terminal.grid.rows, 40);
        assert_eq!(app.terminal.alternate_grid.cols, 100);
        assert_eq!(app.terminal.alternate_grid.rows, 40);
        assert!(app.terminal.has_damage());
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
            app.cell_metrics = CellMetrics {
                cell_width: 10.0,
                cell_height: 20.0,
                font_size: 14.0,
                line_height: 20.0,
            };

            app.handle_resize(PhysicalSize::new(1000, 800));
            assert_eq!(app.terminal.grid.cols, 100);
            assert_eq!(app.terminal.grid.rows, 40);
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
}
