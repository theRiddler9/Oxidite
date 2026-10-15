//! Application event loop wiring (stub — Phase 4).

/// Cross-thread events delivered via `winit`'s `EventLoopProxy`.
#[derive(Debug)]
pub enum AppEvent {
    PtyOutput(Vec<u8>),
    PtyExited(Option<i32>),
    ConfigReloaded,
    ThemeChanged(String),
}
