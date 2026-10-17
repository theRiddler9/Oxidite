//! Application event loop wiring and state management.

use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use crate::input::{GridPoint, InputAction, KeyEvent, Selection, check_input_action, encode_key};
use crate::parser::Parser;
use crate::pty::{PtyReader, PtyWriter};
use crate::renderer::draw::{
    DEFAULT_PALETTE, DrawContext, GlyphPipeline, QuadPipeline, build_cell_background_vertices,
    build_cursor_vertices, build_quad_vertices,
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

/// Spawns a background thread that forwards PTY reader output chunks to the event loop.
pub fn spawn_pty_forwarder(
    mut reader: PtyReader,
    writer: Arc<Mutex<PtyWriter>>,
    proxy: EventLoopProxy<AppEvent>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("pty-event-forwarder".to_string())
        .spawn(move || {
            while let Some(bytes) = reader.recv() {
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
}

impl App {
    /// Creates a new `App` instance with the given event loop proxy, terminal, and optional PTY writer.
    pub fn new(
        proxy: Option<EventLoopProxy<AppEvent>>,
        terminal: Terminal,
        pty_writer: Option<Arc<Mutex<PtyWriter>>>,
    ) -> Self {
        let mut font_system = glyphon::FontSystem::new();
        let cell_metrics = CellMetrics::measure(&mut font_system, 14.0, 1.2);

        Self {
            proxy,
            window: None,
            title: DEFAULT_TITLE.to_string(),
            pty_exited: false,
            pty_exit_code: None,
            terminal,
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

    #[allow(clippy::cast_precision_loss)]
    fn build_selection_vertices(
        grid: &crate::grid::Grid,
        selection: Option<&Selection>,
        metrics: &CellMetrics,
    ) -> Vec<crate::renderer::draw::QuadVertex> {
        let mut verts = Vec::new();
        let Some(sel) = selection else {
            return verts;
        };
        for row in 0..grid.rows {
            let Some(row_cells) = grid.display_row(row) else {
                continue;
            };
            for (col, cell) in row_cells.iter().enumerate() {
                if sel.contains(row, col) {
                    let x = col as f32 * metrics.cell_width;
                    let y = row as f32 * metrics.cell_height;
                    let w = f32::from(cell.width.max(1)) * metrics.cell_width;
                    let h = metrics.cell_height;
                    let sel_color = [0.2, 0.4, 0.8, 0.4];
                    verts.extend_from_slice(&build_quad_vertices(x, y, w, h, sel_color));
                }
            }
        }
        verts
    }

    fn handle_keyboard_input(&mut self, key_ev: &winit::event::KeyEvent, win: &Window) {
        let key_event = KeyEvent::new(
            key_ev.logical_key.clone(),
            key_ev.text.as_ref().map(ToString::to_string),
            self.modifiers,
            key_ev.state,
        );

        if let Some(action) = check_input_action(&key_event) {
            match action {
                InputAction::ScrollUp(lines) => {
                    let delta = isize::try_from(lines).unwrap_or(12);
                    self.terminal.grid.scroll_display(delta);
                    win.request_redraw();
                }
                InputAction::ScrollDown(lines) => {
                    let delta = isize::try_from(lines).unwrap_or(12);
                    self.terminal.grid.scroll_display(-delta);
                    win.request_redraw();
                }
                InputAction::ScrollToTop => {
                    self.terminal.grid.scroll_to_top();
                    win.request_redraw();
                }
                InputAction::ScrollToBottom => {
                    self.terminal.grid.scroll_to_bottom();
                    win.request_redraw();
                }
            }
            return;
        }

        if let Some(bytes) = encode_key(&key_event, &self.terminal.modes) {
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
            win.request_redraw();
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
        if self.terminal.has_damage()
            && let Some(ref win) = self.window
        {
            win.request_redraw();
        }
    }

    /// Renders the terminal grid, cell backgrounds, glyphs, and cursor to the window surface.
    #[allow(clippy::cast_precision_loss)]
    pub fn render(&mut self) {
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

        let mut quad_vertices = build_cell_background_vertices(
            &self.terminal.grid,
            self.cell_metrics.cell_width,
            self.cell_metrics.cell_height,
            &DEFAULT_PALETTE,
        );
        if self.terminal.grid.display_offset == 0 {
            let cursor_vertices = build_cursor_vertices(
                &self.terminal.grid.cursor,
                &self.cell_metrics,
                [0.8, 0.8, 0.8, 0.8],
            );
            quad_vertices.extend_from_slice(&cursor_vertices);
        }

        quad_vertices.extend_from_slice(&Self::build_selection_vertices(
            &self.terminal.grid,
            self.selection.as_ref(),
            &self.cell_metrics,
        ));

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("terminal-render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.05,
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
                palette: &DEFAULT_PALETTE,
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
                    let size = win.inner_size();
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
                    self.handle_resize(size);
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
                if let Some(ref win) = self.window {
                    win.request_redraw();
                }
            }
            AppEvent::ConfigReloaded | AppEvent::ThemeChanged(_) => {}
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
            _ => {}
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
}
