//! Oxidite — cross-platform terminal emulator.
//!
//! Phase 0: contract stubs only. Real PTY passthrough starts in Phase 1.

// Contract stubs are intentionally unused until later phases wire them up.
#![allow(dead_code)]

mod app;
mod config;
mod grid;
mod handler;
mod input;
mod parser;
mod pty;
mod renderer;
mod terminal;
mod theme;

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Runs the PTY in headless passthrough mode, forwarding stdin to the PTY
/// and PTY output to stdout.
fn run_headless(cmd: pty::ShellCommand) -> anyhow::Result<i32> {
    let size = pty::PtySize {
        rows: 24,
        cols: 80,
        px_w: 0,
        px_h: 0,
    };

    let (writer, mut reader) =
        pty::spawn(cmd, size).map_err(|e| anyhow::anyhow!("failed to spawn PTY: {e}"))?;

    let writer = Arc::new(Mutex::new(writer));
    let stdin_writer = Arc::clone(&writer);

    // Spawn stdin forwarding thread
    std::thread::Builder::new()
        .name("stdin-forwarder".to_string())
        .spawn(move || {
            let mut stdin = std::io::stdin();
            let mut buf = [0u8; 1024];
            loop {
                match stdin.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if let Ok(mut guard) = stdin_writer.lock() {
                            if guard.write(&buf[..n]).is_err() {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                }
            }
        })?;

    let mut stdout = std::io::stdout();
    let mut exit_code = 0;

    loop {
        // Poll child process exit status
        {
            if let Ok(mut guard) = writer.lock() {
                match guard.try_wait() {
                    Ok(Some(code)) => {
                        exit_code = i32::try_from(code).unwrap_or(1);
                        while let Some(bytes) = reader.try_recv() {
                            let _ = stdout.write_all(&bytes);
                        }
                        let _ = stdout.flush();
                        break;
                    }
                    Ok(None) => {}
                    Err(_) => break,
                }
            }
        }

        // Receive next chunk of PTY output with timeout
        match reader.recv_timeout(Duration::from_millis(50)) {
            Ok(bytes) => {
                // Respond to cursor position query (DSR) if requested
                if bytes.windows(4).any(|w| w == b"\x1b[6n")
                    && let Ok(mut guard) = writer.lock()
                {
                    let _ = guard.write(b"\x1b[1;1R");
                }
                stdout.write_all(&bytes)?;
                stdout.flush()?;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                if let Ok(mut guard) = writer.lock()
                    && let Ok(code) = guard.wait()
                {
                    exit_code = i32::try_from(code).unwrap_or(1);
                }
                break;
            }
        }
    }

    Ok(exit_code)
}

/// Entry point for Oxidite.
fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("Oxidite terminal emulator");
        println!("Usage: oxidite [--headless] [-- <command> [args...]]");
        return Ok(());
    }

    let cmd = if let Some(pos) = args.iter().position(|a| a == "--") {
        let program = args
            .get(pos + 1)
            .cloned()
            .unwrap_or_else(|| pty::ShellCommand::default_shell().program);
        let cmd_args = args[pos + 2..].to_vec();
        pty::ShellCommand::with_args(program, cmd_args)
    } else {
        pty::ShellCommand::default_shell()
    };

    let exit_code = run_headless(cmd)?;
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}
