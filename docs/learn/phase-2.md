# Phase 2 — The VT / ANSI Parser

## In plain language

A terminal shell outputs a continuous stream of raw bytes. Some of these bytes represent
plain printable text (like `'H'`, `'e'`, `'l'`, `'l'`, `'o'`), while others are control
characters or escape sequences instructing the terminal to move the cursor, erase text,
switch text color, activate the alternate screen, or change the window title.

Phase 2 builds the **interpreter**: it wraps the state-machine parser (`vte`), receives
arbitrary chunks of raw PTY bytes, and translates them into semantic method calls on the
[`Handler`](../../src/handler.rs) trait.

Crucially, **no terminal or grid state lives in the parser**. The parser's sole
responsibility is tokenizing and translating sequence protocols into events; updating
cells, scrolling, and rendering are left to downstream phases.

## Key ideas

- **The State Machine (`vte`):**
  - Parsing terminal escape sequences is notoriously intricate because sequences can be
    interrupted by C0 controls (e.g. `LF` or `BEL` inside a sequence) or split across
    multiple reads.
  - Oxidite wraps the `vte` crate, which implements Paul Flo Williams' DEC ANSI-compatible
    parser state machine. It transitions between states (`Ground`, `Escape`, `CsiEntry`,
    `CsiParam`, `OscString`, etc.) byte by byte.
- **C0 Controls vs Printable Characters:**
  - Standard printable text (including multibyte UTF-8, emoji, and wide CJK characters)
    calls `Handler::print(c)`.
  - C0 control codes (ASCII 0x00–0x1F) execute immediately:
    - Line feed (`LF`, `0x0A`), vertical tab (`VT`, `0x0B`), and form feed (`FF`, `0x0C`)
      trigger `Handler::linefeed()`.
    - Carriage return (`CR`, `0x0D`) triggers `Handler::carriage_return()`.
    - Backspace (`BS`, `0x08`) triggers `Handler::backspace()`.
    - Horizontal tab (`HT`, `0x09`) triggers `Handler::tab()`.
    - Bell (`BEL`, `0x07`) triggers `Handler::bell()`.
- **CSI (Control Sequence Introducer — `ESC [`):**
  - **Cursor movement:**
    - `CUU` (`CSI Ps A`) / `CUD` (`CSI Ps B`) / `CUF` (`CSI Ps C`) / `CUB` (`CSI Ps D`)
      relative displacements via `Handler::move_cursor_by(d_row, d_col)`.
    - `CUP` (`CSI Ps ; Ps H`) and `HVP` (`CSI Ps ; Ps f`) absolute positioning via
      `Handler::move_cursor_to(row, col)`.
  - **Erase operations:**
    - `ED` (`CSI Ps J`) erases in display (to end, to start, all, or all including
      scrollback) via `Handler::erase_in_display(mode)`.
    - `EL` (`CSI Ps K`) erases in line (to end, to start, or all) via
      `Handler::erase_in_line(mode)`.
  - **Insert / Delete mutations:**
    - `IL` (`CSI Ps L`) and `DL` (`CSI Ps M`) insert/delete blank lines.
    - `ICH` (`CSI Ps @`) and `DCH` (`CSI Ps P`) insert/delete characters.
    - `ECH` (`CSI Ps X`) erases characters without shifting text.
  - **Scroll margins:**
    - `DECSTBM` (`CSI Pt ; Pb r`) sets top and bottom scrolling margins via
      `Handler::set_scroll_region(top, bottom)`.
- **SGR (Select Graphic Rendition — `CSI ... m`):**
  - **16 standard colors:** Foreground codes 30–37 (standard) and 90–97 (bright);
    background codes 40–47 and 100–107. Default resets on 39 and 49.
  - **256 colors:** `38;5;n` (fg) and `48;5;n` (bg).
  - **24-bit Truecolor:** `38;2;r;g;b` (fg) and `48;2;r;g;b` (bg), also supporting modern
    colon-delimited formats (`38:2::r:g:b`).
  - **Text styles:** Bold (1), Dim (2), Italic (3), Underline (4), Inverse (7), Hidden (8),
    Strikethrough (9), and their exact cancel codes (22, 23, 24, 27, 28, 29, and full reset 0).
- **DEC Private Modes (DECSET / DECRST — `CSI ? ... h / l`):**
  - Toggles terminal modes via `Handler::set_mode(mode, enabled)`.
  - Supported modes: Autowrap (`?7`), Cursor visibility (`?25`), Application cursor keys
    (`?1`), Alternate screen (`?1049`), Bracketed paste (`?2004`), and Mouse reporting
    (`?1000`/`?1006`).
- **OSC (Operating System Commands — `ESC ]`):**
  - OSC 0 and OSC 2 set window/icon title via `Handler::set_title(title)`, supporting
    both `BEL` (`\x07`) and `ST` (`ESC \`) string terminators.
- **ESC Control Sequences:**
  - `ESC 7` (`DECSC`): Save cursor position and attributes.
  - `ESC 8` (`DECRC`): Restore cursor position and attributes.
  - `ESC M` (`RI`): Reverse index (moves cursor up one row or scrolls downward if at the
    top margin).
- **Chunk Fragmentation Testing:**
  - Real-world byte streams from network sockets or PTY buffers can fragment across any
    arbitrary boundary. The parser wrapper ensures state continuity even when escape
    prefixes, parameters, or multibyte UTF-8 codepoints arrive across separate `advance()`
    calls.

## Where to look in the code

| Component | File Path |
|---|---|
| Parser wrapper & dispatcher | [`src/parser.rs`](../../src/parser.rs) |
| Handler trait & sequence enums | [`src/handler.rs`](../../src/handler.rs) |
| Shared contracts reference | [`docs/contracts/shared-contracts.md`](../contracts/shared-contracts.md) |
| Captured byte stream fixtures | [`tests/fixtures/`](../../tests/fixtures/) |
| Sequence family unit & fixture tests | [`src/parser.rs`](../../src/parser.rs) (`mod tests`) |

## What to read next (Phase 3)

1. `docs/plans/grid.md`
2. Terminal grid storage: cells, rows, cursor clamping, and scroll regions
3. Scrollback buffer ring architecture
4. Alternate screen buffer management (for full-screen apps like `vim`, `htop`, `less`)
5. Implementing `Handler` for `Terminal` state and tracking dirty damaged rows
