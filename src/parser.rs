//! VT/ANSI parser wrapping `vte` (stub — Phase 2).

use crate::handler::Handler;

/// Parser that feeds bytes into a [`Handler`].
#[derive(Debug, Default)]
pub struct Parser;

impl Parser {
    /// Creates an empty parser with no pending escape state.
    pub fn new() -> Self {
        Self
    }

    /// Advances the parser with raw PTY bytes, calling `handler` as sequences complete.
    #[allow(clippy::unused_self)]
    pub fn advance(&mut self, _handler: &mut dyn Handler, _bytes: &[u8]) {
        // Phase 2: wrap `vte::Parser` and dispatch via `Perform`.
    }
}
