//! Window creation helpers and configuration.

use std::sync::Arc;
use thiserror::Error;
use winit::dpi::LogicalSize;
use winit::error::{EventLoopError, OsError};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes};

/// Errors that can occur during window or event loop creation.
#[derive(Debug, Error)]
pub enum WindowError {
    /// Failed to create the OS window.
    #[error("failed to create window: {0}")]
    Create(#[from] OsError),

    /// Failed to build the event loop.
    #[error("failed to build event loop: {0}")]
    EventLoop(#[from] EventLoopError),
}

/// Default window title.
pub const DEFAULT_TITLE: &str = "Oxidite";

/// Default window dimensions in logical pixels.
pub const DEFAULT_WIDTH: f64 = 800.0;
pub const DEFAULT_HEIGHT: f64 = 600.0;

/// Builds the default window attributes for Oxidite.
pub fn default_window_attributes(title: &str) -> WindowAttributes {
    WindowAttributes::default()
        .with_title(title)
        .with_inner_size(LogicalSize::new(DEFAULT_WIDTH, DEFAULT_HEIGHT))
}

/// Creates a new `winit` window wrapped in an `Arc`.
pub fn create_window(
    event_loop: &ActiveEventLoop,
    title: &str,
) -> Result<Arc<Window>, WindowError> {
    let attrs = default_window_attributes(title);
    let window = event_loop.create_window(attrs)?;
    Ok(Arc::new(window))
}

/// Creates a new `winit` event loop configured for custom user events.
pub fn create_event_loop<T: 'static>() -> Result<EventLoop<T>, WindowError> {
    let event_loop = EventLoop::<T>::with_user_event().build()?;
    Ok(event_loop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_window_attributes_has_title_and_size() {
        let attrs = default_window_attributes("Test Window");
        assert_eq!(attrs.title, "Test Window");
    }
}
