# Phase 1 — The PTY (Pseudoterminal)

## In plain language

A terminal emulator does not run commands itself. It hosts a shell process (like
`pwsh.exe`, `powershell.exe`, or `/bin/sh`), pretending to be a physical teletype
device.

Phase 1 establishes the **plumbing**: it spawns a real shell process attached to
a pseudoterminal (PTY), sets standard environment variables (`TERM=xterm-256color`,
`COLORTERM=truecolor`), reads its raw output stream on a dedicated background
thread, accepts keyboard input into the PTY writer, and cleanly exits when the
child shell terminates.

You can now run `cargo run` or `cargo run -- -- <cmd>` to run a real interactive
shell through Oxidite's PTY subsystem without any GUI.

## Key ideas

- **What is a PTY?**
  - A PTY is a bidirectional communication channel with two ends: a **slave** and
    a **master**.
  - The shell child process attaches to the **slave** end, believing standard
    input, output, and error are connected to a physical hardware terminal.
  - Oxidite holds the **master** end. Everything the shell prints comes out of the
    master reader; everything Oxidite writes into the master writer goes directly
    into the shell's standard input.
- **ConPTY vs Unix PTYs:**
  - On Unix, PTYs are managed by the kernel (`openpty` / `posix_openpt`).
  - On Windows 10+, Windows provides **ConPTY** (`CreatePseudoConsole`). It converts
    Windows console API calls into standard VT/ANSI escape code byte streams.
  - `portable-pty` abstracts both platforms behind a unified Rust interface.
- **The ConPTY Startup Handshake (DSR / CPR):**
  - When ConPTY initializes, it queries the host terminal for its cursor position
    by emitting `ESC [ 6 n` (Device Status Report - DSR).
  - The host terminal must respond with `ESC [ 1 ; 1 R` (Cursor Position Report -
    CPR). If unhandled, ConPTY delays for its internal timeout (~5 seconds) before
    outputting child text. Handling DSR ensures sub-millisecond startup.
- **Child Lifecycle & Non-Blocking Reads:**
  - In ConPTY, the output pipe does not close automatically when the child shell
    exits because the Windows `conhost.exe` pseudo-console agent keeps the pipe open.
  - Oxidite's main loop polls the child's exit status (`try_wait()`). When the child
    terminates, Oxidite drains the remaining bytes, closes the PTY master, and
    exits with the child's exact exit code.

## Where to look in the code

| Thing | Path |
|---|---|
| PTY spawn, reader, writer, and types | `src/pty.rs` |
| Default shell detection (`pwsh` -> `powershell` -> `cmd` / `$SHELL`) | `src/pty.rs` (`detect_default_shell`) |
| Headless passthrough runner | `src/main.rs` (`run_headless`) |
| PTY tests and ConPTY handshake assertion | `src/pty.rs` (`tests::spawn_and_read_output`) |

## What to read next (Phase 2)

1. `docs/plans/parser.md`
2. `vte` crate documentation on docs.rs — `vte::Parser` and `vte::Perform`
3. xterm control sequences ("ctlseqs") specification for CSI, SGR, C0, and OSC

Phase 2 goal: feed raw PTY bytes into the VT parser and emit structured `Handler`
trait events for terminal state updates.
