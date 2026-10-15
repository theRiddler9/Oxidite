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

- [ ] Repo builds (`cargo check`)
- [ ] `.gitignore`, `rustfmt.toml`, clippy settings
- [ ] `AGENTS.md`
- [ ] `docs/plan.md` (this file)
- [ ] `docs/contracts/shared-contracts.md`
- [ ] `docs/plans/{pty,parser,grid,renderer,input,config}.md`
- [ ] Handler trait and shared type stubs in `src/`
- [ ] GitHub Actions CI (fmt, clippy, test)
- [ ] `docs/runbooks/demo.md`
- [ ] Tag: *(none for phase 0; first tag is `phase-1-pty`)*

### Phase 1 — PTY (headless, no GUI)

- [ ] `portable-pty` + `anyhow` dependencies
- [ ] Shell detection and spawn
- [ ] Reader thread + write path + resize + exit
- [ ] Headless passthrough mode
- [ ] PTY tests
- [ ] `docs/learn/phase-1.md`
- [ ] Tag: `phase-1-pty`

### Phase 2 — Parser

- [ ] `vte` dependency
- [ ] Parser wrapper + logging Handler
- [ ] Print, C0, CSI, SGR, DECSET/DECRST, OSC, ESC
- [ ] Fixture tests (including split reads)
- [ ] `docs/learn/phase-2.md`
- [ ] Tag: `phase-2-parser`

### Phase 3 — Terminal state (still headless)

- [ ] Cell / Grid / Cursor / scroll / scrollback / wide chars / resize
- [ ] Modes + alternate screen
- [ ] `Handler` for `Terminal` + damage tracking
- [ ] Headless grid dump + snapshot tests
- [ ] `docs/learn/phase-3.md`
- [ ] Tag: `phase-3-grid`

### Phase 4 — Window and basic rendering

- [ ] `winit` / `wgpu` / `glyphon` / `pollster`
- [ ] Window, AppEvent, GPU, fonts, cell bg, glyphs, cursor
- [ ] Resize -> grid -> PTY; PTY output -> parser -> redraw
- [ ] `docs/learn/phase-4.md`
- [ ] Tag: `phase-4-window`

### Phase 5 — Input

- [ ] Key encoding (printable, controls, arrows, mods, app cursor)
- [ ] Bracketed paste, mouse (SGR 1006), scrollback keys
- [ ] Selection + clipboard
- [ ] Table-driven input tests
- [ ] `docs/learn/phase-5.md`
- [ ] Tag: `phase-5-input`

### Phase 6 — Rendering polish

- [ ] Glyph cache, bold/italic, underline/strike, attrs, 256/truecolor
- [ ] Cursor shapes/blink, selection, damage redraw, HiDPI, zoom
- [ ] Wide/emoji, OSC title, visual bell, throughput
- [ ] `docs/learn/phase-6.md`
- [ ] Tag: `phase-6-polish`

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
