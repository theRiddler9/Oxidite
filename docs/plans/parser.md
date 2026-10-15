# Plan: Parser

## Goal

Turn raw PTY bytes into a stream of `Handler` calls. No grid logic lives here.

## Ownership

`src/parser.rs` — wrapper around `vte::Parser` implementing `vte::Perform`,
dispatching into `Handler`.

## Dependencies

- `vte` (Phase 2)

## Public surface

- `Parser` wrapping `vte::Parser`
- `advance(&mut self, handler: &mut dyn Handler, bytes: &[u8])`
- Logging `Handler` for debugging

## Sequence families (Phase 2)

1. Print + C0 (LF, CR, BS, HT, BEL)
2. CSI cursor movement
3. CSI erase in display / line (`ED` / `EL`)
4. SGR 16-color, 256-color, truecolor, text styles
5. DECSET / DECRST modes
6. Scroll region (`DECSTBM`)
7. OSC 0 / 2 title
8. ESC save/restore cursor, reverse index
9. Insert/delete line and character

## Tests

- Byte fixtures under `tests/fixtures/` per sequence family
- Sequences split across multiple `advance` calls

## Exit criteria

Raw PTY bytes produce a correct `Handler` call stream, verified by logging
handler and fixture tests.

## References

- xterm ctlseqs
- `vte` crate docs and source
