# Phase 0 — What We Set Up

## In plain language

Phase 0 does not make a terminal yet. It builds the **blueprint**: docs that say
what each piece should do, shared type contracts so modules agree on APIs, empty
stubs so the project compiles, and CI so every commit stays green.

Think of it like drawing the floor plan and putting up studs before plumbing.

## Key ideas

- **Terminal emulator vs shell vs PTY**
  - The *shell* (bash, pwsh) runs programs and prints bytes.
  - The *PTY* is a fake keyboard/screen pair so the shell thinks it has a real
    terminal.
  - The *emulator* (Oxidite) owns the window, parses those bytes, draws cells,
    and sends keystrokes back through the PTY.
- **Pipeline stages stay separate:** PTY → parser → grid → renderer, and
  separately input → PTY. Do not merge stages.
- **Contracts first:** `Handler` is the parser→terminal boundary. Changing it
  goes through a `contract/*` branch before features depend on it.

## Where to look in the code

| Thing | Path |
|---|---|
| Mission + phase plan | `AGENTS.md`, `docs/plan.md` |
| Shared APIs | `docs/contracts/shared-contracts.md` |
| Handler trait | `src/handler.rs` |
| Grid / color stubs | `src/grid.rs` |
| PTY stubs | `src/pty.rs` |
| App events | `src/app.rs` |
| CI | `.github/workflows/ci.yml` |

## What to read next (Phase 1)

1. `docs/plans/pty.md`
2. `portable-pty` crate docs on docs.rs — `PtySystem`, `CommandBuilder`,
   `MasterPty`
3. A simple WezTerm / `portable-pty` example that spawns a command

Phase 1 goal: a headless passthrough shell (stdin ↔ PTY ↔ stdout) with no GUI.
