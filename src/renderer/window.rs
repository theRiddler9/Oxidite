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
    #[cfg(target_os = "windows")]
    {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let mut builder = EventLoop::<T>::with_user_event();
        builder.with_any_thread(true);
        let event_loop = builder.build()?;
        Ok(event_loop)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let event_loop = EventLoop::<T>::with_user_event().build()?;
        Ok(event_loop)
    }
}

/// Maps a window physical size to terminal grid dimensions (cols, rows).
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn window_size_to_grid_dimensions(
    size: winit::dpi::PhysicalSize<u32>,
    metrics: &crate::renderer::font::CellMetrics,
) -> (u16, u16) {
    if metrics.cell_width <= 0.0 || metrics.cell_height <= 0.0 {
        return (1, 1);
    }

    let cols = ((size.width as f32) / metrics.cell_width).floor().max(1.0) as u16;
    let rows = ((size.height as f32) / metrics.cell_height)
        .floor()
        .max(1.0) as u16;

    (cols, rows)
}

/// Computes the corresponding `PtySize` for a given window size and cell metrics.
#[must_use]
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
pub fn window_size_to_pty_size(
    size: winit::dpi::PhysicalSize<u32>,
    metrics: &crate::renderer::font::CellMetrics,
) -> crate::pty::PtySize {
    let (cols, rows) = window_size_to_grid_dimensions(size, metrics);
    let px_w = u16::try_from(size.width).unwrap_or(u16::MAX);
    let px_h = u16::try_from(size.height).unwrap_or(u16::MAX);

    crate::pty::PtySize {
        rows,
        cols,
        px_w,
        px_h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::font::CellMetrics;
    use winit::dpi::PhysicalSize;

    #[test]
    fn default_window_attributes_has_title_and_size() {
        let attrs = default_window_attributes("Test Window");
        assert_eq!(attrs.title, "Test Window");
    }

    #[test]
    fn window_size_to_grid_dimensions_maps_correctly() {
        let metrics = CellMetrics {
            cell_width: 10.0,
            cell_height: 20.0,
            font_size: 14.0,
            line_height: 20.0,
        };

        let size = PhysicalSize::new(800, 600);
        let (cols, rows) = window_size_to_grid_dimensions(size, &metrics);
        assert_eq!(cols, 80);
        assert_eq!(rows, 30);

        // Very small window clamps to minimum 1x1
        let small_size = PhysicalSize::new(5, 5);
        let (small_cols, small_rows) = window_size_to_grid_dimensions(small_size, &metrics);
        assert_eq!(small_cols, 1);
        assert_eq!(small_rows, 1);
    }

    #[test]
    fn window_size_to_pty_size_preserves_pixel_dimensions() {
        let metrics = CellMetrics {
            cell_width: 8.0,
            cell_height: 16.0,
            font_size: 14.0,
            line_height: 16.0,
        };

        let size = PhysicalSize::new(640, 480);
        let pty_size = window_size_to_pty_size(size, &metrics);
        assert_eq!(pty_size.cols, 80);
        assert_eq!(pty_size.rows, 30);
        assert_eq!(pty_size.px_w, 640);
        assert_eq!(pty_size.px_h, 480);
    }
}
