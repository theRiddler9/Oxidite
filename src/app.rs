//! Application event loop wiring and state management.

use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use crate::pty::{PtyReader, PtyWriter};
use crate::renderer::draw::{GlyphPipeline, QuadPipeline};
use crate::renderer::font::CellMetrics;
use crate::renderer::gpu::GpuContext;
use crate::renderer::window::{
    DEFAULT_TITLE, create_window, window_size_to_grid_dimensions, window_size_to_pty_size,
};
use crate::terminal::Terminal;
use winit::dpi::PhysicalSize;

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

            // Reader channel closed; fetch child exit code if possible
            let exit_code = if let Ok(mut guard) = writer.lock() {
                guard.wait().ok().and_then(|c| i32::try_from(c).ok())
            } else {
                None
            };

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
                    self.window = Some(win);
                    self.handle_resize(size);
                }
                Err(err) => eprintln!("Failed to create window: {err}"),
            }
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::PtyOutput(_bytes) => {
                // In Commit 11, fed into parser and requests redraw
            }
            AppEvent::PtyExited(code) => {
                self.pty_exited = true;
                self.pty_exit_code = code;
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
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pty::{PtySize, ShellCommand};
    use crate::renderer::window::create_event_loop;

    #[test]
    fn app_event_variants_equality() {
        let ev1 = AppEvent::PtyOutput(b"hello".to_vec());
        let ev2 = AppEvent::PtyOutput(b"hello".to_vec());
        assert_eq!(ev1, ev2);

        let exit_ev = AppEvent::PtyExited(Some(0));
        assert_eq!(exit_ev, AppEvent::PtyExited(Some(0)));
    }

    #[test]
    fn app_initializes_with_proxy() {
        let el = create_event_loop::<AppEvent>().expect("event loop should build");
        let proxy = el.create_proxy();
        let app = App::new(Some(proxy), Terminal::new(80, 24), None);
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
}
