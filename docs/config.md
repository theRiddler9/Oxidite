# Oxidite Configuration and Theme Format

Oxidite loads its configuration from a TOML file located in the user's standard platform configuration directory, and supports hot reloading on change without restarting the terminal.

## Configuration File Locations

Oxidite locates `config.toml` and custom themes according to standard OS conventions:

- **Linux / BSD**: `$XDG_CONFIG_HOME/oxidite/config.toml` (defaults to `~/.config/oxidite/config.toml`)
- **macOS**: `~/Library/Application Support/oxidite/config.toml`
- **Windows**: `%APPDATA%\oxidite\config.toml` (e.g. `C:\Users\<User>\AppData\Roaming\oxidite\config.toml`)

User themes are loaded from a `themes/` subdirectory within the configuration directory:
- **Linux / BSD**: `~/.config/oxidite/themes/*.toml`
- **macOS**: `~/Library/Application Support/oxidite/themes/*.toml`
- **Windows**: `%APPDATA%\oxidite\themes\*.toml`

If `config.toml` is absent or contains syntax errors, Oxidite falls back gracefully to built-in defaults and logs a descriptive warning without crashing or interrupting the session.

---

## Configuration Options (`config.toml`)

Below is a complete reference of the top-level options in `config.toml`:

```toml
# The active theme name.
# Can be a built-in preset ("dracula", "solarized-dark", "solarized-light",
# "gruvbox-dark", "nord", "one-dark", "default-dark") or the name of a custom
# theme placed in the themes/ directory (e.g. "my-theme" for "my-theme.toml").
theme = "dracula"

# Font family name for monospace text rendering.
# Defaults to "monospace". Can specify any system monospace font (e.g., "JetBrains Mono", "Cascadia Code").
font_family = "monospace"

# Font size in points.
# Defaults to 14.0.
font_size = 14.0

# Window inner padding in pixels.
# Defaults to 4.0.
padding_x = 4.0
padding_y = 4.0

# Command to launch in the PTY.
# Defaults to None, which detects the user's default shell ($SHELL on Unix, pwsh/powershell/cmd on Windows).
# shell = "/bin/zsh"

# Arguments to pass to the shell command.
# Defaults to empty list [].
# shell_args = ["-l"]

# Number of scrollback lines retained in the buffer.
# Defaults to 10000.
scrollback_lines = 10000

# Custom keybindings
[keybindings]
# Text copy and paste
copy = "Ctrl+Shift+C"
paste = "Ctrl+Shift+V"

# Font zoom controls
zoom_in = "Ctrl+="
zoom_out = "Ctrl+-"
zoom_reset = "Ctrl+0"

# Scrollback navigation
scroll_page_up = "Shift+PageUp"
scroll_page_down = "Shift+PageDown"

# Interactive theme cycling
cycle_theme = "Ctrl+Shift+T"
```

---

## Keybinding Syntax

Keybindings are expressed as modifier combinations separated by `+`:
- **Modifiers**: `Ctrl`, `Shift`, `Alt`, `Super` (or `Cmd` / `Win`)
- **Keys**: Letters (`A`-`Z`), digits (`0`-`9`), symbols (`+`, `=`, `-`), or special navigation keys (`PageUp`, `PageDown`, `Home`, `End`, `Space`, etc.)

Examples:
- `"Ctrl+Shift+C"`
- `"Ctrl+="`
- `"Shift+PageUp"`
- `"Ctrl+Shift+T"`

---

## Theme File Format (`themes/*.toml`)

A theme defines the foreground, background, cursor, selection, and the 16 standard ANSI colors (8 normal + 8 bright).
Colors are specified as 6-digit hexadecimal strings (`"#rrggbb"`).

Oxidite supports both the table syntax and the array syntax for defining ANSI colors.

### Format 1: Colors Table (Recommended)

```toml
name = "my-custom-theme"

[colors]
foreground = "#f8f8f2"
background = "#282a36"
cursor = "#f8f8f2"
selection_bg = "#44475a"

# Standard 8 colors (0-7)
black = "#21222c"
red = "#ff5555"
green = "#50fa7b"
yellow = "#f1fa8c"
blue = "#bd93f9"
magenta = "#ff79c6"
cyan = "#8be9fd"
white = "#f8f8f2"

# Bright 8 colors (8-15)
bright_black = "#6272a4"
bright_red = "#ff6e6e"
bright_green = "#69ff94"
bright_yellow = "#ffffa5"
bright_blue = "#d6acff"
bright_magenta = "#ff92df"
bright_cyan = "#a4ffff"
bright_white = "#ffffff"
```

### Format 2: Array Syntax

```toml
name = "my-custom-theme"
foreground = "#f8f8f2"
background = "#282a36"
cursor = "#f8f8f2"
selection_bg = "#44475a"

ansi = [
    "#21222c", # 0: black
    "#ff5555", # 1: red
    "#50fa7b", # 2: green
    "#f1fa8c", # 3: yellow
    "#bd93f9", # 4: blue
    "#ff79c6", # 5: magenta
    "#8be9fd", # 6: cyan
    "#f8f8f2", # 7: white
    "#6272a4", # 8: bright black
    "#ff6e6e", # 9: bright red
    "#69ff94", # 10: bright green
    "#ffffa5", # 11: bright yellow
    "#d6acff", # 12: bright blue
    "#ff92df", # 13: bright magenta
    "#a4ffff", # 14: bright cyan
    "#ffffff", # 15: bright white
]
```

---

## Built-in Presets

Oxidite includes high-quality built-in presets compiled into the binary, which are available even without external theme files:

- `dracula`
- `solarized-dark`
- `solarized-light`
- `gruvbox-dark`
- `nord`
- `one-dark`
- `default-dark`
