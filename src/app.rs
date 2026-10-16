//! Application event loop wiring and state management.

use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use crate::pty::{PtyReader, PtyWriter};
use crate::renderer::window::{DEFAULT_TITLE, create_window};

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
    /// Event loop proxy for sending events from background threads.
    pub proxy: EventLoopProxy<AppEvent>,
    /// Managed window instance once resumed.
    pub window: Option<Arc<Window>>,
    /// Current window title.
    pub title: String,
    /// Whether the PTY child process has exited.
    pub pty_exited: bool,
    /// Exit code of the child process if available.
    pub pty_exit_code: Option<i32>,
}

impl App {
    /// Creates a new `App` instance with the given event loop proxy.
    pub fn new(proxy: EventLoopProxy<AppEvent>) -> Self {
        Self {
            proxy,
            window: None,
            title: DEFAULT_TITLE.to_string(),
            pty_exited: false,
            pty_exit_code: None,
        }
    }
}

impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            match create_window(event_loop, &self.title) {
                Ok(win) => self.window = Some(win),
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
        if let Some(ref win) = self.window
            && win.id() == window_id
            && event == WindowEvent::CloseRequested
        {
            event_loop.exit();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let app = App::new(proxy);
        assert!(app.window.is_none());
        assert_eq!(app.title, DEFAULT_TITLE);
        assert!(!app.pty_exited);
        assert!(app.pty_exit_code.is_none());
    }
}
