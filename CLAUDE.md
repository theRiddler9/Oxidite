# CLAUDE.md — Oxidite Project Guide & Developer Roadmap

## Project Overview

**Oxidite** is a fast, cross-platform GPU-accelerated terminal emulator and modern developer workspace written entirely in Rust (Edition 2024).

Oxidite is the **host window itself** (similar to Windows Terminal, Alacritty, or Ghostty) — not a TUI application inside another terminal, nor an Electron/webview wrapper. It hosts real shells over a PTY, tokenizes ANSI/VT escape sequences, maintains grid and scrollback state, renders on the GPU via `wgpu` and `glyphon`, encodes keyboard/mouse input back to the shell, and provides an integrated multi-pane developer workspace (tabs, diff review, git panel, workspace sidebar).

---

## Active Roadmap & Immediate Focus

Our current phased plan progresses through three focus areas in sequence:

### Focus 1: Compilation & Environment Setup (Immediate Priority)
- **Objective**: Ensure the local environment cleanly builds, lints, and tests the entire workspace.
- **Key Tasks**:
  - Resolve the Windows C++ linker requirement (`link.exe` / Visual Studio 2022 C++ Build Tools or configuring `x86_64-pc-windows-gnu` / MinGW toolchain).
  - Verify clean execution of:
    - `cargo check`
    - `cargo test`
    - `cargo fmt --check`
    - `cargo clippy -- -D warnings`
  - Ensure all dependencies (`wgpu 30`, `glyphon 0.12`, `winit 0.30`, `portable-pty 0.9`) compile with zero errors on the active toolchain.
  - Test binary launch in both headless mode (`cargo run -- --headless`) and interactive GPU window mode (`cargo run`).

### Focus 2: Core Feature Enhancements
- **Objective**: Expand terminal capabilities and advanced productivity features.
- **Key Tasks**:
  - **In-Terminal Search** `[DONE]`: Incremental scrollback and visible grid search with regex support (`regex 1.13`), amber/orange quad match highlighting, viewport auto-scrolling, navigation shortcuts (`Ctrl+Shift+F`, `F3`, `Shift+F3`), case toggle (`Alt+C`), regex toggle (`Alt+R`), and interactive floating search bar overlay.
  - **Pane Splitting**: Support vertical and horizontal split panes within tabs, allowing side-by-side terminal sessions.
  - **Shell Integration**: Parse OSC 133 sequences for command boundaries, prompts, and execution exit codes (click-to-copy output, prompt jumping).
  - **URL & Path Detection**: Auto-detect clickable hyperlinks (`OSC 8` and regex-based URL/path recognition) with hover underlines and click-to-open.
  - **Tabs Refinement**: Drag-and-drop tab reordering, customizable tab naming, and close-tab confirmations.

### Focus 3: Workspace Cockpit & UI/Diff Refinements
- **Objective**: Perfect the modern developer cockpit inspired by `DESIGN.md`.
- **Key Tasks**:
  - **Side-by-Side Diff Viewer**: Enhance file diff syntax highlighting, inline review annotations, and multi-file review flows.
  - **Git Changes Panel**: Polished stage/unstage toggle UX, interactive branch switching, remote fetch/pull status indicators.
  - **Theme & Appearance Customizer**: Rich live preview in the settings modal for user themes, font selection, and cursor styles.
  - **Splitter & Layout Smoothness**: Fluid resize handling across multi-pane layouts without ConPTY throttling or layout jitter.

---

## Core Pipeline & Architecture

```
Shell process (pwsh / bash / zsh)
  │
  ▼
PTY Master (portable-pty)
  │
  ▼ (raw byte stream, coalesced into up to 64KB batches)
Reader Thread
  │
  ▼ (AppEvent::TabPtyOutput via winit EventLoopProxy)
VT Parser (vte::Perform -> Handler trait)
  │
  ▼ (semantic method dispatch)
Terminal State (Grid, Cursor, Modes, Scrollback, Damage Tracking)
  │
  ▼ (damaged dirty rows only)
Renderer (wgpu + glyphon + custom WGSL quad pipeline)
  │
  ▼
Window (winit)
```

Input flow follows the reverse path:
```
Window Events (Keyboard / Mouse / Resize / Paste)
  │
  ▼
Input Encoder (input.rs -> ANSI escape bytes)
  │
  ▼
PTY Writer (Thread-safe Mutex)
  │
  ▼
Shell process
```

---

## Architectural Invariants & Rules

1. **Decoupled Stages**: Do not merge or skip stages. PTY $\rightarrow$ Parser $\rightarrow$ Grid $\rightarrow$ Renderer is fixed.
2. **Never Block the UI Thread**: PTY reads must occur exclusively on background threads. Only non-blocking event dispatch reaches the main loop.
3. **Single Mutator**: Only the UI main thread mutates `Terminal` and `Grid`.
4. **On-Demand Rendering**: Redraws must be event-driven; idle CPU usage must stay near 0%.
5. **Fixed Resize Order**: `Window size -> Grid dimensions -> PTY size`.
6. **No Fake Stubs**: Never fake Git status, fake terminal outputs, or fabricate agent responses.

---

## Workspace Layout & Key Modules

| Path | Primary Responsibility |
|---|---|
| [`src/main.rs`](src/main.rs) | CLI options (`--headless`, `--dump-grid`), application entry |
| [`src/app.rs`](src/app.rs) | `winit::application::ApplicationHandler`, multi-pane layout orchestration |
| [`src/pty.rs`](src/pty.rs) | `portable-pty` abstraction, shell detection, reader thread forwarder |
| [`src/parser.rs`](src/parser.rs) | `vte::Perform` implementation dispatching to `Handler` |
| [`src/handler.rs`](src/handler.rs) | Parser $\rightarrow$ Terminal contract trait |
| [`src/terminal.rs`](src/terminal.rs) | Modes, alternate screen buffer, dirty row tracking |
| [`src/grid.rs`](src/grid.rs) | Cell matrix, autowrap, wide characters, scrollback ring buffer |
| [`src/input.rs`](src/input.rs) | `KeyEvent` & `MouseEvent` encoding into ANSI escape sequences |
| [`src/renderer/`](src/renderer/) | `wgpu` surface, `glyphon` text rendering, quad shader pipeline |
| [`src/tab.rs`](src/tab.rs) | Multi-tab state, tab bar geometry, AI agent CLI detection |
| [`src/sidebar.rs`](src/sidebar.rs) | Collapsible workspace rail (repositories, branches, chat sessions) |
| [`src/diff_view.rs`](src/diff_view.rs) | Side-by-side code diff viewer, line gutters, review verdicts |
| [`src/changes_panel.rs`](src/changes_panel.rs) | Git changes list, staging toggles, commit composer |
| [`src/git.rs`](src/git.rs) | Native git integration (status, diff hunks, log, commit) |
| [`src/settings_modal.rs`](src/settings_modal.rs) | Settings modal overlay for theme, font, and cursor controls |
| [`src/status_bar.rs`](src/status_bar.rs) | Bottom status bar with session metrics and quick toggles |
| [`src/config.rs`](src/config.rs) | TOML configuration parser with `notify` hot-reload |
| [`src/theme.rs`](src/theme.rs) | Theme loader, built-in color presets, hex RGB parsing |

---

## Common Development Commands

### Building & Running
```bash
# Run with default shell in GUI window
cargo run

# Run with a specific shell or command
cargo run -- pwsh.exe -NoLogo
cargo run -- /bin/zsh -l

# Run in headless passthrough mode
cargo run -- --headless

# Run grid dump mode (for snapshots and testing)
cargo run -- --dump-grid

# Build optimized release binary
cargo build --release
```

### Verification & Quality Checks
```bash
# Compile check
cargo check

# Formatting check
cargo fmt --check

# Format in-place
cargo fmt

# Clippy linter (zero warnings policy)
cargo clippy -- -D warnings

# Run all unit and integration tests
cargo test

# Run property tests
cargo test parser::properties

# Run benchmarks
cargo bench

# Run parser fuzzer (e.g. 2,000 iterations)
cargo run --bin fuzz_parser -- --iterations 2000
```

---

## Coding Standards & Git Rules

- **Conventional Commits**: Format commit messages as `type(scope): summary` (e.g., `feat(tab): add split pane support`, `fix(renderer): handle HiDPI scaling`).
- **Feature Branches**: Work on feature branches (`feat/<topic>`, `fix/<topic>`). Never commit broken code directly to `main`.
- **Error Handling**: Use `thiserror` for library modules and `anyhow` in `main.rs`/entry points. No unhandled `unwrap()` or `expect()` in production code.
- **Safety**: No `unsafe` blocks without an explicit comment explaining soundness.
- **Module Size**: Keep modules clean and under ~500–600 lines where practical; extract submodules when components grow.
- **Dependencies**: Pin versions in `Cargo.toml`. Never add third-party crates outside the established tech stack without explicit evaluation.
