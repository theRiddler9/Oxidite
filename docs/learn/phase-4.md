# Phase 4 — Window & Basic Rendering

## In plain language

Phase 4 transforms Oxidite from a headless terminal state engine into a fully independent,
hardware-accelerated desktop window. A real operating system window opens, hosts a shell
process over a PTY, and renders terminal text, colors, cell backgrounds, and an interactive cursor
directly on the GPU using `winit`, `wgpu`, and `glyphon`.

The terminal emulator does not use webviews, electron, or third-party terminal widgets. Every
glyph, background rectangle, and cursor quad is positioned and rendered via dedicated GPU
pipelines written directly in this repository.

## Key ideas

- **The Event-Driven GUI Pipeline:**
  ```
  PTY Reader Thread -> AppEvent::PtyOutput(bytes) -> EventLoopProxy -> UI Thread
    -> VT Parser -> Terminal State (Damaged Rows) -> Window::request_redraw()
    -> RedrawRequested -> QuadPipeline + GlyphPipeline -> GPU Present
  ```
- **Winit 0.30 & Cross-Thread Event Plumbing (`src/app.rs`, `src/renderer/window.rs`):**
  - The UI runs inside `winit`'s `ApplicationHandler<AppEvent>` lifecycle.
  - The UI thread never blocks on PTY I/O. A dedicated background thread (`pty-event-forwarder`)
    reads raw bytes from the PTY master channel and delivers them to the event loop using
    `EventLoopProxy<AppEvent>::send_event`.
  - Child exit status is observed upon channel closure and dispatched as `AppEvent::PtyExited`.
- **WGPU 30 Hardware Graphics Architecture (`src/renderer/gpu.rs`):**
  - Initializes `wgpu::Instance`, creates a hardware `Surface` tied to the window handle, selects
    a high-performance `Adapter`, and configures the logical `Device` and command `Queue`.
  - Configures the swapchain surface with `PresentMode::AutoVsync` to prevent visual tearing and
    ensure smooth frame pacing.
  - Manages surface reconfiguration on resize and surface recreation if outdated or lost.
- **Monospace Font Metrics & Cell Measurement (`src/renderer/font.rs`):**
  - Measures glyph advance width and line height using `glyphon` / `cosmic-text`'s `FontSystem`.
  - By shaping a reference monospace character (`'M'`), `CellMetrics` calculates the precise
    physical pixel dimensions of a terminal cell (`cell_width` and `cell_height`).
  - Sizing functions (`window_size_to_grid_dimensions` and `window_size_to_pty_size`) map window
    pixel dimensions to terminal rows and columns.
- **Cell Background & Cursor Quad Rendering (`src/renderer/draw.rs`, `src/renderer/shaders/quad.wgsl`):**
  - `QuadPipeline` compiles a custom WGSL shader that transforms 2D pixel coordinates into
    normalized device coordinates (NDC `[-1.0, 1.0]`).
  - `build_cell_background_vertices` iterates through visible grid cells and emits two triangles
    (6 vertices) for any cell whose background color differs from the terminal default.
  - `build_cursor_vertices` generates vertices for the cursor according to its shape (`Block`,
    `Underline`, or `Bar`) and visibility mode.
- **Text Glyph Rasterization via Glyphon (`src/renderer/draw.rs`):**
  - `GlyphPipeline` integrates `glyphon::TextRenderer`, `TextAtlas`, and `Viewport`.
  - Grid rows are scanned to coalesce contiguous spans of identical text color into rich text
    buffers, minimizing memory and draw overhead.
  - Glyphs are dynamically rasterized and cached into a GPU texture atlas and rendered in the
    active render pass with correct per-cell foreground colors.
- **Strict Resize Order:**
  - Resize flows **window -> grid -> PTY**, in that exact order, in one place (`App::handle_resize`):
    1. Reconfigure the GPU surface to new physical pixel dimensions.
    2. Recalculate column and row counts from cell metrics and resize `Terminal` (resizing both
       primary and alternate screen grids).
    3. Update the PTY master with the new row, column, and pixel dimensions so interactive shells
       (e.g. `zsh`, `bash`, `fish`, `pwsh`) adjust their line editing buffers and redraw.
- **Damage-Driven Redrawing:**
  - Redrawing is strictly demand-driven: frames are rendered only when `WindowEvent::RedrawRequested`
    fires. Idle CPU usage remains near zero.
  - When PTY output arrives, `Terminal` marks damaged rows, and a redraw is requested only if
    damage actually occurred.

## Where to look in the code

| Component | File Path |
|---|---|
| Window creation & dimension mapping | [`src/renderer/window.rs`](../../src/renderer/window.rs) |
| WGPU context & surface management | [`src/renderer/gpu.rs`](../../src/renderer/gpu.rs) |
| Font metrics & cell dimensions | [`src/renderer/font.rs`](../../src/renderer/font.rs) |
| Quad & glyph drawing pipelines | [`src/renderer/draw.rs`](../../src/renderer/draw.rs) |
| Quad 2D vertex/fragment shader | [`src/renderer/shaders/quad.wgsl`](../../src/renderer/shaders/quad.wgsl) |
| Event loop wiring & resize flow | [`src/app.rs`](../../src/app.rs) |
| Application startup & GUI dispatch | [`src/main.rs`](../../src/main.rs) |

## What to read next (Phase 5)

1. `docs/plans/input.md`
2. Winit `KeyEvent`, `PhysicalKey`, `NamedKey`, and key modifier states
3. ANSI escape sequence encoding for printable characters, enter, tab, and backspace
4. Application cursor keys (`DECCKM`) and arrow/navigation key encoding
5. Function keys and modified keys (`CSI 1;mod`)
6. Bracketed paste mode (`?2004`) and mouse reporting protocols (`SGR 1006`)
7. Mouse text selection and system clipboard integration (`arboard`)
