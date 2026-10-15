//! Keyboard / mouse / paste encoding to PTY bytes (stub — Phase 5).

use crate::terminal::Modes;

/// Placeholder for a window key event until `winit` is wired in Phase 4–5.
#[derive(Debug, Clone)]
pub struct KeyEvent {
    pub text: Option<String>,
}

/// Encode a key event into bytes for the PTY, or `None` if unhandled.
pub fn encode_key(_event: &KeyEvent, _modes: &Modes) -> Option<Vec<u8>> {
    None
}
