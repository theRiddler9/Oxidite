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
                let output_bytes = if bytes.windows(4).any(|w| w == b"\x1b[6n") {
                    if let Ok(mut guard) = writer.lock() {
                        let _ = guard.write(b"\x1b[1;1R");
                    }
                    let mut clean = Vec::with_capacity(bytes.len());
                    let mut i = 0;
                    while i < bytes.len() {
                        if i + 4 <= bytes.len() && &bytes[i..i + 4] == b"\x1b[6n" {
                            i += 4;
                        } else {
                            clean.push(bytes[i]);
                            i += 1;
                        }
                    }
                    clean
                } else {
                    bytes
                };
                stdout.write_all(&output_bytes)?;
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

/// Runs headless grid dump mode: feeds PTY output into Parser and Terminal,
/// and prints the formatted grid to stdout upon completion.
fn run_grid_dump(cmd: Option<pty::ShellCommand>) -> anyhow::Result<i32> {
    let mut term = terminal::Terminal::new(80, 24);
    let mut vt_parser = parser::Parser::new();

    if let Some(cmd) = cmd {
        let size = pty::PtySize {
            rows: 24,
            cols: 80,
            px_w: 0,
            px_h: 0,
        };

        let (writer, mut reader) =
            pty::spawn(cmd, size).map_err(|e| anyhow::anyhow!("failed to spawn PTY: {e}"))?;

        let writer = Arc::new(Mutex::new(writer));
        let mut exit_code = 0;

        loop {
            // Check child exit status
            if let Ok(mut guard) = writer.lock() {
                match guard.try_wait() {
                    Ok(Some(code)) => {
                        exit_code = i32::try_from(code).unwrap_or(1);
                        while let Some(bytes) = reader.try_recv() {
                            vt_parser.advance(&mut term, &bytes);
                        }
                        break;
                    }
                    Ok(None) => {}
                    Err(_) => break,
                }
            }

            match reader.recv_timeout(Duration::from_millis(50)) {
                Ok(bytes) => {
                    if bytes.windows(4).any(|w| w == b"\x1b[6n")
                        && let Ok(mut guard) = writer.lock()
                    {
                        let _ = guard.write(b"\x1b[1;1R");
                    }
                    vt_parser.advance(&mut term, &bytes);
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

        print!("{}", term.grid.to_plain_text());
        Ok(exit_code)
    } else {
        // Read directly from stdin until EOF
        let mut stdin = std::io::stdin();
        let mut buf = Vec::new();
        stdin.read_to_end(&mut buf)?;
        vt_parser.advance(&mut term, &buf);
        print!("{}", term.grid.to_plain_text());
        Ok(0)
    }
}

/// Runs Oxidite in interactive GPU window mode: creates event loop, spawns PTY,
/// and runs the winit application handler.
fn run_gui(cmd: pty::ShellCommand) -> anyhow::Result<i32> {
    let event_loop = renderer::window::create_event_loop::<app::AppEvent>()
        .map_err(|e| anyhow::anyhow!("failed to create event loop: {e}"))?;
    let proxy = event_loop.create_proxy();

    let term = terminal::Terminal::new(80, 24);
    let size = pty::PtySize {
        rows: 24,
        cols: 80,
        px_w: 800,
        px_h: 600,
    };

    let (writer, reader) =
        pty::spawn(cmd, size).map_err(|e| anyhow::anyhow!("failed to spawn PTY: {e}"))?;

    let writer = Arc::new(Mutex::new(writer));
    app::spawn_pty_forwarder(reader, Arc::clone(&writer), proxy.clone());

    let mut application = app::App::new(Some(proxy), term, Some(writer));
    event_loop.run_app(&mut application)?;

    Ok(application.pty_exit_code.unwrap_or(0))
}

/// Entry point for Oxidite.
fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("Oxidite terminal emulator");
        println!("Usage: oxidite [--dump-grid] [--headless] [-- <command> [args...]]");
        return Ok(());
    }

    let dump_grid = args.iter().any(|a| a == "--dump-grid");
    let headless = args.iter().any(|a| a == "--headless");

    let cmd = if let Some(pos) = args.iter().position(|a| a == "--") {
        let program = args
            .get(pos + 1)
            .cloned()
            .unwrap_or_else(|| pty::ShellCommand::default_shell().program);
        let cmd_args = args[pos + 2..].to_vec();
        Some(pty::ShellCommand::with_args(program, cmd_args))
    } else if dump_grid {
        None
    } else {
        Some(pty::ShellCommand::default_shell())
    };

    let exit_code = if dump_grid {
        run_grid_dump(cmd)?
    } else if headless {
        let cmd = cmd.unwrap_or_else(pty::ShellCommand::default_shell);
        run_headless(cmd)?
    } else {
        let cmd = cmd.unwrap_or_else(pty::ShellCommand::default_shell);
        run_gui(cmd)?
    };

    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}
