# Phase 3 — Terminal State & Grid

## In plain language

A terminal emulator is fundamentally a stateful text canvas. While Phase 2 built the parser
to decode raw bytes into semantic actions (such as "print `'A'`", "move cursor to row 4, col 10",
or "scroll down"), Phase 3 builds the **memory model and state engine**: the 2D grid of character
cells, the cursor, the scrollback history, and application modes.

When full-screen programs like `vim`, `htop`, or `nano` run, or when simple command-line tools
output formatted colored text, the terminal maintains this state in memory. The terminal state
is the single source of truth that downstream rendering phases (Phase 4 and Phase 6) read to
draw the window on the GPU.

## Key ideas

- **The 2D Grid & Cell Model (`src/grid.rs`):**
  - The visible screen is modeled as a contiguous array of `Cell` structures organized into rows
    and columns (`cols * rows`).
  - Each `Cell` stores:
    - `ch`: The unicode character (e.g. `' '`, `'A'`, `'日'`).
    - `fg` / `bg`: Foreground and background colors (`Color::Default`, `Color::Indexed(0..255)`,
      or `Color::Rgb(r, g, b)`).
    - `attrs`: Style bitflags (`Attrs::BOLD`, `ITALIC`, `UNDERLINE`, `INVERSE`, `DIM`, `HIDDEN`,
      `STRIKETHROUGH`).
    - `width`: Number of columns occupied by this character (1 for standard Latin, 2 for CJK/emoji,
      0 for continuation cells or combining marks).
- **Deferred Autowrap:**
  - In VT-compatible terminals, printing a character into the final column (`col == cols - 1`)
    does **not** immediately wrap the cursor to the next line. If an application prints 80 characters
    on an 80-column terminal, the cursor sits at column 79 with a deferred wrap flag (`wrap_next = true`).
  - Wrapping only executes when the *subsequent* printable character arrives. If the next command is
    a carriage return (`\r`) or cursor positioning (`CUP`), the line does not prematurely wrap.
- **Wide Characters & Overwrite Safety:**
  - Double-width characters (e.g. CJK ideographs, wide emoji) occupy two columns. Oxidite checks
    `unicode_width::UnicodeWidthChar::width(c)`.
  - When placing a wide character at column `x`, the primary cell is stored at `x` with `width: 2`,
    and column `x + 1` is populated with a continuation cell (`ch: ' '`, `width: 0`).
  - If a subsequent print or erase hits either half of a wide character, the complementary cell is
    automatically cleared to avoid visual corruption.
- **Cursor Management & Clamping:**
  - The cursor tracks `(row, col)`, visibility, and shape.
  - Internal coordinates are 0-indexed (`0..rows`, `0..cols`), whereas escape sequences from the parser
    are 1-indexed. The `Handler` implementation converts and clamps all coordinates to screen bounds.
  - Save/Restore Cursor (`DECSC` / `DECRC`) preserves position, graphic attributes (`attrs`), colors,
    and autowrap state.
- **Scroll Regions (`DECSTBM`) & Scrolling:**
  - By default, scrolling affects the full screen (`top = 0`, `bottom = rows - 1`).
  - Applications like `less` or split TUIs can define restricted scroll margins via `DECSTBM`.
  - Scrolling up shifts rows within the margin upward and introduces blank lines at the bottom;
    scrolling down (`RI` / reverse index) shifts rows downward and introduces blank lines at the top.
  - Lines scrolled out of the top of the *full-screen* scroll region are archived into the scrollback
    ring buffer.
- **Scrollback Buffer:**
  - A `VecDeque<Vec<Cell>>` ring buffer stores up to a configurable maximum of historical rows
    (default: 10,000 lines).
  - When new lines scroll off the top of the screen, they enter scrollback. When capacity is reached,
    the oldest rows are evicted, maintaining predictable memory usage.
- **Alternate Screen Buffer (`?1049`):**
  - Full-screen terminal programs (e.g. `vim`, `htop`, `tmux`) activate the alternate screen.
  - Oxidite maintains two distinct `Grid` instances: the `primary_grid` (with scrollback history)
    and the `alt_grid` (a dedicated scratchpad without scrollback).
  - Toggling mode `?1049` automatically saves cursor state and clears the alternate screen on enter,
    and restores the untouched primary screen and cursor upon exit.
- **Damage Tracking:**
  - High performance requires avoiding unnecessary full-screen redraws.
  - `Terminal` tracks `dirty_rows: Vec<bool>`, marking individual rows whenever cells, cursor,
    erases, or scroll operations modify that line. Downstream renderers query `is_row_dirty(row)`
    and call `clear_damage()` after presenting a frame.
- **Headless Grid Dump (`--dump-grid`):**
  - Allows driving shell commands and verifying grid layout directly from the terminal without
    starting a GUI window. ConPTY DSR queries (`\x1b[6n`) are automatically answered with cursor
    reports (`\x1b[1;1R`) to prevent blocking on Windows.

## Where to look in the code

| Component | File Path |
|---|---|
| Cell, Color, Attrs, Grid, and Cursor | [`src/grid.rs`](../../src/grid.rs) |
| Terminal state, Modes, Alternate screen, Handler impl | [`src/terminal.rs`](../../src/terminal.rs) |
| Handler trait interface | [`src/handler.rs`](../../src/handler.rs) |
| Headless grid dump execution | [`src/main.rs`](../../src/main.rs) |
| Grid snapshot & unit tests | [`src/grid.rs`](../../src/grid.rs), [`src/terminal.rs`](../../src/terminal.rs) |
| Screen test fixtures | [`tests/fixtures/`](../../tests/fixtures/) |

## What to read next (Phase 4)

1. `docs/plans/renderer.md`
2. `winit` 0.30 event loop architecture and window creation
3. `wgpu` graphics pipeline setup (Surface, Adapter, Device, Queue)
4. Monospace font loading, cell measurement, and glyph rasterization with `glyphon`
5. Cell background quad rendering and cursor drawing
6. Translating damage tracking rows into efficient incremental draw calls
