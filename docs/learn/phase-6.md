# Phase 6 — Rendering Polish & Visual Fidelity

## In plain language

In Phase 6, Oxidite transforms from a bare-bones monospace renderer into a production-grade,
smoothly animated, full-featured terminal renderer. While Phase 4 opened the window and rendered
basic monochrome text, and Phase 5 wired user input, Phase 6 provides the visual fidelity and
performance characteristics expected of modern high-performance terminals like Alacritty, WezTerm,
and Windows Terminal.

Output now looks crisp on modern high-DPI displays, supports full truecolor and 256-color palettes,
animates cursor shapes and blinking, draws text decorations (underline, strikethrough, dim, inverse),
renders wide CJK characters and emoji, and stays silky smooth even under intense output loads like
`cat` of large files or `seq 1 100000`.

---

## Key ideas

### 1. The Rendering Architecture

```
Terminal State (Grid, Cursor, Modes, Title, Damage)
  │
  ├── 1. Background Quads (build_cell_background_vertices)
  │     └── Generates solid 2D quads for non-default cell backgrounds, respecting wide cells.
  │
  ├── 2. Text Decorations (build_text_decoration_vertices)
  │     └── Underline and strikethrough lines with proportional thickness.
  │
  ├── 3. Cursor (build_cursor_vertices_with_width)
  │     ├── Block, Underline, and Bar cursor shapes (DECSCUSR).
  │     ├── 500ms blink phase toggle with zero idle CPU when steady.
  │     └── Expands to 2 cells wide when hovering over wide characters.
  │
  ├── 4. Selection Highlight (build_selection_vertices)
  │     └── Translucent accent quad spanning active mouse drag selection.
  │
  ├── 5. Visual Bell (build_quad_vertices overlay)
  │     └── 100ms translucent white flash on C0 BEL, cleared on timer.
  │
  └── 6. Glyph Pipeline (GlyphPipeline & GlyphCache)
        ├── Row damage check (dirty_rows): skips clean cached rows.
        ├── Rich text spans with Bold, Italic, Dim, Hidden, and Truecolor.
        ├── Monospace + Fallback emoji / CJK shaping via glyphon.
        └── Single WGPU render pass combining quads and text glyphs.
```

---

### 2. Glyph Caching & Row Damage Tracking

Shaping text with font engines (HarfBuzz / `cosmic-text`) is computationally heavy. If a window
has 50 rows of 120 columns, reshaping 6,000 cells every frame would drop frame rates and waste battery.

Oxidite solves this through two tiers of caching:
1. **Row Damage Tracking**: The terminal tracks which rows were touched by PTY output or scroll
   operations (`dirty_rows`). When redrawing, rows marked clean are skipped entirely.
2. **Glyph Buffer Cache (`GlyphCache`)**: Each row stores a `CachedRowBuffer` containing its shaped
   `FontBuffer` and a 64-bit content hash signature (incorporating row index, font metrics, and cell
   content). Unchanged rows retain their GPU-resident glyph assignments without reshaping.

---

### 3. Full 256-Color Palette and Truecolor (24-bit RGB)

Terminal applications rely on sophisticated color hierarchies:
- **16-color ANSI**: Standard 8 colors + 8 high-intensity colors from the terminal theme palette.
- **256-color palette**:
  - `0..15`: Base ANSI colors.
  - `16..231`: A 6x6x6 color cube ($16 + 36r + 6g + b$ where components are $0, 95, 135, 175, 215, 255$).
  - `232..255`: A 24-step grayscale ramp from dark gray ($8, 8, 8$) to near white ($238, 238, 238$).
- **Truecolor (`Color::Rgb(r, g, b)`)**: Direct 24-bit RGB values (`16.7 million colors`), converted
  linearly to normalized float RGBA components (`[r/255, g/255, b/255, 1.0]`).

---

### 4. Text Attributes: Inverse, Dim, and Hidden

- **Inverse (`Attrs::INVERSE`)**: Swaps the foreground and background colors. If the background was
  default, it takes the default foreground color, ensuring legible inverted text.
- **Dim (`Attrs::DIM`)**: Scales the foreground RGB components down by 35% for faint text.
- **Hidden (`Attrs::HIDDEN`)**: Used by password prompts; masks characters as spaces while preserving
  underlying cell dimensions and background colors.

---

### 5. Cursor Shapes and Blink Timer

Programs switch cursor appearance using the standard `DECSCUSR` escape sequence (`CSI Ps SP q`):
- `Ps = 0, 1`: Blinking block
- `Ps = 2`: Steady block
- `Ps = 3`: Blinking underline
- `Ps = 4`: Steady underline
- `Ps = 5`: Blinking bar
- `Ps = 6`: Steady bar

In `App::about_to_wait`, Oxidite inspects whether cursor blinking is active. When active, it sets
the event loop control flow to wait for the remaining fraction of the 500ms blink duration. When
blinking is disabled or the window loses focus, control flow sleeps indefinitely (`ControlFlow::Wait`),
guaranteeing **zero idle CPU usage**.

---

### 6. HiDPI Scaling & Live Font Zoom

Displays range from standard 96 DPI to 200%+ 4K and Retina screens:
- **HiDPI**: On startup and on `WindowEvent::ScaleFactorChanged`, Oxidite captures the monitor scale
  factor and measures font metrics at `base_font_size * scale_factor`.
- **Font Zoom**: Users can dynamically scale the font size using `Ctrl + Plus`, `Ctrl + Minus`, or
  reset it with `Ctrl + 0`. Oxidite recalculates cell metrics, clears shaped glyph caches, adjusts
  the terminal grid dimensions, and notifies the PTY of the new column and row count.

---

### 7. Wide Characters & Emoji

Unicode characters like East Asian CJK characters (`語`, `日`, `本`) and emoji (`🦀`, `🚀`) occupy
two standard character cells.
- The grid represents wide characters as a leading cell (`width = 2`) followed by a spacer cell
  (`width = 0`).
- Background quads, text decorations, and the cursor expand to cover both physical cell columns.
- Text span extraction drops spacer cells so HarfBuzz shapes the complete grapheme sequence without
  gaps or corruption.

---

### 8. Large-Output Throughput & Burst Coalescing

When dumping large streams (`cat`, `seq 1 100000`, compilation logs), PTYs yield hundreds of small
chunks. Forwarding each chunk individually through window event proxies causes thread contention
and event starvation.

Oxidite introduces **burst coalescing** in `pty-event-forwarder`: when a chunk arrives, the thread
drains any pending chunks already buffered in the channel (up to 64 KB per batch). The event loop
then parses the entire batch in one pass, updates damage tracking once, and lets `winit` coalesce
redraw requests into smooth, tear-free frame presentations.

---

## What to read next

- `src/renderer/draw.rs` — Quad pipelines, glyph shaping, color resolution, and selection geometry.
- `src/app.rs` — Event loop integration, zoom handlers, visual bell, and PTY batch coalescing.
- `docs/plans/config.md` — The architecture for Phase 7 (TOML configuration, themes, and hot-reload).
