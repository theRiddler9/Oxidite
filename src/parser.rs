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
    fn print(&mut self, _c: char) {}

    fn execute(&mut self, _byte: u8) {}

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
