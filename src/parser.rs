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
        _params: &vte::Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: char,
    ) {
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
}
