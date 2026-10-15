//! VT/ANSI parser wrapping `vte` (stub — Phase 2).

use crate::handler::Handler;

/// Parser that feeds bytes into a [`Handler`].
pub struct Parser {
    inner: vte::Parser,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    /// Creates an empty parser with no pending escape state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: vte::Parser::new(),
        }
    }

    /// Advances the parser with raw PTY bytes, calling `handler` as sequences complete.
    pub fn advance(&mut self, handler: &mut dyn Handler, bytes: &[u8]) {
        let mut dispatcher = Dispatcher { handler };
        self.inner.advance(&mut dispatcher, bytes);
    }
}

/// Helper returning parameter at `index` or `default` if missing or zero.
fn param_or(params: &vte::Params, index: usize, default: u16) -> u16 {
    params
        .iter()
        .nth(index)
        .and_then(|subparams| subparams.first().copied())
        .map_or(default, |val| if val == 0 { default } else { val })
}

/// Converts a 16-bit parameter to an `isize` displacement without wrapping.
fn to_isize(val: u16) -> isize {
    isize::try_from(val).unwrap_or(isize::MAX)
}

/// Internal dispatcher connecting [`vte::Perform`] events to a [`Handler`].
struct Dispatcher<'a> {
    handler: &'a mut dyn Handler,
}

impl vte::Perform for Dispatcher<'_> {
    fn print(&mut self, c: char) {
        self.handler.print(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x07 => self.handler.bell(),
            0x08 => self.handler.backspace(),
            0x09 => self.handler.tab(),
            0x0A..=0x0C => self.handler.linefeed(),
            0x0D => self.handler.carriage_return(),
            _ => {}
        }
    }

    fn hook(&mut self, _params: &vte::Params, _intermediates: &[u8], _ignore: bool, _action: char) {
    }

    fn put(&mut self, _byte: u8) {}

    fn unhook(&mut self) {}

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        if intermediates.is_empty() {
            match action {
                // CUU — Cursor Up (CSI Ps A)
                'A' => {
                    let count = to_isize(param_or(params, 0, 1));
                    self.handler.move_cursor_by(-count, 0);
                }
                // CUD — Cursor Down (CSI Ps B)
                'B' => {
                    let count = to_isize(param_or(params, 0, 1));
                    self.handler.move_cursor_by(count, 0);
                }
                // CUF — Cursor Forward (CSI Ps C)
                'C' => {
                    let count = to_isize(param_or(params, 0, 1));
                    self.handler.move_cursor_by(0, count);
                }
                // CUB — Cursor Backward (CSI Ps D)
                'D' => {
                    let count = to_isize(param_or(params, 0, 1));
                    self.handler.move_cursor_by(0, -count);
                }
                // CUP / HVP — Cursor Position (CSI Ps ; Ps H / f)
                'H' | 'f' => {
                    let row = usize::from(param_or(params, 0, 1).saturating_sub(1));
                    let col = usize::from(param_or(params, 1, 1).saturating_sub(1));
                    self.handler.move_cursor_to(row, col);
                }
                _ => {}
            }
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}

/// A [`Handler`] implementation that logs method calls and records them for debugging and tests.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LoggingHandler {
    /// Recorded call trace formatted as human-readable strings.
    pub events: Vec<String>,
}

impl LoggingHandler {
    /// Creates a new, empty logging handler.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears recorded events.
    pub fn clear(&mut self) {
        self.events.clear();
    }
}

impl Handler for LoggingHandler {
    fn print(&mut self, c: char) {
        self.events.push(format!("print({c:?})"));
    }

    fn linefeed(&mut self) {
        self.events.push("linefeed".into());
    }

    fn carriage_return(&mut self) {
        self.events.push("carriage_return".into());
    }

    fn backspace(&mut self) {
        self.events.push("backspace".into());
    }

    fn tab(&mut self) {
        self.events.push("tab".into());
    }

    fn bell(&mut self) {
        self.events.push("bell".into());
    }

    fn move_cursor_to(&mut self, row: usize, col: usize) {
        self.events.push(format!("move_cursor_to({row}, {col})"));
    }

    fn move_cursor_by(&mut self, d_row: isize, d_col: isize) {
        self.events
            .push(format!("move_cursor_by({d_row}, {d_col})"));
    }

    fn erase_in_display(&mut self, mode: crate::handler::EraseMode) {
        self.events.push(format!("erase_in_display({mode:?})"));
    }

    fn erase_in_line(&mut self, mode: crate::handler::EraseMode) {
        self.events.push(format!("erase_in_line({mode:?})"));
    }

    fn set_sgr(&mut self, attr: crate::handler::SgrAttr) {
        self.events.push(format!("set_sgr({attr:?})"));
    }

    fn set_mode(&mut self, mode: crate::handler::Mode, enabled: bool) {
        self.events.push(format!("set_mode({mode:?}, {enabled})"));
    }

    fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        self.events
            .push(format!("set_scroll_region({top}, {bottom})"));
    }

    fn set_title(&mut self, title: String) {
        self.events.push(format!("set_title({title:?})"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logging_handler_records_calls() {
        let mut handler = LoggingHandler::new();
        handler.bell();
        handler.carriage_return();
        handler.linefeed();
        assert_eq!(handler.events, vec!["bell", "carriage_return", "linefeed"]);
        handler.clear();
        assert_eq!(handler.events, Vec::<String>::new());
    }

    #[test]
    fn parser_handles_print_and_c0() {
        let mut parser = Parser::new();
        let mut handler = LoggingHandler::new();

        parser.advance(&mut handler, b"Hi\r\n\t\x08\x07");
        assert_eq!(
            handler.events,
            vec![
                "print('H')",
                "print('i')",
                "carriage_return",
                "linefeed",
                "tab",
                "backspace",
                "bell",
            ]
        );
    }

    #[test]
    fn parser_handles_csi_cursor_movement() {
        let mut parser = Parser::new();
        let mut handler = LoggingHandler::new();

        // CUU, CUD, CUF, CUB with default and explicit parameters
        parser.advance(&mut handler, b"\x1b[A\x1b[3B\x1b[5C\x1b[2D");
        // CUP and HVP
        parser.advance(&mut handler, b"\x1b[H\x1b[12;34H\x1b[5;10f");

        assert_eq!(
            handler.events,
            vec![
                "move_cursor_by(-1, 0)",
                "move_cursor_by(3, 0)",
                "move_cursor_by(0, 5)",
                "move_cursor_by(0, -2)",
                "move_cursor_to(0, 0)",
                "move_cursor_to(11, 33)",
                "move_cursor_to(4, 9)",
            ]
        );
    }
}
