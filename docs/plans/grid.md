# Plan: Terminal State (Grid)

## Goal

Maintain the authoritative screen: cells, cursor, scroll regions, scrollback,
alternate screen, modes, title, and damage. Implements `Handler`.

## Ownership

- `src/grid.rs` — `Color`, `Attrs`, `Cell`, `Grid`, cursor helpers
- `src/terminal.rs` — `Modes`, alternate buffer, `Handler` impl, damage

## Dependencies

- `unicode-width` (wide characters)
- `bitflags` (attrs / modes)

## Implementation order (Phase 3)

1. `Color`, `Attrs`, `Cell`
2. Grid storage and indexing
3. `put_char` with autowrap
4. Cursor movement with clamping
5. Erase operations
6. Scroll up/down + scroll regions
7. Scrollback ring buffer
8. Wide character handling
9. Resize behavior
10. `Modes` + alternate screen
11. `Handler` for `Terminal`
12. Damage tracking for dirty rows
13. Headless grid dump mode
14. Snapshot tests from fixtures
15. `docs/learn/phase-3.md`

## Exit criteria

Captured byte streams produce a grid matching expected snapshots, including
alternate screen and scrollback.

## References

- Alacritty `alacritty_terminal` term/cell (read only, never depend)
- xterm ctlseqs for ED/EL/IL/DL/scroll
