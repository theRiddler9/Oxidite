//! PTY spawn, read, write, and resize (stub — Phase 1).

use std::fmt;

/// Desired PTY geometry in character cells and pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
    pub rows: u16,
    pub cols: u16,
    pub px_w: u16,
    pub px_h: u16,
}

/// Shell command to run on the PTY slave.
#[derive(Debug, Clone)]
pub struct ShellCommand {
    pub program: String,
    pub args: Vec<String>,
}

/// Errors from PTY operations.
#[derive(Debug)]
pub enum PtyError {
    /// Stub until Phase 1 implements spawn.
    NotImplemented,
}

impl fmt::Display for PtyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotImplemented => write!(f, "PTY not implemented yet (Phase 1)"),
        }
    }
}

impl std::error::Error for PtyError {}

/// Write side of the PTY master (stub).
#[derive(Debug)]
pub struct PtyWriter;

impl PtyWriter {
    /// Write bytes to the shell.
    #[allow(clippy::unused_self)]
    pub fn write(&mut self, _bytes: &[u8]) -> Result<usize, PtyError> {
        Err(PtyError::NotImplemented)
    }

    /// Notify the PTY of a new size.
    #[allow(clippy::unused_self)]
    pub fn resize(&mut self, _size: PtySize) -> Result<(), PtyError> {
        Err(PtyError::NotImplemented)
    }

    /// Kill the child process.
    #[allow(clippy::unused_self)]
    pub fn kill(&mut self) -> Result<(), PtyError> {
        Err(PtyError::NotImplemented)
    }
}

/// Read side of the PTY master (stub).
///
/// Phase 1 runs a reader thread that forwards bytes as `AppEvent::PtyOutput`.
#[derive(Debug)]
pub struct PtyReader;

/// Spawn a shell on a PTY with the given size.
pub fn spawn(_cmd: ShellCommand, _size: PtySize) -> Result<(PtyWriter, PtyReader), PtyError> {
    Err(PtyError::NotImplemented)
}
