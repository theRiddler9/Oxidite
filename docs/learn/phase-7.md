# Phase 7 — Configuration & Live Themes

## In plain language

In Phase 7, Oxidite gains complete customization: users can configure their fonts, window padding, shell program, scrollback history, and keybindings, as well as customize their visual theme or switch between popular built-in color schemes live at runtime.

Crucially, Oxidite adopts a "never crash on bad config" policy. If a user mistypes a hex color, corrupts the TOML syntax, or deletes a theme file, Oxidite logs a descriptive warning and falls back safely to sensible defaults, keeping the active terminal session completely uninterrupted. Furthermore, changes to `config.toml` are detected automatically in the background using filesystem events (`notify`), hot-reloading font sizes and themes without restarting the application.

---

## Key ideas

### 1. The Theme Contract & Palette Hierarchy

A terminal theme governs colors across foreground, background, cursor, selection, and the 16 ANSI colors:

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

Colors are parsed from standard 6-digit hex strings (`#rrggbb`) using `Rgb::from_hex`. Oxidite supports both:
1. **Shipped & Built-in Presets**: High quality compiled-in presets (`dracula`, `solarized-dark`, `solarized-light`, `gruvbox-dark`, `nord`, `one-dark`, and `default-dark`).
2. **User Themes**: External `.toml` files located in the user's `themes/` directory, loaded on demand or referenced in `config.toml`.

---

### 2. Configuration Schema & Cross-Platform Paths

Oxidite standardizes configuration paths per operating system via `dirs`:
- **Linux/BSD**: `~/.config/oxidite/config.toml`
- **macOS**: `~/Library/Application Support/oxidite/config.toml`
- **Windows**: `%APPDATA%\oxidite\config.toml`

The configuration structure supports serde default attributes:

```rust
pub struct Config {
    pub theme: Theme,
    pub font_family: String,
    pub font_size: f32,
    pub padding_x: f32,
    pub padding_y: f32,
    pub shell: Option<String>,
    pub shell_args: Vec<String>,
    pub scrollback_lines: usize,
    pub keybindings: KeybindingsConfig,
}
```

If any field is omitted or misspelled in `config.toml`, only that specific field falls back to its default without failing the entire file.

---

### 3. Hot Reload with `notify`

Oxidite uses the cross-platform `notify` crate to watch the directory containing `config.toml`. When a file write or rename event occurs:
1. The watcher thread triggers a callback.
2. The main event loop receives an `AppEvent::ConfigReloaded` notification via `EventLoopProxy`.
3. The UI thread re-reads `config.toml` with `Config::load_or_default()`.
4. Visual parameters (theme, font metrics, padding) are updated dynamically and a window redraw is requested immediately.

---

### 4. Live Theme Switching

Pressing `Ctrl+Shift+T` (or the configured `cycle_theme` keybinding) cycles through all available built-in and shipped themes in real-time. The active theme immediately refreshes the cell palette, background quad colors, cursor highlights, and text styles, triggering a redraw pass across the current screen buffer.

---

### 5. Resilient Fallbacks

Bad configurations are treated as non-fatal events:
- If `config.toml` contains invalid TOML syntax: logs a warning and loads default configuration.
- If a requested `theme = "unknown"` cannot be found: logs a warning and falls back to `default-dark`.
- If a file write is in progress when the watcher triggers: graceful handling prevents partial reads from terminating the terminal emulator.

---

## Where to look in the code

- `src/theme.rs`: `Rgb` struct, hex color parser, `Theme` model, built-in presets (`dracula`, `solarized`, `nord`, `gruvbox`, `one-dark`), TOML deserializers.
- `src/config.rs`: `Config` model, platform path resolution (`config_dir()`, `config_path()`, `themes_dir()`), theme loader, filesystem watcher with `notify`, keybinding definitions.
- `src/app.rs`: `AppEvent::ConfigReloaded`, `AppEvent::ThemeChanged`, theme cycling action handler, metric recalculation, and quad rebuild.
- `docs/config.md`: Complete guide to configuration options, keybinding syntax, and theme authoring.
- `config.example.toml`: Ready-to-use template file.

---

## What to read next

Proceed to **Phase 8 — Hardening and release**:
- Property-based testing for parser state with `proptest`.
- High-throughput benchmarking with large byte dumps (`seq` / `cat`).
- Platform-specific fixes (ConPTY on Windows, Wayland / X11 on Linux).
- Cross-platform CI matrix and golden demo verification.
