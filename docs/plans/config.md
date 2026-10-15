# Plan: Config and Themes

## Goal

Load TOML config and themes, support hot reload and live theme switching, and
never crash the app on bad config.

## Ownership

- `src/config.rs` — paths, load, defaults, keybindings, hot reload
- `src/theme.rs` — `Theme`, `Rgb`, built-in presets
- `themes/*.toml` — shipped theme files

## Dependencies

- `serde`, `toml`
- `notify` (hot reload)
- `dirs` (config directory)

## Public surface

```rust
pub struct Theme {
    pub name: String,
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
    pub selection_bg: Rgb,
    pub ansi: [Rgb; 16],
}
```

Config options (Phase 7): font, size, padding, shell, scrollback, keybindings,
active theme.

## Implementation order (Phase 7)

1. `Theme` + `Rgb` parsing
2. Built-in presets: dracula, solarized-dark/light, gruvbox, nord, one-dark
3. Resolve config directory per platform
4. Load `config.toml` with serde defaults
5. Load themes from themes directory
6. Font / size / padding / shell / scrollback options
7. Configurable keybindings
8. Hot reload with `notify`
9. Theme-cycle keybinding + live switch
10. Invalid config → defaults + log error
11. Parsing/fallback tests
12. Document config and theme format
13. `docs/learn/phase-7.md`

## Exit criteria

Themes switch live, config edits hot-reload, bad config never crashes.

## References

- `serde` / `toml` / `notify` / `dirs` docs
