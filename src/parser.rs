//! VT/ANSI parser wrapping `vte` (stub — Phase 2).

use crate::handler::Handler;

/// Parser that feeds bytes into a [`Handler`].
#[derive(Debug, Default)]
pub struct Parser;

impl Parser {
    pub fn new() -> Self {
        Self
    }

    /// Advance the parser with raw PTY bytes.
    #[allow(clippy::unused_self)]
    pub fn advance(&mut self, _handler: &mut dyn Handler, _bytes: &[u8]) {
        // Phase 2: wrap `vte::Parser` and dispatch via `Perform`.
    }
}
