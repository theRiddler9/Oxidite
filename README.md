# Oxidite

A cross-platform terminal emulator written in Rust. Oxidite is the host window
itself (like Windows Terminal or Ghostty): it spawns a real shell over a PTY,
parses VT/ANSI, maintains a grid, and renders with a GPU-backed window.

Phase 0 is scaffold only (docs, contracts, stubs). Headless PTY passthrough
lands in Phase 1. See `AGENTS.md` and `docs/plan.md` for the build plan.

## Build and run

```bash
cargo run
```

Release build (needed for the golden demo in later phases):

```bash
cargo run --release
```

## Writing a theme

Theme **loading from TOML is not implemented in Phase 0** — that arrives in
Phase 7. The on-disk format below is the planned contract so you can draft
files under `themes/` early.

Each theme is a TOML file with a name, UI colors, and a 16-color ANSI palette
(`ansi_0` … `ansi_15`, or a single `ansi` array of 16 `"#RRGGBB"` strings):

```toml
name = "example-dark"

foreground   = "#E8E8ED"
background   = "#0A0A0C"
cursor       = "#E8E8ED"
selection_bg = "#2A2A3D"

ansi = [
  "#1C1C22", # 0 black
  "#FF5C7A", # 1 red
  "#3DDC97", # 2 green
  "#F5C542", # 3 yellow
  "#6EA8FF", # 4 blue
  "#C792EA", # 5 magenta
  "#64D2E8", # 6 cyan
  "#E8E8ED", # 7 white
  "#5A5A66", # 8 bright black
  "#FF7A94", # 9 bright red
  "#5AEBB0", # 10 bright green
  "#FFD56A", # 11 bright yellow
  "#8FC0FF", # 12 bright blue
  "#D9A8FF", # 13 bright magenta
  "#8AE0F0", # 14 bright cyan
  "#FFFFFF", # 15 bright white
]
```

Until Phase 7, the in-memory default is `Theme::default_dark()` in
`src/theme.rs`.

## License

MIT
