# Plan: Input

## Goal

Translate keyboard, mouse, and paste events into bytes for the PTY. Reads
terminal modes; never mutates the grid.

## Ownership

`src/input.rs` — `encode_key`, mouse reporting, paste, selection bindings.

## Dependencies

- `arboard` for clipboard (Phase 5)
- Window events from `winit`

## Public surface

```rust
pub fn encode_key(event: &KeyEvent, modes: &Modes) -> Option<Vec<u8>>;
```

## Implementation order (Phase 5)

1. Printable text
2. Enter, Tab, Backspace, Escape
3. Ctrl+letter
4. Arrows, Home, End, PageUp, PageDown
5. Application cursor key mode
6. Function keys
7. Alt as ESC prefix
8. Modified keys (`CSI 1;mod`)
9. Bracketed paste
10. Mouse reporting (SGR 1006)
11. Scrollback keybindings
12. Text selection with the mouse
13. Clipboard copy and paste
14. Table-driven encoding tests
15. `docs/learn/phase-5.md`

## Exit criteria

Shell is fully usable: line editing, history, and vim.

## References

- xterm ctlseqs (keyboard / mouse sections)
- Alacritty/WezTerm behavior when ambiguous
