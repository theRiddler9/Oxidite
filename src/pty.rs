use portable_pty::{Child, CommandBuilder, MasterPty, PtySize as NativePtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use thiserror::Error;

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

impl From<PtySize> for NativePtySize {
    fn from(size: PtySize) -> Self {
        Self {
            // Windows ConPTY and Unix PTYs require positive non-zero geometry (min 1x1)
            // to avoid E_INVALIDARG errors in CreatePseudoConsole / TIOCSWINSZ.
            rows: size.rows.max(1),
            cols: size.cols.max(1),
            pixel_width: size.px_w,
            pixel_height: size.px_h,
        }
    }
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
    } else if let Ok(comspec) = std::env::var("COMSPEC") {
        (comspec, Vec::new())
    } else {
        ("cmd.exe".to_string(), Vec::new())
    }
}

#[cfg(not(windows))]
fn detect_default_shell() -> (String, Vec<String>) {
    if let Ok(shell) = std::env::var("SHELL")
        && !shell.is_empty()
    {
        return (shell, Vec::new());
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
#[derive(Debug, Error)]
pub enum PtyError {
    /// Failed to open a new PTY instance.
    #[error("failed to open PTY: {0}")]
    Open(String),

    /// Failed to spawn the shell process.
    #[error("failed to spawn shell process: {0}")]
    Spawn(String),

    /// Failed to acquire writer from PTY master.
    #[error("failed to acquire writer from PTY: {0}")]
    TakeWriter(String),

    /// Failed to write bytes to the PTY master.
    #[error("failed to write to PTY: {0}")]
    Write(#[from] std::io::Error),

    /// Failed to resize the PTY master.
    #[error("failed to resize PTY: {0}")]
    Resize(String),

    /// Failed to kill the child process.
    #[error("failed to kill child process: {0}")]
    Kill(String),

    /// Failed to acquire reader from PTY master.
    #[error("failed to acquire reader from PTY: {0}")]
    TakeReader(String),

    /// Child process wait error.
    #[error("failed to wait on child process: {0}")]
    Wait(String),
}

/// Write side of the PTY master.
pub struct PtyWriter {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

impl std::fmt::Debug for PtyWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyWriter").finish_non_exhaustive()
    }
}

impl PtyWriter {
    /// Writes bytes to the shell.
    pub fn write(&mut self, bytes: &[u8]) -> Result<usize, PtyError> {
        let n = self.writer.write(bytes)?;
        self.writer.flush()?;
        Ok(n)
    }

    /// Notifies the PTY of a new size.
    pub fn resize(&mut self, size: PtySize) -> Result<(), PtyError> {
        self.master
            .resize(size.into())
            .map_err(|e| PtyError::Resize(e.to_string()))
    }

    /// Kills the child process.
    pub fn kill(&mut self) -> Result<(), PtyError> {
        self.child.kill().map_err(|e| PtyError::Kill(e.to_string()))
    }

    /// Checks if the child process has exited without blocking.
    pub fn try_wait(&mut self) -> Result<Option<u32>, PtyError> {
        self.child
            .try_wait()
            .map(|opt| opt.map(|s| s.exit_code()))
            .map_err(|e| PtyError::Wait(e.to_string()))
    }

    /// Waits for the child process to exit.
    pub fn wait(&mut self) -> Result<u32, PtyError> {
        self.child
            .wait()
            .map(|s| s.exit_code())
            .map_err(|e| PtyError::Wait(e.to_string()))
    }
}

/// Read side of the PTY master.
///
/// Runs a reader thread that forwards bytes as chunks over a channel.
pub struct PtyReader {
    receiver: Receiver<Vec<u8>>,
    reader_thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for PtyReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyReader").finish_non_exhaustive()
    }
}

impl PtyReader {
    /// Attempts to read the next chunk of bytes without blocking.
    pub fn try_recv(&mut self) -> Option<Vec<u8>> {
        self.receiver.try_recv().ok()
    }

    /// Blocks until the next chunk of bytes is received from the PTY.
    pub fn recv(&mut self) -> Option<Vec<u8>> {
        self.receiver.recv().ok()
    }

    /// Waits for the next chunk of bytes from the PTY up to `timeout`.
    pub fn recv_timeout(
        &mut self,
        timeout: std::time::Duration,
    ) -> Result<Vec<u8>, std::sync::mpsc::RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }

    /// Returns a reference to the receiver channel.
    pub fn receiver(&self) -> &Receiver<Vec<u8>> {
        &self.receiver
    }

    /// Creates a `PtyReader` wrapping a channel receiver for testing.
    #[cfg(test)]
    #[must_use]
    pub fn from_receiver(receiver: Receiver<Vec<u8>>) -> Self {
        Self {
            receiver,
            reader_thread: None,
        }
    }
}

/// Spawns a shell on a PTY with the given size.
pub fn spawn(cmd: ShellCommand, size: PtySize) -> Result<(PtyWriter, PtyReader), PtyError> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(size.into())
        .map_err(|e| PtyError::Open(e.to_string()))?;

    let mut builder = CommandBuilder::new(cmd.program);
    builder.args(cmd.args);
    builder.env("TERM", "xterm-256color");
    builder.env("COLORTERM", "truecolor");
    builder.env("TERM_PROGRAM", "Oxidite");
    builder.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
    #[cfg(windows)]
    {
        // ConPTY and modern PowerShell utilities check for WT_SESSION to activate full VT capabilities.
        if std::env::var_os("WT_SESSION").is_none() {
            builder.env("WT_SESSION", "00000000-0000-0000-0000-000000000000");
        }
    }

    let child = pair
        .slave
        .spawn_command(builder)
        .map_err(|e| PtyError::Spawn(e.to_string()))?;

    // Drop the slave handle in this process so EOF is triggered when the child terminates.
    drop(pair.slave);

    let writer = pair
        .master
        .take_writer()
        .map_err(|e| PtyError::TakeWriter(e.to_string()))?;

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| PtyError::TakeReader(e.to_string()))?;

    let (sender, receiver) = mpsc::channel();
    let reader_thread = thread::Builder::new()
        .name("pty-reader".to_string())
        .spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if sender.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        })
        .map_err(|e| PtyError::TakeReader(e.to_string()))?;

    let pty_writer = PtyWriter {
        master: pair.master,
        writer,
        child,
    };

    let pty_reader = PtyReader {
        receiver,
        reader_thread: Some(reader_thread),
    };

    Ok((pty_writer, pty_reader))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that `ShellCommand::default_shell` returns a non-empty program.
    #[test]
    fn default_shell_is_non_empty() {
        let cmd = ShellCommand::default_shell();
        assert_ne!(cmd.program, "");
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
        assert_eq!(cmd.args, Vec::<String>::new());

        let cmd = ShellCommand::with_args("test", ["--flag", "arg"]);
        assert_eq!(cmd.program, "test");
        assert_eq!(cmd.args, vec!["--flag", "arg"]);
    }

    /// Verifies spawning a PTY with an initial size and reading output.
    #[test]
    fn spawn_and_read_output() {
        #[cfg(windows)]
        let cmd = ShellCommand::with_args("cmd.exe", ["/C", "echo", "oxidite_pty_test"]);
        #[cfg(not(windows))]
        let cmd = ShellCommand::with_args("/bin/sh", ["-c", "echo oxidite_pty_test"]);

        let size = PtySize {
            rows: 24,
            cols: 80,
            px_w: 0,
            px_h: 0,
        };

        let (mut writer, mut reader) = spawn(cmd, size).expect("failed to spawn PTY");
        let mut output = Vec::new();
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(10) {
            match reader.recv_timeout(std::time::Duration::from_millis(200)) {
                Ok(bytes) => {
                    if bytes.windows(4).any(|w| w == b"\x1b[6n") {
                        let _ = writer.write(b"\x1b[1;1R");
                    }
                    output.extend_from_slice(&bytes);
                    if String::from_utf8_lossy(&output).contains("oxidite_pty_test") {
                        break;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if String::from_utf8_lossy(&output).contains("oxidite_pty_test") {
                        break;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        let text = String::from_utf8_lossy(&output);
        assert!(
            text.contains("oxidite_pty_test"),
            "expected output to contain 'oxidite_pty_test', got: {text}"
        );
    }

    #[test]
    fn pty_size_clamping_prevents_zero_dimensions_for_conpty() {
        let zero_size = PtySize {
            rows: 0,
            cols: 0,
            px_w: 0,
            px_h: 0,
        };
        let native: NativePtySize = zero_size.into();
        assert_eq!(native.rows, 1, "ConPTY requires rows >= 1");
        assert_eq!(native.cols, 1, "ConPTY requires cols >= 1");
    }

    #[test]
    fn pty_size_preserves_valid_dimensions() {
        let size = PtySize {
            rows: 50,
            cols: 120,
            px_w: 960,
            px_h: 800,
        };
        let native: NativePtySize = size.into();
        assert_eq!(native.rows, 50);
        assert_eq!(native.cols, 120);
        assert_eq!(native.pixel_width, 960);
        assert_eq!(native.pixel_height, 800);
    }
}
