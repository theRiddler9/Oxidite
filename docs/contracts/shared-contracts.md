# Shared Contracts

Cross-module types, traits, and events. Changes to these contracts go through a
`contract/*` branch and merge to `main` before dependent work continues.

Code stubs live in `src/` and must stay in sync with this document.

---

## Handler (`src/handler.rs`)

Parser -> terminal contract. The parser implements `vte::Perform` and translates
tokenized sequences into these method calls. The terminal implements `Handler`.
The parser must contain no grid logic.

```rust
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
    fn save_cursor(&mut self);
    fn restore_cursor(&mut self);
    fn reverse_index(&mut self);
    // extend as sequences are implemented
}
```

Supporting enums (stubs first, expand in Phase 2–3):

- `EraseMode` — CSI `J` / `K` modes (to end, from start, all, …)
- `SgrAttr` — reset, bold/dim/italic/underline/…, fg/bg colors
- `Mode` — autowrap, cursor visible, app cursor keys, bracketed paste, mouse, …

---

## Grid shapes (`src/grid.rs`)

```rust
pub enum Color {
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub attrs: Attrs,
    pub width: u8,
}

pub struct Cursor {
    pub row: usize,
    pub col: usize,
    pub visible: bool,
    pub shape: CursorShape,
}
```

`Attrs` is a bitflags set (bold, dim, italic, underline, inverse, strikethrough,
hidden). `CursorShape` is block / underline / bar.

---

## PTY (`src/pty.rs`)

```rust
pub struct PtySize {
    pub rows: u16,
    pub cols: u16,
    pub px_w: u16,
    pub px_h: u16,
}

pub fn spawn(cmd: ShellCommand, size: PtySize) -> Result<(PtyWriter, PtyReader)>;
// PtyWriter: write(&[u8]), resize(PtySize), kill()
// PtyReader: runs on its own thread, forwards bytes as AppEvent::PtyOutput
```

Environment for the child: `TERM=xterm-256color`, `COLORTERM=truecolor`.

---

## App events (`src/app.rs`)

Delivered via `winit`'s `EventLoopProxy` from background threads to the UI thread:

```rust
pub enum AppEvent {
    PtyOutput(Vec<u8>),
    PtyExited(Option<i32>),
    ConfigReloaded,
    ThemeChanged(String),
}
```

Rules:

- UI thread never blocks on PTY reads.
- Only the main thread mutates `Terminal`.
- Renderer redraws on demand (idle CPU near zero).
- Resize: window -> grid -> PTY, in that order, in one place.

---

## Input (`src/input.rs`)

```rust
pub fn encode_key(event: &KeyEvent, modes: &Modes) -> Option<Vec<u8>>;
```

Reads terminal modes; never mutates the grid.

---

## Theme (`src/theme.rs`)

```rust
pub struct Theme {
    pub name: String,
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
    pub selection_bg: Rgb,
    pub ansi: [Rgb; 16],
}
```

---

## Data flow

```
PTY reader thread
  -> AppEvent::PtyOutput(bytes) via EventLoopProxy
  -> main thread feeds bytes to Parser
  -> Parser calls Handler on Terminal
  -> Terminal marks damaged rows
  -> Renderer redraws on RedrawRequested

Window KeyboardInput
  -> Input::encode_key(event, terminal.modes)
  -> PtyWriter::write(bytes)
```
