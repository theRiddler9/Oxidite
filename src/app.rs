//! Application event loop wiring (stub — Phase 4).

/// Cross-thread events delivered via `winit`'s `EventLoopProxy`.
#[derive(Debug)]
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
