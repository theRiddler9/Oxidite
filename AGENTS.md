# Oxidite

## Mission

Build **Oxidite**, a cross-platform terminal emulator written entirely in Rust.

Oxidite is the host window itself, like Windows Terminal or iTerm2. It is not a TUI app running inside another terminal.

The system must spawn a real shell over a PTY, parse the VT/ANSI byte stream, maintain its own terminal grid state, render that grid in a GPU-backed window, translate keyboard and mouse input back into bytes, and support theme switching through TOML files.

## Non-Goals

Do not reduce the project to:

- a TUI app built with `ratatui` / `crossterm`
- a web, Electron, Tauri, or webview app
- a wrapper around `alacritty_terminal`, `egui_term`, `wezterm-term`, or any crate that already implements the parser + grid for us
- a program that launches or embeds another terminal
- a fake shell (Oxidite never interprets commands; it hosts a real shell)

Allowed crates handle PTY spawning, byte tokenizing, windowing, glyph layout, and config parsing. The terminal state, escape-sequence semantics, input encoding, renderer, and themes are written in this repository.

A human will study this code later. Every non-trivial piece must be readable and explainable.

## Core Pipeline

```
Shell process
  -> PTY (portable-pty master)
  -> Reader thread (raw bytes)
  -> VT Parser (vte::Perform)
  -> Terminal State (Grid, Cursor, Scrollback, Modes)
  -> Renderer (wgpu + glyphon)
  -> Window (winit)

Window events (keyboard / mouse / resize / paste)
  -> Input Encoder
  -> PTY writer
  -> Shell process
```

The PTY -> parse -> grid -> render architecture is fixed. Do not merge stages or skip one.

## Shared Source of Truth

Read these before implementation. If a file does not exist yet, create it in Phase 0.

- `docs/plan.md` — overall architecture, phase checklist, and integration status
- `docs/contracts/shared-contracts.md` — cross-module types, traits, and events
- `docs/plans/pty.md` — PTY component plan
- `docs/plans/parser.md` — parser component plan
- `docs/plans/grid.md` — terminal state component plan
- `docs/plans/renderer.md` — window and rendering component plan
- `docs/plans/input.md` — input encoding component plan
- `docs/plans/config.md` — config and theme component plan
- `docs/integration.md` — merge and integration checkpoints
- `docs/runbooks/demo.md` — golden demo definition
- `docs/learn/phase-N.md` — plain-language notes written after each phase

External references to follow when implementing behavior:

- xterm control sequences ("ctlseqs") — the spec for what escape codes mean
- `vte` crate docs and source
- `portable-pty` docs and the WezTerm repo examples
- `winit`, `wgpu`, `glyphon` docs
- Alacritty and WezTerm source — reference for behavior, never copy-paste

## Tech Stack

| Concern | Crate |
|---|---|
| PTY | `portable-pty` |
| Escape-sequence tokenizer | `vte` |
| Window + event loop | `winit` |
| GPU | `wgpu` |
| Text layout + glyphs | `glyphon` |
| Unicode width | `unicode-width` |
| Config | `serde`, `toml` |
| Hot reload | `notify` |
| Config paths | `dirs` |
| Clipboard | `arboard` |
| Errors | `thiserror` (modules), `anyhow` (main) |
| Logging | `log`, `env_logger` |
| Bitflags | `bitflags` |

Rules:

- `winit`, `wgpu`, and `glyphon` versions must be mutually compatible. Check what `glyphon` requires before pinning.
- Ask before adding any dependency not listed here.
- Pin versions in `Cargo.toml`. Commit `Cargo.lock`.

## Package Boundaries

Any contributor may work anywhere in the repository. Keep changes coherent with these component boundaries, but do not treat them as exclusive ownership.

- **PTY** (`src/pty.rs`): shell detection, spawn, reader thread, writer, resize, child exit handling.
- **Parser** (`src/parser.rs`): `vte::Perform` implementation that translates sequences into calls on a `Handler` trait. Contains no grid logic.
- **Terminal state** (`src/grid.rs`, `src/terminal.rs`): cells, cursor, scroll regions, scrollback, alternate screen, modes, title. Implements `Handler`.
- **Renderer** (`src/renderer/`): window creation, GPU setup, font metrics, drawing grid + cursor + selection. Reads terminal state, never mutates it.
- **Input** (`src/input.rs`): key, mouse, and paste events -> bytes for the PTY. Reads terminal modes, never mutates the grid.
- **Config + Theme** (`src/config.rs`, `src/theme.rs`, `themes/`): TOML loading, defaults, built-in presets, hot reload, keybindings.
- **App** (`src/main.rs`, `src/app.rs`): wires everything together and owns the event loop. Stays small.

## Repository Layout

```
oxidite/
├── AGENTS.md
├── Cargo.toml
├── Cargo.lock
├── README.md
├── rustfmt.toml
├── .gitignore
├── .github/workflows/ci.yml
├── docs/
│   ├── plan.md
│   ├── integration.md
│   ├── contracts/shared-contracts.md
│   ├── plans/{pty,parser,grid,renderer,input,config}.md
│   ├── runbooks/demo.md
│   └── learn/phase-N.md
├── themes/
│   ├── dracula.toml
│   ├── solarized-dark.toml
│   ├── solarized-light.toml
│   ├── gruvbox-dark.toml
│   ├── nord.toml
│   └── one-dark.toml
├── tests/
│   ├── fixtures/            # captured byte streams for parser/grid tests
│   └── *.rs
└── src/
    ├── main.rs
    ├── app.rs
    ├── pty.rs
    ├── parser.rs
    ├── handler.rs           # the Handler trait (parser -> terminal contract)
    ├── terminal.rs
    ├── grid.rs
    ├── input.rs
    ├── config.rs
    ├── theme.rs
    └── renderer/
        ├── mod.rs
        ├── window.rs
        ├── gpu.rs
        ├── font.rs
        └── draw.rs
```

## Shared Contracts

These are the starting contracts. Write them in `docs/contracts/shared-contracts.md` and as code stubs first. Any change to a contract goes through its own `contract/*` branch and merges before dependent work continues.

```rust
// handler.rs — parser -> terminal contract
pub trait Handler {
    fn print(&mut self, c: char);
    fn linefeed(&mut self);
    fn carriage_return(&mut self);
    fn backspace(&mut self);
    fn tab(&mut self);
    fn bell(&mut self);
    fn move_cursor_to(&mut self, row: usize, col: usize);
    fn move_cursor_by(&mut self, d_row: isize, d_col: isize);
    fn erase_in_display(&mut self, mode: EraseMode);
    fn erase_in_line(&mut self, mode: EraseMode);
    fn set_sgr(&mut self, attr: SgrAttr);
    fn set_mode(&mut self, mode: Mode, enabled: bool);
    fn set_scroll_region(&mut self, top: usize, bottom: usize);
    fn set_title(&mut self, title: String);
    // extend as sequences are implemented
}

// grid.rs — terminal state shapes
pub enum Color { Default, Indexed(u8), Rgb(u8, u8, u8) }
pub struct Cell { pub ch: char, pub fg: Color, pub bg: Color, pub attrs: Attrs, pub width: u8 }
pub struct Cursor { pub row: usize, pub col: usize, pub visible: bool, pub shape: CursorShape }

// pty.rs — PTY contract
pub struct PtySize { pub rows: u16, pub cols: u16, pub px_w: u16, pub px_h: u16 }
pub fn spawn(cmd: ShellCommand, size: PtySize) -> Result<(PtyWriter, PtyReader)>;
// PtyWriter: write(&[u8]), resize(PtySize), kill()
// PtyReader: runs on its own thread, forwards bytes as AppEvent::PtyOutput

// app.rs — cross-thread events delivered via winit's EventLoopProxy
pub enum AppEvent {
    PtyOutput(Vec<u8>),
    PtyExited(Option<i32>),
    ConfigReloaded,
    ThemeChanged(String),
}

// input.rs — input contract
pub fn encode_key(event: &KeyEvent, modes: &Modes) -> Option<Vec<u8>>;

// theme.rs — theme contract
pub struct Theme {
    pub name: String,
    pub foreground: Rgb, pub background: Rgb,
    pub cursor: Rgb, pub selection_bg: Rgb,
    pub ansi: [Rgb; 16],
}
```

## Data Flow Boundary

```
PTY reader thread (blocking read)
  -> AppEvent::PtyOutput(bytes) via EventLoopProxy
  -> main thread feeds bytes to Parser
  -> Parser calls Handler on Terminal
  -> Terminal marks damaged rows
  -> Renderer redraws on RedrawRequested

Window KeyboardInput
  -> Input::encode_key(event, terminal.modes)
  -> PtyWriter::write(bytes)
```

Rules:

- The UI thread never blocks on PTY reads.
- Only the main thread mutates `Terminal`.
- The renderer redraws on demand, not in a busy loop. Idle CPU should be near zero.
- Resize flows window -> grid -> PTY, in that order, in one place.

## v1 Terminal Feature Scope

Must work in v1:

- printable text, UTF-8, wide characters (CJK, emoji), combining marks degrade gracefully
- C0 controls: LF, CR, BS, HT, BEL
- cursor movement and positioning, save/restore cursor
- erase in display / line, insert/delete lines and characters
- SGR: bold, dim, italic, underline, inverse, strikethrough, 16 / 256 / truecolor
- scroll regions, scrollback buffer, alternate screen buffer (required for vim, htop, less)
- modes: autowrap, cursor visibility, application cursor keys, bracketed paste, basic mouse reporting (SGR 1006)
- OSC 0 / 2 window title
- resize with correct PTY notification
- selection, copy, paste
- environment: `TERM=xterm-256color`, `COLORTERM=truecolor`

Stretch (only after the golden demo passes): tabs, split panes, search, URL detection, font ligatures, inline images, shell integration marks.

## Build Phases

Work through the phases in order. Each phase ends with a tag, a `docs/learn/phase-N.md`, and passing checks. Do not start a phase until the previous one meets its exit criteria. A phase with fewer than 8 commits is under-committed.

### Phase 0 — Scaffold, docs, contracts

Exit: repo builds, CI is green, all docs and contract stubs exist.

Commits:

1. `chore: initialize cargo project`
2. `chore: add .gitignore, rustfmt.toml, and clippy settings`
3. `docs: add AGENTS.md`
4. `docs: add plan.md with phase checklist`
5. `docs(contracts): add shared-contracts.md`
6. `docs(plans): add per-component plan files`
7. `feat(contracts): add Handler trait and shared type stubs`
8. `chore(ci): add GitHub Actions for fmt, clippy, and test`
9. `docs(runbook): add golden demo definition`

### Phase 1 — PTY (headless, no GUI)

Exit: running the binary in a normal terminal gives a working passthrough shell: stdin -> PTY, PTY -> stdout, resize respected, clean exit.

Commits:

1. `chore(deps): add portable-pty and anyhow`
2. `feat(pty): add ShellCommand and default shell detection`
3. `feat(pty): spawn shell on a PTY with initial size`
4. `feat(pty): set TERM and COLORTERM in the child environment`
5. `feat(pty): add reader thread forwarding bytes over a channel`
6. `feat(pty): add write path to the PTY master`
7. `feat(pty): add resize support`
8. `feat(pty): handle child exit and cleanup`
9. `feat(main): add headless passthrough mode`
10. `test(pty): spawn a command and assert its output`
11. `docs(learn): add phase 1 notes`

### Phase 2 — Parser

Exit: raw PTY bytes produce a correct stream of `Handler` calls, verified by a logging handler and fixture tests.

Commits:

1. `chore(deps): add vte`
2. `feat(parser): add Parser wrapper around vte::Parser`
3. `feat(parser): add logging Handler for debugging`
4. `feat(parser): handle print and C0 controls`
5. `feat(parser): handle CSI cursor movement`
6. `feat(parser): handle CSI erase in display and line`
7. `feat(parser): handle SGR 16-color attributes`
8. `feat(parser): handle SGR 256-color and truecolor`
9. `feat(parser): handle SGR text styles`
10. `feat(parser): handle DECSET / DECRST modes`
11. `feat(parser): handle scroll region (DECSTBM)`
12. `feat(parser): handle OSC title sequences`
13. `feat(parser): handle ESC sequences (save/restore cursor, reverse index)`
14. `feat(parser): handle insert/delete line and character`
15. `test(parser): add byte-fixture tests for each sequence family`
16. `test(parser): verify sequences split across reads`
17. `docs(learn): add phase 2 notes`

### Phase 3 — Terminal state (still headless)

Exit: feeding captured byte streams produces a grid that matches expected snapshots, including alternate screen and scrollback.

Commits:

1. `feat(grid): add Color, Attrs, and Cell types`
2. `feat(grid): add Grid storage and indexing`
3. `feat(grid): add put_char with autowrap`
4. `feat(grid): add cursor movement with clamping`
5. `feat(grid): add erase operations`
6. `feat(grid): add scroll up/down`
7. `feat(grid): add scroll regions`
8. `feat(grid): add scrollback ring buffer`
9. `feat(grid): add wide character handling`
10. `feat(grid): add resize behavior`
11. `feat(terminal): add Modes struct`
12. `feat(terminal): add alternate screen buffer`
13. `feat(terminal): implement Handler for Terminal`
14. `feat(terminal): add damage tracking for dirty rows`
15. `feat(main): add headless grid dump mode`
16. `test(grid): add snapshot tests from fixtures`
17. `docs(learn): add phase 3 notes`

### Phase 4 — Window and basic rendering

Exit: a window opens, the shell runs, and live output appears as monospace text with colors. Ugly is acceptable.

Commits:

1. `chore(deps): add winit, wgpu, glyphon, pollster`
2. `feat(window): create window and event loop`
3. `feat(app): add AppEvent and EventLoopProxy plumbing`
4. `feat(renderer): initialize wgpu surface and device`
5. `feat(renderer): load monospace font and measure cell size`
6. `feat(renderer): draw cell background quads`
7. `feat(renderer): draw glyphs through glyphon`
8. `feat(renderer): draw cursor`
9. `feat(window): map window size to rows and cols`
10. `feat(app): propagate resize to grid and PTY`
11. `feat(app): wire PTY output into parser and request redraw`
12. `docs(learn): add phase 4 notes`

### Phase 5 — Input

Exit: the shell is fully usable, including line editing, history, and vim.

Commits:

1. `feat(input): encode printable text`
2. `feat(input): encode Enter, Tab, Backspace, Escape`
3. `feat(input): encode Ctrl+letter`
4. `feat(input): encode arrows, Home, End, PageUp, PageDown`
5. `feat(input): honor application cursor key mode`
6. `feat(input): encode function keys`
7. `feat(input): encode Alt as ESC prefix`
8. `feat(input): encode modified keys (CSI 1;mod)`
9. `feat(input): add bracketed paste`
10. `feat(input): add mouse reporting (SGR 1006)`
11. `feat(input): add scrollback keybindings`
12. `feat(input): add text selection with the mouse`
13. `feat(input): add clipboard copy and paste`
14. `test(input): table-driven tests for key encoding`
15. `docs(learn): add phase 5 notes`

### Phase 6 — Rendering polish

Exit: output looks like a real terminal and stays smooth under load.

Commits:

1. `feat(renderer): add glyph cache`
2. `feat(renderer): add bold and italic font variants`
3. `feat(renderer): draw underline and strikethrough`
4. `feat(renderer): support inverse, dim, and hidden attributes`
5. `feat(renderer): support the full 256-color palette and truecolor`
6. `feat(renderer): add cursor shapes and blink timer`
7. `feat(renderer): draw selection highlight`
8. `feat(renderer): redraw only damaged regions`
9. `feat(renderer): handle HiDPI scale factor`
10. `feat(renderer): add font zoom keybindings`
11. `feat(renderer): render wide characters and emoji`
12. `feat(app): apply OSC title to the window`
13. `feat(app): add visual bell`
14. `perf(renderer): profile and fix large-output throughput`
15. `docs(learn): add phase 6 notes`

### Phase 7 — Config and themes

Exit: themes switch live, config edits hot-reload, bad config never crashes the app.

Commits:

1. `feat(theme): add Theme struct and Rgb parsing`
2. `feat(theme): add built-in dracula preset`
3. `feat(theme): add built-in solarized-dark and solarized-light presets`
4. `feat(theme): add built-in gruvbox, nord, and one-dark presets`
5. `feat(config): resolve config directory per platform`
6. `feat(config): load config.toml with serde defaults`
7. `feat(config): load themes from the themes directory`
8. `feat(config): add font, size, padding, shell, and scrollback options`
9. `feat(config): add configurable keybindings`
10. `feat(config): hot reload with notify`
11. `feat(app): add theme cycle keybinding and live theme switching`
12. `feat(config): fall back to defaults on invalid config and log the error`
13. `test(config): add parsing and fallback tests`
14. `docs: document the config and theme format`
15. `docs(learn): add phase 7 notes`

### Phase 8 — Hardening and release

Exit: the golden demo passes on macOS, Linux, and Windows, and the project is presentable.

Commits:

1. `test(parser): add property tests for arbitrary byte input`
2. `test(parser): add a fuzz target`
3. `perf: add a throughput benchmark (cat of a large file)`
4. `fix(windows): handle ConPTY quirks`
5. `fix(linux): handle X11 and Wayland differences`
6. `chore(ci): add macOS, Linux, and Windows build matrix`
7. `chore(ci): add release workflow`
8. `docs: write README with install steps and screenshots`
9. `docs: add architecture diagram`
10. `chore: add LICENSE`
11. `docs(learn): add final notes`

## Git Rules

- Work on separate feature branches. Never commit directly to `main`.
- Make small logical commits.
- Merge shared contracts before dependent work.
- Pull/rebase shared changes before integration.
- Avoid destructive Git operations.

### Branches

- Naming: `feat/<topic>`, `fix/<topic>`, `docs/<topic>`, `test/<topic>`, `chore/<topic>`, `refactor/<topic>`, `perf/<topic>`, `contract/<topic>`.
- One branch per feature, for example `feat/pty-spawn`, `feat/parser-sgr`, `feat/grid-scrollback`.
- Branches are short-lived. Merge and delete them once finished.
- Contract changes get their own `contract/<topic>` branch and merge to `main` before any dependent branch continues.

### Commits

- Use Conventional Commits: `type(scope): summary`. Scope is the module (`pty`, `parser`, `grid`, `terminal`, `renderer`, `input`, `config`, `theme`, `app`, `ci`, `docs`).
- Summary is imperative, lowercase, under 72 characters.
- Add a body when the reason is not obvious, especially for protocol behavior (cite the ctlseqs sequence name, e.g. `CSI Ps J (ED)`).
- Commit frequently. Make at least one commit for each of:
  - a new file
  - a new public type or trait
  - a new escape-sequence family
  - a new keybinding group
  - a new config option
  - a new test file
  - a docs update
- Keep a commit under roughly 150 changed lines and one concern.
- Do not mix refactoring with feature work. Do not mix formatting changes with logic changes.
- Every commit must pass `cargo check`. Do not leave broken intermediate commits.
- Commit tests with their feature or immediately after, as `test(scope): ...`.
- Never commit `target/`, `.env`, editor settings, or machine-specific paths.

### Integration

```
git switch main && git pull --ff-only
git switch -c feat/pty-spawn

# work in small steps, commit after each logical change

git fetch origin
git rebase origin/main
cargo fmt --check && cargo clippy -- -D warnings && cargo test

git push -u origin feat/pty-spawn
# open a PR; merge with a merge commit (no squash, history is the progress log)
```

- Before opening a PR, rebase on the latest `main`.
- Merge with merge commits. Do not squash; the commit trail is part of the deliverable.
- Tag each finished phase: `git tag phase-1-pty`, `phase-2-parser`, `phase-3-grid`, `phase-4-window`, `phase-5-input`, `phase-6-polish`, `phase-7-config`, `phase-8-release`.

### Destructive operations

Do not run these without explicit human approval:

- `git push --force` (on your own feature branch after a rebase, `--force-with-lease` is the only acceptable form)
- `git reset --hard`
- `git clean -fd`
- `git checkout .` or `git restore .` on uncommitted work
- `git branch -D` on an unmerged branch
- `git rebase` or any history rewrite on `main` or on a branch others use
- `git commit --amend` on a commit that is already pushed

If unsure, create a backup branch first: `git branch backup/<name>`.

## Coding Standards

- `cargo fmt` and `cargo clippy -- -D warnings` must pass.
- No `unwrap()` / `expect()` outside tests and startup code in `main`. Use `Result` with `thiserror` types in modules and `anyhow` in `main`.
- No `unsafe` without a comment explaining why it is sound.
- Never block the UI thread.
- Avoid per-frame allocation in the render path.
- Use `log` macros, not `println!`, except in the explicit headless debug modes.
- Keep modules under about 500 lines; split when they grow.
- Public items get doc comments. Protocol-specific code gets comments explaining why, citing the relevant ctlseqs section.
- Parser and grid logic is tested with byte fixtures in `tests/fixtures/` and snapshot-style assertions.

Platforms: macOS, Linux (X11 and Wayland), Windows 10+ (ConPTY). Default shell: `$SHELL`, falling back to `/bin/sh`; on Windows, `pwsh.exe`, then `powershell.exe`, then `cmd.exe`.

Performance targets:

- `cat` of a very large file does not freeze the window
- idle CPU usage is near zero
- scrolling stays smooth

## Agent Working Agreement

1. Identify the current phase from `docs/plan.md` and `git tag`. Do not skip phases or combine them.
2. Read the relevant docs and contracts before writing code.
3. State the plan first: list the commits you intend to make for this task.
4. Implement the smallest slice, verify it, commit it, and repeat.
5. After each task run `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test`, then show `git status` and `git log --oneline -n 15`.
6. Tick the phase checklist in `docs/plan.md` as part of the relevant commit.
7. At the end of every phase write `docs/learn/phase-N.md` in plain language: what the component does, the key concepts (PTY, CSI, SGR, alternate screen, and so on), what to read next, and where in the code to look. The human will study these later.
8. If blocked, record the blocker in `docs/integration.md` and ask. Do not silently work around a contract.
9. Ask before adding a dependency outside the tech stack table.
10. When behavior is unclear, match what Alacritty or WezTerm does and note the reference in a comment.

## Golden Demo

The full definition lives in `docs/runbooks/demo.md`. The project is demo-ready when all of this works on a release build:

1. `cargo run --release` opens a window with the user's shell prompt in the default theme.
2. `ls --color=auto` (`ls -G` on macOS) shows correct colors.
3. `vim` or `nvim` enters the alternate screen and restores the previous screen on exit.
4. `htop` or `top` renders correctly and reflows when the window is resized.
5. `seq 1 100000` completes without freezing, and scrollback works with Shift+PageUp.
6. Mouse selection, copy, and paste work.
7. The theme-cycle keybinding changes colors instantly, and editing a theme TOML hot-reloads it.
8. `printf '\e[38;2;255;100;0mtruecolor\e[0m\n'` shows orange text, and `echo "日本語 🦀"` renders correctly.
9. Exiting the shell closes the window or shows a clear "process exited" state.

## Definition of Done

- All eight phases are tagged and merged to `main`.
- CI is green on macOS, Linux, and Windows.
- The golden demo passes.
- `README.md` explains what Oxidite is, how to build and run it, and how to write a theme.
- Every phase has a `docs/learn/phase-N.md`.
- The Git history shows small, logical commits for every stage, file, and feature.
