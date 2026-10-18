# Oxidite — Architecture Plan

## Overview

Oxidite is a cross-platform terminal emulator written in Rust. It is the host
window itself (like Windows Terminal or iTerm2), not a TUI app inside another
terminal.

Fixed pipeline (do not merge or skip stages):

```
Shell -> PTY -> Reader thread -> VT Parser -> Terminal State -> Renderer -> Window
Window events -> Input Encoder -> PTY writer -> Shell
```

See `AGENTS.md` for the full mission, non-goals, tech stack, and coding standards.

## Component Map

| Component | Path | Role |
|---|---|---|
| PTY | `src/pty.rs` | Spawn shell, read/write/resize, exit |
| Parser | `src/parser.rs` | `vte::Perform` -> `Handler` calls |
| Handler | `src/handler.rs` | Parser -> terminal contract |
| Terminal | `src/terminal.rs` | Modes, alt screen, implements `Handler` |
| Grid | `src/grid.rs` | Cells, cursor, scrollback, damage |
| Renderer | `src/renderer/` | Window, GPU, fonts, draw |
| Input | `src/input.rs` | Keys/mouse/paste -> PTY bytes |
| Config | `src/config.rs` | TOML config, hot reload |
| Theme | `src/theme.rs` | Color palettes |
| App | `src/main.rs`, `src/app.rs` | Event loop wiring |

## Phase Checklist

### Phase 0 — Scaffold, docs, contracts

- [x] Repo builds (`cargo check`)
- [x] `.gitignore`, `rustfmt.toml`, clippy settings
- [x] `AGENTS.md`
- [x] `docs/plan.md` (this file)
- [x] `docs/contracts/shared-contracts.md`
- [x] `docs/plans/{pty,parser,grid,renderer,input,config}.md`
- [x] Handler trait and shared type stubs in `src/`
- [x] GitHub Actions CI (fmt, clippy, test)
- [x] `docs/runbooks/demo.md`
- [x] Tag: *(none for phase 0; first tag is `phase-1-pty`)*

### Phase 1 — PTY (headless, no GUI)

- [x] `portable-pty` + `anyhow` dependencies
- [x] Shell detection and spawn
- [x] Reader thread + write path + resize + exit
- [x] Headless passthrough mode
- [x] PTY tests
- [x] `docs/learn/phase-1.md`
- [x] Tag: `phase-1-pty`

### Phase 2 — Parser

- [x] `vte` dependency
- [x] Parser wrapper + logging Handler
- [x] Print, C0, CSI, SGR, DECSET/DECRST, OSC, ESC
- [x] Fixture tests (including split reads)
- [x] `docs/learn/phase-2.md`
- [x] Tag: `phase-2-parser`

### Phase 3 — Terminal state (still headless)

- [x] Cell / Grid / Cursor / scroll / scrollback / wide chars / resize
- [x] Modes + alternate screen
- [x] `Handler` for `Terminal` + damage tracking
- [x] Headless grid dump + snapshot tests
- [x] `docs/learn/phase-3.md`
- [x] Tag: `phase-3-grid`

### Phase 4 — Window and basic rendering

- [x] `winit` / `wgpu` / `glyphon` / `pollster`
- [x] Window, AppEvent, GPU, fonts, cell bg, glyphs, cursor
- [x] Resize -> grid -> PTY; PTY output -> parser -> redraw
- [x] `docs/learn/phase-4.md`
- [x] Tag: `phase-4-window`

### Phase 5 — Input

- [x] Key encoding (printable, controls, arrows, mods, app cursor)
- [x] Bracketed paste, mouse (SGR 1006), scrollback keys
- [x] Selection + clipboard
- [x] Table-driven input tests
- [x] `docs/learn/phase-5.md`
- [x] Tag: `phase-5-input`

### Phase 6 — Rendering polish

- [x] Glyph cache, bold/italic, underline/strike, attrs, 256/truecolor
- [x] Cursor shapes/blink, selection, damage redraw, HiDPI, zoom
- [x] Wide/emoji, OSC title, visual bell, throughput
- [x] `docs/learn/phase-6.md`
- [x] Tag: `phase-6-polish`

### Phase 7 — Config and themes

- [ ] Theme struct + built-in presets
- [ ] Config load, options, keybindings, hot reload
- [ ] Live theme switch + invalid-config fallback
- [ ] Config tests + format docs
- [ ] `docs/learn/phase-7.md`
- [ ] Tag: `phase-7-config`

### Phase 8 — Hardening and release

- [ ] Property/fuzz tests, throughput bench
- [ ] Platform fixes, CI matrix, release workflow
- [ ] README, architecture diagram, LICENSE
- [ ] Golden demo passes
- [ ] `docs/learn/phase-8.md`
- [ ] Tag: `phase-8-release`

## Integration Notes

See `docs/integration.md` for merge checkpoints and blockers.
See `docs/contracts/shared-contracts.md` for shared types and traits.
See per-component plans under `docs/plans/`.
