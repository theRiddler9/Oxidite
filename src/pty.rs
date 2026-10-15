//! PTY spawn, read, write, and resize (stub — Phase 1).

use std::fmt;

/// Desired PTY geometry in character cells and pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
    /// Height in character cells.
    pub rows: u16,
    /// Width in character cells.
    pub cols: u16,
    /// Width in pixels (advisory).
    pub px_w: u16,
    /// Height in pixels (advisory).
    pub px_h: u16,
}

/// Shell command to run on the PTY slave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCommand {
    /// Path or binary name of the shell program to execute.
    pub program: String,
    /// Command-line arguments passed to the shell program.
    pub args: Vec<String>,
}

#[cfg(windows)]
fn which_in_path(binary: &str) -> bool {
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let full = dir.join(binary);
            if full.is_file() {
                return true;
            }
        }
    }
    false
}

#[cfg(windows)]
fn detect_default_shell() -> (String, Vec<String>) {
    if which_in_path("pwsh.exe") {
        ("pwsh.exe".to_string(), Vec::new())
    } else if which_in_path("powershell.exe") {
        ("powershell.exe".to_string(), Vec::new())
    } else {
        ("cmd.exe".to_string(), Vec::new())
    }
}

#[cfg(not(windows))]
fn detect_default_shell() -> (String, Vec<String>) {
    if let Ok(shell) = std::env::var("SHELL") {
        if !shell.is_empty() {
            return (shell, Vec::new());
        }
    }
    ("/bin/sh".to_string(), Vec::new())
}

impl ShellCommand {
    /// Creates a new shell command with no arguments.
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
        }
    }

    /// Creates a new shell command with a program and arguments.
    pub fn with_args<I, S>(program: impl Into<String>, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }

    /// Detects the system default shell according to platform conventions.
    ///
    /// On Windows: probes `pwsh.exe`, then `powershell.exe`, then `cmd.exe`.
    /// On Unix: reads `$SHELL`, falling back to `/bin/sh`.
    pub fn default_shell() -> Self {
        let (program, args) = detect_default_shell();
        Self { program, args }
    }
}

/// Errors from PTY operations.
#[derive(Debug)]
pub enum PtyError {
    /// Stub until Phase 1 implements spawn.
    NotImplemented,
}

impl fmt::Display for PtyError {
    /// Formats the PTY error for display.
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
    /// Writes bytes to the shell.
    #[allow(clippy::unused_self)]
    pub fn write(&mut self, _bytes: &[u8]) -> Result<usize, PtyError> {
        Err(PtyError::NotImplemented)
    }

    /// Notifies the PTY of a new size.
    #[allow(clippy::unused_self)]
    pub fn resize(&mut self, _size: PtySize) -> Result<(), PtyError> {
        Err(PtyError::NotImplemented)
    }

    /// Kills the child process.
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

/// Spawns a shell on a PTY with the given size.
pub fn spawn(_cmd: ShellCommand, _size: PtySize) -> Result<(PtyWriter, PtyReader), PtyError> {
    Err(PtyError::NotImplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that `ShellCommand::default_shell` returns a non-empty program.
    #[test]
    fn default_shell_is_non_empty() {
        let cmd = ShellCommand::default_shell();
        assert!(!cmd.program.is_empty());
        #[cfg(windows)]
        {
            let prog = cmd.program.to_lowercase();
            assert!(
                prog.contains("pwsh") || prog.contains("powershell") || prog.contains("cmd"),
                "unexpected Windows default shell: {prog}"
            );
        }
    }

    /// Verifies `ShellCommand` constructors with and without arguments.
    #[test]
    fn shell_command_constructors() {
        let cmd = ShellCommand::new("test");
        assert_eq!(cmd.program, "test");
        assert!(cmd.args.is_empty());

        let cmd = ShellCommand::with_args("test", ["--flag", "arg"]);
        assert_eq!(cmd.program, "test");
        assert_eq!(cmd.args, vec!["--flag", "arg"]);
    }
}
