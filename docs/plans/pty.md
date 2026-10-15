# Plan: PTY

## Goal

Spawn a real shell over a portable PTY, stream bytes to the app, accept writes
and resizes, and report child exit. Phase 1 delivers a headless stdin/stdout
passthrough; later phases keep the same API behind `AppEvent`.

## Ownership

`src/pty.rs` — shell detection, spawn, reader thread, writer, resize, exit.

## Dependencies

- `portable-pty` (Phase 1)
- `anyhow` at the app boundary; `thiserror` for module errors

## Public surface (from contracts)

- `PtySize { rows, cols, px_w, px_h }`
- `ShellCommand` + default shell detection
- `spawn(cmd, size) -> Result<(PtyWriter, PtyReader)>`
- `PtyWriter`: `write(&[u8])`, `resize(PtySize)`, `kill()`
- `PtyReader`: background thread -> `AppEvent::PtyOutput` / channel in Phase 1

## Implementation order (Phase 1)

1. Add `portable-pty` and `anyhow`
2. `ShellCommand` + default shell (`$SHELL` / `/bin/sh`; Windows: pwsh → powershell → cmd)
3. Open PTY and spawn with initial size
4. Set `TERM=xterm-256color`, `COLORTERM=truecolor`
5. Reader thread forwarding bytes over a channel
6. Write path to the master
7. Resize support
8. Child exit and cleanup
9. Headless passthrough in `main`
10. Spawn-and-assert-output test
11. `docs/learn/phase-1.md`

## Platform notes

- Unix: classic PTY pair
- Windows: ConPTY via `portable-pty`
- Do not block the UI thread on reads (Phase 4+)

## Exit criteria

Running the binary in a normal terminal gives a working passthrough shell:
stdin → PTY, PTY → stdout, resize respected, clean exit.

## References

- `portable-pty` docs and WezTerm examples
- Alacritty/WezTerm for behavior reference only (no copy-paste)
