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

/// Mouse button identifier for terminal reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
    Other(u8),
}

/// Action performed with a mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    Press,
    Release,
    Motion,
}

/// Represents a mouse interaction on the terminal grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MouseEvent {
    pub button: MouseButton,
    pub action: MouseAction,
    /// 0-indexed column in the grid.
    pub col: usize,
    /// 0-indexed row in the grid.
    pub row: usize,
    pub modifiers: ModifiersState,
}

impl MouseEvent {
    /// Constructs a mouse event at the specified grid position.
    #[must_use]
    pub fn new(
        button: MouseButton,
        action: MouseAction,
        col: usize,
        row: usize,
        modifiers: ModifiersState,
    ) -> Self {
        Self {
            button,
            action,
            col,
            row,
            modifiers,
        }
    }
}

/// A point on the terminal grid representing (col, row).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridPoint {
    /// 0-indexed column in the grid.
    pub col: usize,
    /// 0-indexed row in the grid.
    pub row: usize,
}

impl GridPoint {
    /// Creates a new grid point.
    #[must_use]
    pub const fn new(col: usize, row: usize) -> Self {
        Self { col, row }
    }
}

/// Text selection range spanning between two grid coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    /// Selection start anchor.
    pub start: GridPoint,
    /// Selection end point.
    pub end: GridPoint,
}

impl Selection {
    /// Creates a new selection between `start` and `end`.
    #[must_use]
    pub const fn new(start: GridPoint, end: GridPoint) -> Self {
        Self { start, end }
    }

    /// Returns the normalized `(top_left, bottom_right)` endpoints ordered by row and col.
    #[must_use]
    pub fn normalized(&self) -> (GridPoint, GridPoint) {
        if self.start.row < self.end.row
            || (self.start.row == self.end.row && self.start.col <= self.end.col)
        {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }

    /// Checks whether the cell at `(row, col)` is covered by this selection.
    #[must_use]
    pub fn contains(&self, row: usize, col: usize) -> bool {
        let (min, max) = self.normalized();
        if row < min.row || row > max.row {
            return false;
        }
        if min.row == max.row {
            return col >= min.col && col <= max.col;
        }
        if row == min.row {
            col >= min.col
        } else if row == max.row {
            col <= max.col
        } else {
            true
        }
    }

    /// Extracts the text covered by this selection from the given grid.
    #[must_use]
    pub fn extract_text(&self, grid: &crate::grid::Grid) -> String {
        let (min, max) = self.normalized();
        let mut result = String::new();

        for r in min.row..=max.row {
            if r >= grid.rows {
                break;
            }
            let Some(row_cells) = grid.display_row(r) else {
                continue;
            };

            let start_col = if r == min.row { min.col } else { 0 };
            let end_col = if r == max.row {
                max.col.min(row_cells.len().saturating_sub(1))
            } else {
                row_cells.len().saturating_sub(1)
            };

            if start_col <= end_col && start_col < row_cells.len() {
                let mut line = String::new();
                for c in start_col..=end_col {
                    let cell = &row_cells[c];
                    if cell.width != 0 {
                        line.push(cell.ch);
                    }
                }
                let trimmed = line.trim_end_matches(' ');
                result.push_str(trimmed);
            }

            if r < max.row {
                result.push('\n');
            }
        }

        result
    }
}

/// Terminal action triggered by key combinations (such as scrollback navigation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum InputAction {
    /// Scroll viewport up into history by `lines` rows.
    ScrollUp(usize),
    /// Scroll viewport down toward active screen by `lines` rows.
    ScrollDown(usize),
    /// Scroll directly to the oldest row in scrollback.
    ScrollToTop,
    /// Scroll directly to the active prompt.
    ScrollToBottom,
    /// Copy currently selected text to system clipboard.
    Copy,
    /// Paste text from system clipboard to PTY.
    Paste,
}

/// Checks if a key event corresponds to a terminal internal action (e.g. scrollback).
#[must_use]
pub fn check_input_action(event: &KeyEvent) -> Option<InputAction> {
    if event.state != ElementState::Pressed {
        return None;
    }

    let shift = event.modifiers.shift_key();
    let ctrl = event.modifiers.control_key();
    let alt = event.modifiers.alt_key();

    if shift && !ctrl && !alt {
        match event.logical_key {
            Key::Named(NamedKey::PageUp) => return Some(InputAction::ScrollUp(12)),
            Key::Named(NamedKey::PageDown) => return Some(InputAction::ScrollDown(12)),
            Key::Named(NamedKey::Home) => return Some(InputAction::ScrollToTop),
            Key::Named(NamedKey::End) => return Some(InputAction::ScrollToBottom),
            _ => {}
        }
    }

    let is_ctrl_shift = ctrl && shift && !alt;
    let is_super = event.modifiers.super_key() && !ctrl && !alt;
    if (is_ctrl_shift || is_super)
        && let Key::Character(ref ch) = event.logical_key
    {
        if ch.eq_ignore_ascii_case("c") {
            return Some(InputAction::Copy);
        }
        if ch.eq_ignore_ascii_case("v") {
            return Some(InputAction::Paste);
        }
    }

    None
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

    // 3. Characters / printable text (with or without Alt prefix)
    if !event.modifiers.control_key() {
        if alt {
            if let Key::Character(ref ch) = event.logical_key {
                let mut bytes = Vec::with_capacity(ch.len() + 1);
                bytes.push(0x1b);
                bytes.extend_from_slice(ch.as_bytes());
                return Some(bytes);
            }
        } else if let Some(ref text) = event.text {
            if !text.is_empty() {
                return Some(text.as_bytes().to_vec());
            }
        } else if let Key::Character(ref ch) = event.logical_key {
            return Some(ch.as_bytes().to_vec());
        }
    }

    None
}

/// Encodes pasted text into bytes, wrapping in bracketed paste escape sequences if enabled.
#[must_use]
pub fn encode_paste(text: &str, modes: &Modes) -> Vec<u8> {
    if modes.bracketed_paste {
        let mut bytes = Vec::with_capacity(text.len() + 12);
        bytes.extend_from_slice(b"\x1b[200~");
        bytes.extend_from_slice(text.as_bytes());
        bytes.extend_from_slice(b"\x1b[201~");
        bytes
    } else {
        text.as_bytes().to_vec()
    }
}

/// Encodes a mouse event into SGR 1006 escape sequences when mouse reporting is enabled.
#[must_use]
pub fn encode_mouse_event(event: &MouseEvent, modes: &Modes) -> Option<Vec<u8>> {
    if !modes.mouse_reporting {
        return None;
    }

    let mut code: u16 = match event.button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
        MouseButton::WheelUp => 64,
        MouseButton::WheelDown => 65,
        MouseButton::Other(b) => u16::from(b),
    };

    if event.modifiers.shift_key() {
        code += 4;
    }
    if event.modifiers.alt_key() {
        code += 8;
    }
    if event.modifiers.control_key() {
        code += 16;
    }
    if event.action == MouseAction::Motion {
        code += 32;
    }

    let col = event.col.saturating_add(1);
    let row = event.row.saturating_add(1);
    let suffix = if event.action == MouseAction::Release {
        'm'
    } else {
        'M'
    };

    Some(format!("\x1b[<{code};{col};{row}{suffix}").into_bytes())
}

fn encode_named_key(named: NamedKey, modifiers: ModifiersState, modes: &Modes) -> Option<Vec<u8>> {
    let alt = modifiers.alt_key();

    match named {
        NamedKey::Enter => {
            return Some(if alt {
                b"\x1b\r".to_vec()
            } else {
                b"\r".to_vec()
            });
        }
        NamedKey::Tab => {
            if modifiers.shift_key() {
                return Some(b"\x1b[Z".to_vec());
            }
            return Some(if alt {
                b"\x1b\t".to_vec()
            } else {
                b"\t".to_vec()
            });
        }
        NamedKey::Backspace => {
            return Some(if alt {
                b"\x1b\x7f".to_vec()
            } else {
                b"\x7f".to_vec()
            });
        }
        NamedKey::Escape => {
            return Some(if alt {
                b"\x1b\x1b".to_vec()
            } else {
                b"\x1b".to_vec()
            });
        }
        _ => {}
    }

    if let Some(m) = modifier_param(modifiers)
        && let Some(bytes) = encode_modified_key(named, m)
    {
        return Some(bytes);
    }

    match named {
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

/// Calculates the standard xterm modifier parameter: 1 + Shift(1) + Alt(2) + Ctrl(4).
fn modifier_param(modifiers: ModifiersState) -> Option<u8> {
    let mut val = 1;
    if modifiers.shift_key() {
        val += 1;
    }
    if modifiers.alt_key() {
        val += 2;
    }
    if modifiers.control_key() {
        val += 4;
    }
    if val > 1 { Some(val) } else { None }
}

/// Encodes modified navigation, cursor, and function keys using xterm CSI 1;mod sequences.
fn encode_modified_key(named: NamedKey, m: u8) -> Option<Vec<u8>> {
    match named {
        NamedKey::ArrowUp => Some(format!("\x1b[1;{m}A").into_bytes()),
        NamedKey::ArrowDown => Some(format!("\x1b[1;{m}B").into_bytes()),
        NamedKey::ArrowRight => Some(format!("\x1b[1;{m}C").into_bytes()),
        NamedKey::ArrowLeft => Some(format!("\x1b[1;{m}D").into_bytes()),
        NamedKey::Home => Some(format!("\x1b[1;{m}H").into_bytes()),
        NamedKey::End => Some(format!("\x1b[1;{m}F").into_bytes()),
        NamedKey::PageUp => Some(format!("\x1b[5;{m}~").into_bytes()),
        NamedKey::PageDown => Some(format!("\x1b[6;{m}~").into_bytes()),
        NamedKey::Insert => Some(format!("\x1b[2;{m}~").into_bytes()),
        NamedKey::Delete => Some(format!("\x1b[3;{m}~").into_bytes()),
        NamedKey::F1 => Some(format!("\x1b[1;{m}P").into_bytes()),
        NamedKey::F2 => Some(format!("\x1b[1;{m}Q").into_bytes()),
        NamedKey::F3 => Some(format!("\x1b[1;{m}R").into_bytes()),
        NamedKey::F4 => Some(format!("\x1b[1;{m}S").into_bytes()),
        NamedKey::F5 => Some(format!("\x1b[15;{m}~").into_bytes()),
        NamedKey::F6 => Some(format!("\x1b[17;{m}~").into_bytes()),
        NamedKey::F7 => Some(format!("\x1b[18;{m}~").into_bytes()),
        NamedKey::F8 => Some(format!("\x1b[19;{m}~").into_bytes()),
        NamedKey::F9 => Some(format!("\x1b[20;{m}~").into_bytes()),
        NamedKey::F10 => Some(format!("\x1b[21;{m}~").into_bytes()),
        NamedKey::F11 => Some(format!("\x1b[23;{m}~").into_bytes()),
        NamedKey::F12 => Some(format!("\x1b[24;{m}~").into_bytes()),
        _ => None,
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

    #[test]
    fn encode_alt_as_esc_prefix() {
        let modes = Modes::default();
        let ev_alt_b = KeyEvent::new(
            Key::Character(SmolStr::new("b")),
            Some("b".to_string()),
            ModifiersState::ALT,
            ElementState::Pressed,
        );
        assert_eq!(encode_key(&ev_alt_b, &modes), Some(b"\x1bb".to_vec()));

        let ev_alt_f = KeyEvent::new(
            Key::Character(SmolStr::new("f")),
            Some("f".to_string()),
            ModifiersState::ALT,
            ElementState::Pressed,
        );
        assert_eq!(encode_key(&ev_alt_f, &modes), Some(b"\x1bf".to_vec()));
    }

    #[test]
    fn encode_modified_keys_csi_1_mod() {
        let modes = Modes::default();

        // Ctrl+Up -> \x1b[1;5A
        let ctrl_up = KeyEvent::new(
            Key::Named(NamedKey::ArrowUp),
            None,
            ModifiersState::CONTROL,
            ElementState::Pressed,
        );
        assert_eq!(encode_key(&ctrl_up, &modes), Some(b"\x1b[1;5A".to_vec()));

        // Shift+Down -> \x1b[1;2B
        let shift_down = KeyEvent::new(
            Key::Named(NamedKey::ArrowDown),
            None,
            ModifiersState::SHIFT,
            ElementState::Pressed,
        );
        assert_eq!(encode_key(&shift_down, &modes), Some(b"\x1b[1;2B".to_vec()));

        // Alt+Right -> \x1b[1;3C
        let alt_right = KeyEvent::new(
            Key::Named(NamedKey::ArrowRight),
            None,
            ModifiersState::ALT,
            ElementState::Pressed,
        );
        assert_eq!(encode_key(&alt_right, &modes), Some(b"\x1b[1;3C".to_vec()));

        // Ctrl+Shift+Left -> \x1b[1;6D
        let cs_left = KeyEvent::new(
            Key::Named(NamedKey::ArrowLeft),
            None,
            ModifiersState::CONTROL | ModifiersState::SHIFT,
            ElementState::Pressed,
        );
        assert_eq!(encode_key(&cs_left, &modes), Some(b"\x1b[1;6D".to_vec()));

        // Ctrl+Delete -> \x1b[3;5~
        let ctrl_del = KeyEvent::new(
            Key::Named(NamedKey::Delete),
            None,
            ModifiersState::CONTROL,
            ElementState::Pressed,
        );
        assert_eq!(encode_key(&ctrl_del, &modes), Some(b"\x1b[3;5~".to_vec()));

        // Shift+F1 -> \x1b[1;2P
        let shift_f1 = KeyEvent::new(
            Key::Named(NamedKey::F1),
            None,
            ModifiersState::SHIFT,
            ElementState::Pressed,
        );
        assert_eq!(encode_key(&shift_f1, &modes), Some(b"\x1b[1;2P".to_vec()));

        // Ctrl+F5 -> \x1b[15;5~
        let ctrl_f5 = KeyEvent::new(
            Key::Named(NamedKey::F5),
            None,
            ModifiersState::CONTROL,
            ElementState::Pressed,
        );
        assert_eq!(encode_key(&ctrl_f5, &modes), Some(b"\x1b[15;5~".to_vec()));
    }

    #[test]
    fn encode_paste_unbracketed_and_bracketed() {
        let modes_normal = Modes::default();
        let payload = "echo 'hello world'\n";
        assert_eq!(
            encode_paste(payload, &modes_normal),
            payload.as_bytes().to_vec()
        );

        let modes_bracketed = Modes {
            bracketed_paste: true,
            ..Default::default()
        };
        assert_eq!(
            encode_paste(payload, &modes_bracketed),
            b"\x1b[200~echo 'hello world'\n\x1b[201~".to_vec()
        );
    }

    #[test]
    fn encode_mouse_reporting_sgr_1006() {
        let modes_off = Modes::default();
        let ev_left_press = MouseEvent::new(
            MouseButton::Left,
            MouseAction::Press,
            0,
            0,
            ModifiersState::empty(),
        );
        assert_eq!(encode_mouse_event(&ev_left_press, &modes_off), None);

        let modes_on = Modes {
            mouse_reporting: true,
            ..Default::default()
        };

        // Left press at (0, 0) -> \x1b[<0;1;1M
        assert_eq!(
            encode_mouse_event(&ev_left_press, &modes_on),
            Some(b"\x1b[<0;1;1M".to_vec())
        );

        // Left release at (0, 0) -> \x1b[<0;1;1m
        let ev_left_rel = MouseEvent::new(
            MouseButton::Left,
            MouseAction::Release,
            0,
            0,
            ModifiersState::empty(),
        );
        assert_eq!(
            encode_mouse_event(&ev_left_rel, &modes_on),
            Some(b"\x1b[<0;1;1m".to_vec())
        );

        // Right press with Shift at (5, 5) -> 2 + 4 = 6 -> \x1b[<6;6;6M
        let ev_shift_right = MouseEvent::new(
            MouseButton::Right,
            MouseAction::Press,
            5,
            5,
            ModifiersState::SHIFT,
        );
        assert_eq!(
            encode_mouse_event(&ev_shift_right, &modes_on),
            Some(b"\x1b[<6;6;6M".to_vec())
        );

        // WheelUp at (2, 3) -> 64 -> \x1b[<64;3;4M
        let ev_wheel = MouseEvent::new(
            MouseButton::WheelUp,
            MouseAction::Press,
            2,
            3,
            ModifiersState::empty(),
        );
        assert_eq!(
            encode_mouse_event(&ev_wheel, &modes_on),
            Some(b"\x1b[<64;3;4M".to_vec())
        );

        // Motion drag at (7, 8) with Left button -> 0 + 32 = 32 -> \x1b[<32;8;9M
        let ev_motion = MouseEvent::new(
            MouseButton::Left,
            MouseAction::Motion,
            7,
            8,
            ModifiersState::empty(),
        );
        assert_eq!(
            encode_mouse_event(&ev_motion, &modes_on),
            Some(b"\x1b[<32;8;9M".to_vec())
        );
    }

    #[test]
    fn check_scrollback_keybindings() {
        let shift_page_up = KeyEvent::new(
            Key::Named(NamedKey::PageUp),
            None,
            ModifiersState::SHIFT,
            ElementState::Pressed,
        );
        assert_eq!(
            check_input_action(&shift_page_up),
            Some(InputAction::ScrollUp(12))
        );

        let shift_page_down = KeyEvent::new(
            Key::Named(NamedKey::PageDown),
            None,
            ModifiersState::SHIFT,
            ElementState::Pressed,
        );
        assert_eq!(
            check_input_action(&shift_page_down),
            Some(InputAction::ScrollDown(12))
        );

        let shift_home = KeyEvent::new(
            Key::Named(NamedKey::Home),
            None,
            ModifiersState::SHIFT,
            ElementState::Pressed,
        );
        assert_eq!(
            check_input_action(&shift_home),
            Some(InputAction::ScrollToTop)
        );

        let shift_end = KeyEvent::new(
            Key::Named(NamedKey::End),
            None,
            ModifiersState::SHIFT,
            ElementState::Pressed,
        );
        assert_eq!(
            check_input_action(&shift_end),
            Some(InputAction::ScrollToBottom)
        );

        // Plain PageUp is not an input action (handled by encode_key)
        let plain_page_up = KeyEvent::simple(Key::Named(NamedKey::PageUp), None);
        assert_eq!(check_input_action(&plain_page_up), None);
    }

    #[test]
    fn selection_containment_and_text_extraction() {
        use crate::grid::{Attrs, Color, Grid};

        let mut grid = Grid::new(10, 3);
        // Row 0: "hello     "
        for (i, ch) in "hello".chars().enumerate() {
            grid.move_cursor_to(0, i);
            grid.put_char(ch, Color::Default, Color::Default, Attrs::empty(), false);
        }
        // Row 1: "world     "
        for (i, ch) in "world".chars().enumerate() {
            grid.move_cursor_to(1, i);
            grid.put_char(ch, Color::Default, Color::Default, Attrs::empty(), false);
        }

        // Single line selection: "ell"
        let sel_single = Selection::new(GridPoint::new(1, 0), GridPoint::new(3, 0));
        assert!(sel_single.contains(0, 1));
        assert!(sel_single.contains(0, 2));
        assert!(sel_single.contains(0, 3));
        assert!(!sel_single.contains(0, 0));
        assert!(!sel_single.contains(0, 4));
        assert_eq!(sel_single.extract_text(&grid), "ell");

        // Multi-line selection from (3, 0) ("lo") to (2, 1) ("wor")
        let sel_multi = Selection::new(GridPoint::new(3, 0), GridPoint::new(2, 1));
        assert!(sel_multi.contains(0, 3));
        assert!(sel_multi.contains(0, 4));
        assert!(sel_multi.contains(1, 0));
        assert!(sel_multi.contains(1, 2));
        assert!(!sel_multi.contains(1, 3));
        assert_eq!(sel_multi.extract_text(&grid), "lo\nwor");
    }

    #[test]
    fn check_clipboard_copy_paste_keybindings() {
        // Ctrl+Shift+C -> Copy
        let ctrl_shift_c = KeyEvent::new(
            Key::Character(SmolStr::new("c")),
            Some("c".to_string()),
            ModifiersState::CONTROL | ModifiersState::SHIFT,
            ElementState::Pressed,
        );
        assert_eq!(check_input_action(&ctrl_shift_c), Some(InputAction::Copy));

        // Ctrl+Shift+V -> Paste
        let ctrl_shift_v = KeyEvent::new(
            Key::Character(SmolStr::new("v")),
            Some("v".to_string()),
            ModifiersState::CONTROL | ModifiersState::SHIFT,
            ElementState::Pressed,
        );
        assert_eq!(check_input_action(&ctrl_shift_v), Some(InputAction::Paste));

        // Plain Ctrl+C is NOT an input action (handled by encode_key for SIGINT)
        let ctrl_c = KeyEvent::new(
            Key::Character(SmolStr::new("c")),
            Some("c".to_string()),
            ModifiersState::CONTROL,
            ElementState::Pressed,
        );
        assert_eq!(check_input_action(&ctrl_c), None);
    }
}
