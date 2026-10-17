//! Keyboard / mouse / paste encoding to PTY bytes.

use winit::event::ElementState;
use winit::keyboard::{Key, ModifiersState, NamedKey};

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

    let alt = event.modifiers.alt_key();

    // 1. Enter, Tab, Backspace, Escape
    match event.logical_key {
        Key::Named(NamedKey::Enter) => {
            return Some(if alt {
                b"\x1b\r".to_vec()
            } else {
                b"\r".to_vec()
            });
        }
        Key::Named(NamedKey::Tab) => {
            if event.modifiers.shift_key() {
                return Some(b"\x1b[Z".to_vec());
            }
            return Some(if alt {
                b"\x1b\t".to_vec()
            } else {
                b"\t".to_vec()
            });
        }
        Key::Named(NamedKey::Backspace) => {
            return Some(if alt {
                b"\x1b\x7f".to_vec()
            } else {
                b"\x7f".to_vec()
            });
        }
        Key::Named(NamedKey::Escape) => {
            return Some(if alt {
                b"\x1b\x1b".to_vec()
            } else {
                b"\x1b".to_vec()
            });
        }
        _ => {}
    }

    // 2. Ctrl + letter / symbol combinations
    if event.modifiers.control_key()
        && let Key::Character(ref s) = event.logical_key
        && let Some(ch) = s.chars().next()
        && let Some(ctrl_byte) = encode_ctrl_char(ch)
    {
        return Some(if alt {
            vec![0x1b, ctrl_byte]
        } else {
            vec![ctrl_byte]
        });
    }

    // 3. Printable text without Ctrl/Alt
    if !event.modifiers.control_key() && !alt {
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

/// Translates a character combined with the Control modifier into its ASCII control code.
fn encode_ctrl_char(ch: char) -> Option<u8> {
    match ch {
        'a'..='z' | 'A'..='Z' => Some((ch.to_ascii_uppercase() as u8) - 0x40),
        '@' | ' ' => Some(0x00),
        '[' => Some(0x1B),
        '\\' => Some(0x1C),
        ']' => Some(0x1D),
        '^' | '6' => Some(0x1E),
        '_' | '/' => Some(0x1F),
        '?' => Some(0x7F),
        _ => None,
    }
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

    #[test]
    fn encode_enter_tab_backspace_escape() {
        let modes = Modes::default();

        // Enter -> \r
        let ev_enter = KeyEvent::simple(Key::Named(NamedKey::Enter), None);
        assert_eq!(encode_key(&ev_enter, &modes), Some(b"\r".to_vec()));

        // Tab -> \t
        let ev_tab = KeyEvent::simple(Key::Named(NamedKey::Tab), None);
        assert_eq!(encode_key(&ev_tab, &modes), Some(b"\t".to_vec()));

        // Shift+Tab -> \x1b[Z
        let ev_backtab = KeyEvent::new(
            Key::Named(NamedKey::Tab),
            None,
            ModifiersState::SHIFT,
            ElementState::Pressed,
        );
        assert_eq!(encode_key(&ev_backtab, &modes), Some(b"\x1b[Z".to_vec()));

        // Backspace -> \x7f
        let ev_bs = KeyEvent::simple(Key::Named(NamedKey::Backspace), None);
        assert_eq!(encode_key(&ev_bs, &modes), Some(b"\x7f".to_vec()));

        // Escape -> \x1b
        let ev_esc = KeyEvent::simple(Key::Named(NamedKey::Escape), None);
        assert_eq!(encode_key(&ev_esc, &modes), Some(b"\x1b".to_vec()));

        // Alt+Enter -> \x1b\r
        let ev_alt_enter = KeyEvent::new(
            Key::Named(NamedKey::Enter),
            None,
            ModifiersState::ALT,
            ElementState::Pressed,
        );
        assert_eq!(encode_key(&ev_alt_enter, &modes), Some(b"\x1b\r".to_vec()));
    }

    #[test]
    fn encode_ctrl_combinations() {
        let modes = Modes::default();

        // Ctrl+C -> 0x03
        let ev_ctrl_c = KeyEvent::new(
            Key::Character(SmolStr::new("c")),
            None,
            ModifiersState::CONTROL,
            ElementState::Pressed,
        );
        assert_eq!(encode_key(&ev_ctrl_c, &modes), Some(vec![0x03]));

        // Ctrl+A -> 0x01
        let ev_ctrl_a = KeyEvent::new(
            Key::Character(SmolStr::new("a")),
            None,
            ModifiersState::CONTROL,
            ElementState::Pressed,
        );
        assert_eq!(encode_key(&ev_ctrl_a, &modes), Some(vec![0x01]));

        // Ctrl+Z -> 0x1A
        let ev_ctrl_z = KeyEvent::new(
            Key::Character(SmolStr::new("z")),
            None,
            ModifiersState::CONTROL,
            ElementState::Pressed,
        );
        assert_eq!(encode_key(&ev_ctrl_z, &modes), Some(vec![0x1A]));

        // Ctrl+Alt+C -> \x1b\x03
        let ev_ctrl_alt_c = KeyEvent::new(
            Key::Character(SmolStr::new("c")),
            None,
            ModifiersState::CONTROL | ModifiersState::ALT,
            ElementState::Pressed,
        );
        assert_eq!(encode_key(&ev_ctrl_alt_c, &modes), Some(vec![0x1B, 0x03]));
    }
}
