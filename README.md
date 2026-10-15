# Oxidite

A fast, cross-platform terminal emulator written entirely in Rust.

Oxidite is the **host window itself** (similar to Windows Terminal, Ghostty, or Alacritty). It is not a TUI application running inside an existing terminal, nor a wrapper around webviews or third-party terminal crates. It spawns a real shell over a PTY, parses the raw VT/ANSI byte stream, maintains its own terminal grid and scrollback, renders that grid in a GPU-accelerated window, and encodes user input back to the shell process.

---

## Architecture

The PTY -> Parser -> Grid -> Renderer pipeline is strictly modular and decoupled:

```
Shell process
  │
  ▼
PTY Master (portable-pty)
  │
  ▼ (raw byte stream)
Reader Thread
  │
  ▼ (chunks over channel)
VT Parser (vte::Perform)
  │
  ▼ (Handler trait calls)
Terminal State (Grid, Cursor, Modes, Scrollback)
  │
  ▼ (damage-tracked cell matrix)
Renderer (wgpu + glyphon)
  │
  ▼
Window (winit)
```

Input and window events follow the reverse path:

```
Window Events (Keyboard / Mouse / Resize / Paste)
  │
  ▼
Input Encoder (KeyEvent + Modes -> ANSI bytes)
  │
  ▼
PTY Writer
  │
  ▼
Shell process
```

---

## Build Phases & Roadmap

Oxidite is built in eight structured, verifiable phases:

| Phase | Milestone | Status | Description |
|---|---|:---:|---|
| **Phase 0** | Scaffold & Contracts | **Done** | Module boundaries, shared contracts (`Handler`), CI workflow, design docs |
| **Phase 1** | PTY Subsystem | **Done** | Shell detection, ConPTY/Unix spawn, non-blocking I/O, headless passthrough |
| **Phase 2** | VT/ANSI Parser | **In Progress** | `vte` wrapper, C0 controls, CSI cursor/erase, SGR colors, DEC modes |
| **Phase 3** | Terminal State | Planned | 2D cell grid, cursor clamping, scrollback ring buffer, alternate screen |
| **Phase 4** | Window & GPU Renderer | Planned | `winit` event loop, `wgpu` surface, `glyphon` text layout, GUI window |
| **Phase 5** | Input Encoding | Planned | Keyboard translation, application cursor keys, mouse reporting, clipboard |
| **Phase 6** | Rendering Polish | Planned | Glyph caching, bold/italic, underline/strikethrough, HiDPI, truecolor |
| **Phase 7** | Config & Themes | Planned | TOML configuration, live hot-reloading (`notify`), theme switching |
| **Phase 8** | Release & Hardening | Planned | Cross-platform verification, fuzzing, golden demo validation |

---

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (edition 2024, version 1.85+)
- Windows 10+ (ConPTY), Linux (X11/Wayland), or macOS

### Build & Run

Run with the default system shell (`pwsh.exe` / `powershell.exe` / `cmd.exe` on Windows, `$SHELL` on Unix):

```bash
cargo run
```

Run a specific command through the PTY:

```bash
cargo run -- -- cmd.exe /c echo "Hello from Oxidite!"
```

Display CLI options and help:

```bash
cargo run -- --help
```

Build optimized release binary:

```bash
cargo run --release
```

Run unit tests and verification checks:

```bash
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

---

## Writing a Theme

Oxidite uses standard TOML files for color theme definitions. Built-in presets (Dracula, Solarized, Gruvbox, Nord, One Dark) land in Phase 7, alongside user theme loading from the configuration directory.

Each theme specifies core UI colors (foreground, background, cursor, selection) and a complete 16-color ANSI palette:

```toml
name = "oxidite-dark"

foreground   = "#E8E8ED"
background   = "#0A0A0C"
cursor       = "#E8E8ED"
selection_bg = "#2A2A3D"

ansi = [
  "#1C1C22", #  0: Black
  "#FF5C7A", #  1: Red
  "#3DDC97", #  2: Green
  "#F5C542", #  3: Yellow
  "#6EA8FF", #  4: Blue
  "#C792EA", #  5: Magenta
  "#64D2E8", #  6: Cyan
  "#E8E8ED", #  7: White
  "#5A5A66", #  8: Bright Black
  "#FF7A94", #  9: Bright Red
  "#5AEBB0", # 10: Bright Green
  "#FFD56A", # 11: Bright Yellow
  "#8FC0FF", # 12: Bright Blue
  "#D9A8FF", # 13: Bright Magenta
  "#8AE0F0", # 14: Bright Cyan
  "#FFFFFF", # 15: Bright White
]
```

---

## Tech Stack

| Component | Technology / Crate |
|---|---|
| **PTY** | `portable-pty` |
| **VT Tokenizer** | `vte` |
| **Windowing & Events** | `winit` |
| **GPU Rendering** | `wgpu` |
| **Text Layout & Glyphs** | `glyphon` |
| **Unicode Width** | `unicode-width` |
| **Config & Themes** | `toml`, `serde` |
| **Hot Reload** | `notify` |
| **Clipboard** | `arboard` |
| **Error Handling** | `thiserror` (internal), `anyhow` (application boundary) |

---

## License

This project is licensed under the [MIT License](LICENSE).
