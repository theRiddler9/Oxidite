//! Keyboard / mouse / paste encoding to PTY bytes.

use winit::event::ElementState;
use winit::keyboard::{Key, ModifiersState};

use crate::terminal::Modes;

/// High-level key event representing keyboard activity from the windowing layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    /// Logical key produced by keyboard layout mapping.
    pub logical_key: Key,
    /// Optional UTF-8 text representation produced by the key.
    pub text: Option<String>,
    /// Active modifier key states (Shift, Ctrl, Alt, Super).
    pub modifiers: ModifiersState,
    /// Key state (Pressed or Released).
    pub state: ElementState,
}

impl KeyEvent {
    /// Constructs a pressed key event with no modifiers.
    #[must_use]
    pub fn simple(logical_key: Key, text: Option<String>) -> Self {
        Self {
            logical_key,
            text,
            modifiers: ModifiersState::empty(),
            state: ElementState::Pressed,
        }
    }

    /// Constructs a key event with specified modifiers and element state.
    #[must_use]
    pub fn new(
        logical_key: Key,
        text: Option<String>,
        modifiers: ModifiersState,
        state: ElementState,
    ) -> Self {
        Self {
            logical_key,
            text,
            modifiers,
            state,
        }
    }
}

/// Encodes a key event into bytes for the PTY, or `None` if unhandled.
#[must_use]
pub fn encode_key(event: &KeyEvent, _modes: &Modes) -> Option<Vec<u8>> {
    if event.state == ElementState::Released {
        return None;
    }

    // 1. Printable text without Ctrl/Alt
    if !event.modifiers.control_key() && !event.modifiers.alt_key() {
        if let Some(ref text) = event.text {
            if !text.is_empty() {
                return Some(text.as_bytes().to_vec());
            }
        } else if let Key::Character(ref ch) = event.logical_key {
            return Some(ch.as_bytes().to_vec());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::SmolStr;

    #[test]
    fn encode_printable_ascii_and_unicode() {
        let modes = Modes::default();

        // Printable ASCII 'a'
        let ev_a = KeyEvent::simple(Key::Character(SmolStr::new("a")), Some("a".to_string()));
        assert_eq!(encode_key(&ev_a, &modes), Some(b"a".to_vec()));

        // Printable UTF-8 / CJK '🦀'
        let ev_crab = KeyEvent::simple(Key::Character(SmolStr::new("🦀")), Some("🦀".to_string()));
        assert_eq!(encode_key(&ev_crab, &modes), Some("🦀".as_bytes().to_vec()));

        // Released key produces None
        let ev_released = KeyEvent::new(
            Key::Character(SmolStr::new("x")),
            Some("x".to_string()),
            ModifiersState::empty(),
            ElementState::Released,
        );
        assert_eq!(encode_key(&ev_released, &modes), None);
    }
}
