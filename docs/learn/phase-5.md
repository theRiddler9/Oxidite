# Phase 5 — Input Encoding & Clipboard Integration

## In plain language

In Phase 5, Oxidite becomes fully interactive. A terminal emulator is fundamentally bidirectional:
while Phase 4 established the output pipeline (PTY -> Parser -> Grid -> GPU Window), Phase 5
establishes the complete input pipeline (Window Events -> Input Encoder -> PTY Master -> Shell).

When a user strikes a key, drags a mouse, or pastes text, the operating system generates window events.
A shell process running inside a terminal does not understand GUI event objects; it expects an
asynchronous stream of raw bytes conforming to VT/ANSI/xterm terminal protocols. Phase 5 implements
the complete translation of keyboard inputs, modifier combinations, mouse gestures, bracketed paste,
and clipboard interactions into these precise protocol bytes.

## Key ideas

### 1. The Input Pipeline
```
Window Event (winit)
  │
  ├── ModifiersChanged -> Track active Shift, Ctrl, Alt, Super modifiers
  │
  ├── KeyboardInput
  │     ├── check_input_action (Internal terminal actions)
  │     │     ├── Shift+PageUp / Shift+PageDown -> Scroll viewport into scrollback
  │     │     ├── Shift+Home / Shift+End -> Scroll to oldest history / bottom prompt
  │     │     ├── Ctrl+Shift+C / Cmd+C -> Copy selected text to system clipboard
  │     │     └── Ctrl+Shift+V / Cmd+V -> Read clipboard & paste with bracketed paste
  │     │
  │     └── encode_key (PTY-bound key events)
  │           ├── Printable characters & UTF-8 text (e.g. 'a', '🦀')
  │           ├── C0 controls (Enter -> \r, Tab -> \t, Backspace -> \x7f, Escape -> \x1b)
  │           ├── Ctrl + Letter/Symbol (e.g. Ctrl+C -> 0x03, Ctrl+D -> 0x04)
  │           ├── Alt as ESC prefix (e.g. Alt+b -> \x1bb, Alt+f -> \x1bf)
  │           ├── Navigation keys (Arrows, Home, End, PageUp, PageDown)
  │           ├── Application cursor keys mode (DECCKM -> \x1bOA..\x1bOD)
  │           ├── Function keys (F1..F12 -> VT220/xterm sequences)
  │           └── Modified keys (CSI 1;mod -> \x1b[1;{mod}A..\x1b[1;{mod}F)
  │
  ├── CursorMoved & MouseInput (when mouse reporting is disabled)
  │     └── Drag-to-select text on the grid, updating Selection bounds
  │
  ├── MouseWheel (when mouse reporting is disabled)
  │     └── Scroll viewport display offset up or down through scrollback history
  │
  └── MouseInput & MouseWheel (when mouse reporting is enabled via DECSET 1006)
        └── encode_mouse_event -> SGR 1006 sequence: \x1b[<{btn};{col};{row}{M|m}
```

### 2. Standard ASCII Control Codes (C0 Controls & Ctrl+Letter)
In ASCII, Control codes occupy values `0x00` through `0x1F`:
- Standard terminal Enter sends carriage return `\r` (`0x0D`), not `\n`.
- Tab sends `\t` (`0x09`); Shift+Tab sends the ANSI backtab sequence `\x1b[Z`.
- Backspace sends `\x7f` (`DEL`), matching standard Unix tty defaults.
- Escape sends `\x1b` (`0x1B`).
- Pressing `Ctrl` with a letter maps that letter to its 1-indexed alphabetical ASCII control code:
  `Ctrl+A` is `0x01`, `Ctrl+C` is `0x03` (`SIGINT`), `Ctrl+D` is `0x04` (`EOF`), `Ctrl+Z` is `0x1A` (`SUSP`).
  The algorithm subtracts `0x40` from the uppercase character byte.

### 3. Application Cursor Keys Mode (DECCKM)
When text editors like `vim`, `nano`, or `less` run, they frequently enable Application Cursor Key Mode
via escape sequence `\x1b[?1h`. In this mode:
- Normal cursor mode: `Up` -> `\x1b[A`, `Down` -> `\x1b[B`, `Right` -> `\x1b[C`, `Left` -> `\x1b[D`.
- Application cursor mode: `Up` -> `\x1bOA`, `Down` -> `\x1bOB`, `Right` -> `\x1bOC`, `Left` -> `\x1bOD`.
- Home and End similarly transition to `\x1bOH` and `\x1bOF`.

### 4. Modified Keys Protocol (`CSI 1;mod`)
When navigation or function keys are combined with modifiers (Shift, Alt, Ctrl), terminals encode
the active modifiers as a numeric parameter:
$$\text{mod} = 1 + \text{Shift}(1) + \text{Alt}(2) + \text{Ctrl}(4)$$
- Shift only: `1 + 1 = 2`
- Alt only: `1 + 2 = 3`
- Shift + Alt: `1 + 1 + 2 = 4`
- Ctrl only: `1 + 4 = 5`
- Shift + Ctrl: `1 + 1 + 4 = 6`
- Alt + Ctrl: `1 + 2 + 4 = 7`
- Shift + Alt + Ctrl: `1 + 1 + 2 + 4 = 8`

Arrows and cursor keys encode as `\x1b[1;{mod}A` through `\x1b[1;{mod}D`. Tilde-style navigation keys
(PageUp, PageDown, Insert, Delete) encode as `\x1b[{code};{mod}~` (for example, `Ctrl+Delete` -> `\x1b[3;5~`).

### 5. Alt as ESC Prefix (Meta Key)
When the user holds Alt while typing a character (e.g. `Alt+b` to move backward a word, or `Alt+f` to move
forward a word in shells), the terminal prefixes the character sequence with the ESC byte (`0x1b`).
`Alt+b` transmits `\x1bb`, and `Alt+Enter` transmits `\x1b\r`.

### 6. Bracketed Paste Mode (DECSET 2004)
When interactive shells or REPLs enable bracketed paste, pasting multi-line or untrusted text must not
execute immediately as if typed keystroke-by-keystroke. Oxidite wraps pasted text between guard markers:
$$\text{\textbackslash x1b[200\textasciitilde} + \text{text} + \text{\textbackslash x1b[201\textasciitilde}$$
When disabled, the text passes through as raw UTF-8 bytes.

### 7. SGR 1006 Extended Mouse Reporting
When applications (e.g. `htop`, `tmux`, mouse-enabled `vim`) enable mouse tracking via DECSET 1006,
mouse events are encoded in SGR decimal format:
$$\text{\textbackslash x1b[<\{button\};\{col\};\{row\}\{action\}}$$
- Buttons: 0 for Left, 1 for Middle, 2 for Right, 64 for WheelUp, 65 for WheelDown.
- Modifier bits: +4 for Shift, +8 for Alt, +16 for Ctrl, +32 for Motion drag.
- Coordinates: 1-indexed (`col + 1`, `row + 1`).
- Action suffix: `'M'` for press and motion, `'m'` for release.

### 8. Viewport Scrollback & Mouse Wheel
When mouse reporting is disabled, mouse wheel scrolling and keyboard shortcuts (`Shift+PageUp`, `Shift+PageDown`,
`Shift+Home`, `Shift+End`) adjust `Grid.display_offset`.
- `display_offset == 0` displays the live active terminal screen.
- `display_offset > 0` shifts the rendered viewport upwards into the scrollback ring buffer.
- `Grid::display_row` maps viewport row index `0..rows` directly to the correct scrollback or visible row.
- Typing any key automatically resets the viewport to the live prompt (`display_offset = 0`).

### 9. Text Selection & Clipboard Integration
- Users can click and drag with the left mouse button across grid cells to select text.
- `Selection` normalizes start and end points into top-left and bottom-right ordering.
- `Selection::extract_text` iterates across selected rows, excludes continuation half-cells of wide
  characters, trims trailing blank space per line, and joins lines with newlines.
- `Ctrl+Shift+C` (or `Cmd+C` on macOS) extracts the selected text and writes it to the operating system
  clipboard via `arboard`.
- `Ctrl+Shift+V` (or `Cmd+V`) reads from the system clipboard and pastes it into the PTY with bracketed paste.

## What to read next

- `src/input.rs` — the complete key, mouse, paste, and selection encoding logic.
- `src/app.rs` — window event loop handling, viewport scrolling, and clipboard dispatching.
- `src/grid.rs` — `display_row`, `scroll_display`, and scrollback ring buffer management.
- `tests/input.rs` and `table_driven_key_encoding_matrix` in `src/input.rs` — table-driven test cases.
- xterm ctlseqs: Keyboard Input and SGR 1006 Extended Mouse Mode specifications.
