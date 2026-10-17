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
pub fn encode_key(event: &KeyEvent, modes: &Modes) -> Option<Vec<u8>> {
    if event.state == ElementState::Released {
        return None;
    }

    let alt = event.modifiers.alt_key();

    // 1. Named keys (Enter, Tab, Backspace, Escape, arrows, navigation)
    if let Key::Named(named) = event.logical_key
        && let Some(bytes) = encode_named_key(named, event.modifiers, modes)
    {
        return Some(bytes);
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

fn encode_named_key(named: NamedKey, modifiers: ModifiersState, modes: &Modes) -> Option<Vec<u8>> {
    let alt = modifiers.alt_key();
    match named {
        NamedKey::Enter => Some(if alt {
            b"\x1b\r".to_vec()
        } else {
            b"\r".to_vec()
        }),
        NamedKey::Tab => {
            if modifiers.shift_key() {
                Some(b"\x1b[Z".to_vec())
            } else if alt {
                Some(b"\x1b\t".to_vec())
            } else {
                Some(b"\t".to_vec())
            }
        }
        NamedKey::Backspace => Some(if alt {
            b"\x1b\x7f".to_vec()
        } else {
            b"\x7f".to_vec()
        }),
        NamedKey::Escape => Some(if alt {
            b"\x1b\x1b".to_vec()
        } else {
            b"\x1b".to_vec()
        }),
        NamedKey::ArrowUp => Some(if modes.application_cursor_keys {
            b"\x1bOA".to_vec()
        } else {
            b"\x1b[A".to_vec()
        }),
        NamedKey::ArrowDown => Some(if modes.application_cursor_keys {
            b"\x1bOB".to_vec()
        } else {
            b"\x1b[B".to_vec()
        }),
        NamedKey::ArrowRight => Some(if modes.application_cursor_keys {
            b"\x1bOC".to_vec()
        } else {
            b"\x1b[C".to_vec()
        }),
        NamedKey::ArrowLeft => Some(if modes.application_cursor_keys {
            b"\x1bOD".to_vec()
        } else {
            b"\x1b[D".to_vec()
        }),
        NamedKey::Home => Some(if modes.application_cursor_keys {
            b"\x1bOH".to_vec()
        } else {
            b"\x1b[H".to_vec()
        }),
        NamedKey::End => Some(if modes.application_cursor_keys {
            b"\x1bOF".to_vec()
        } else {
            b"\x1b[F".to_vec()
        }),
        NamedKey::PageUp => Some(b"\x1b[5~".to_vec()),
        NamedKey::PageDown => Some(b"\x1b[6~".to_vec()),
        NamedKey::Insert => Some(b"\x1b[2~".to_vec()),
        NamedKey::Delete => Some(b"\x1b[3~".to_vec()),
        other => encode_function_key(other).map(<[u8]>::to_vec),
    }
}

/// Encodes standard VT220 / xterm function keys F1..F12.
fn encode_function_key(named: NamedKey) -> Option<&'static [u8]> {
    match named {
        NamedKey::F1 => Some(b"\x1bOP"),
        NamedKey::F2 => Some(b"\x1bOQ"),
        NamedKey::F3 => Some(b"\x1bOR"),
        NamedKey::F4 => Some(b"\x1bOS"),
        NamedKey::F5 => Some(b"\x1b[15~"),
        NamedKey::F6 => Some(b"\x1b[17~"),
        NamedKey::F7 => Some(b"\x1b[18~"),
        NamedKey::F8 => Some(b"\x1b[19~"),
        NamedKey::F9 => Some(b"\x1b[20~"),
        NamedKey::F10 => Some(b"\x1b[21~"),
        NamedKey::F11 => Some(b"\x1b[23~"),
        NamedKey::F12 => Some(b"\x1b[24~"),
        _ => None,
    }
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

    #[test]
    fn encode_arrows_home_end_page_navigation() {
        let modes = Modes::default();

        let up = KeyEvent::simple(Key::Named(NamedKey::ArrowUp), None);
        assert_eq!(encode_key(&up, &modes), Some(b"\x1b[A".to_vec()));

        let down = KeyEvent::simple(Key::Named(NamedKey::ArrowDown), None);
        assert_eq!(encode_key(&down, &modes), Some(b"\x1b[B".to_vec()));

        let right = KeyEvent::simple(Key::Named(NamedKey::ArrowRight), None);
        assert_eq!(encode_key(&right, &modes), Some(b"\x1b[C".to_vec()));

        let left = KeyEvent::simple(Key::Named(NamedKey::ArrowLeft), None);
        assert_eq!(encode_key(&left, &modes), Some(b"\x1b[D".to_vec()));

        let home = KeyEvent::simple(Key::Named(NamedKey::Home), None);
        assert_eq!(encode_key(&home, &modes), Some(b"\x1b[H".to_vec()));

        let end = KeyEvent::simple(Key::Named(NamedKey::End), None);
        assert_eq!(encode_key(&end, &modes), Some(b"\x1b[F".to_vec()));

        let page_up = KeyEvent::simple(Key::Named(NamedKey::PageUp), None);
        assert_eq!(encode_key(&page_up, &modes), Some(b"\x1b[5~".to_vec()));

        let page_down = KeyEvent::simple(Key::Named(NamedKey::PageDown), None);
        assert_eq!(encode_key(&page_down, &modes), Some(b"\x1b[6~".to_vec()));
    }

    #[test]
    fn honor_application_cursor_keys_mode() {
        let modes = Modes {
            application_cursor_keys: true,
            ..Default::default()
        };

        let up = KeyEvent::simple(Key::Named(NamedKey::ArrowUp), None);
        assert_eq!(encode_key(&up, &modes), Some(b"\x1bOA".to_vec()));

        let down = KeyEvent::simple(Key::Named(NamedKey::ArrowDown), None);
        assert_eq!(encode_key(&down, &modes), Some(b"\x1bOB".to_vec()));

        let right = KeyEvent::simple(Key::Named(NamedKey::ArrowRight), None);
        assert_eq!(encode_key(&right, &modes), Some(b"\x1bOC".to_vec()));

        let left = KeyEvent::simple(Key::Named(NamedKey::ArrowLeft), None);
        assert_eq!(encode_key(&left, &modes), Some(b"\x1bOD".to_vec()));

        let home = KeyEvent::simple(Key::Named(NamedKey::Home), None);
        assert_eq!(encode_key(&home, &modes), Some(b"\x1bOH".to_vec()));

        let end = KeyEvent::simple(Key::Named(NamedKey::End), None);
        assert_eq!(encode_key(&end, &modes), Some(b"\x1bOF".to_vec()));
    }

    #[test]
    fn encode_function_keys_f1_to_f12() {
        let modes = Modes::default();
        let expected = [
            (NamedKey::F1, b"\x1bOP".as_slice()),
            (NamedKey::F2, b"\x1bOQ".as_slice()),
            (NamedKey::F3, b"\x1bOR".as_slice()),
            (NamedKey::F4, b"\x1bOS".as_slice()),
            (NamedKey::F5, b"\x1b[15~".as_slice()),
            (NamedKey::F6, b"\x1b[17~".as_slice()),
            (NamedKey::F7, b"\x1b[18~".as_slice()),
            (NamedKey::F8, b"\x1b[19~".as_slice()),
            (NamedKey::F9, b"\x1b[20~".as_slice()),
            (NamedKey::F10, b"\x1b[21~".as_slice()),
            (NamedKey::F11, b"\x1b[23~".as_slice()),
            (NamedKey::F12, b"\x1b[24~".as_slice()),
        ];

        for (key, exp) in expected {
            let ev = KeyEvent::simple(Key::Named(key), None);
            assert_eq!(encode_key(&ev, &modes), Some(exp.to_vec()));
        }
    }
}
