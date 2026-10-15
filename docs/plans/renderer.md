# Plan: Window and Renderer

## Goal

Open a GPU-backed window and draw the terminal grid as monospace text with
colors and a cursor. Reads terminal state; never mutates it.

## Ownership

`src/renderer/`:

- `mod.rs` — public API
- `window.rs` — winit window helpers
- `gpu.rs` — wgpu surface / device
- `font.rs` — monospace font load + cell metrics
- `draw.rs` — background quads, glyphs, cursor

Also `src/app.rs` for event-loop wiring (Phase 4).

## Dependencies

- `winit`, `wgpu`, `glyphon`, `pollster`
- Versions must be mutually compatible (pin from what `glyphon` requires)

## Implementation order

### Phase 4 (basic)

1. Add deps
2. Window + event loop
3. `AppEvent` + `EventLoopProxy`
4. wgpu surface/device
5. Font + cell size
6. Cell background quads
7. Glyphs via glyphon
8. Cursor
9. Window size → rows/cols
10. Resize → grid → PTY
11. PTY output → parser → redraw request

### Phase 6 (polish)

Glyph cache, bold/italic, underline/strike, inverse/dim/hidden, full palette,
cursor blink/shapes, selection, damage-only redraw, HiDPI, font zoom,
wide/emoji, OSC title, visual bell, throughput fixes.

## Exit criteria (Phase 4)

Window opens, shell runs, live output appears as monospace text with colors.
Ugly is acceptable.

## References

- `winit`, `wgpu`, `glyphon` docs
