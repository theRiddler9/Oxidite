use std::path::PathBuf;

use crate::theme::Theme;

/// Returns the platform-specific configuration directory for Oxidite.
///
/// - Windows: `%APPDATA%\oxidite`
/// - Linux: `$XDG_CONFIG_HOME/oxidite` or `~/.config/oxidite`
/// - macOS: `~/Library/Application Support/oxidite`
#[must_use]
pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("oxidite"))
}

/// Returns the path to the primary configuration file (`config.toml`).
#[must_use]
pub fn config_path() -> Option<PathBuf> {
    config_dir().map(|p| p.join("config.toml"))
}

/// Returns the directory where custom theme files are stored (`themes/`).
#[must_use]
pub fn themes_dir() -> Option<PathBuf> {
    config_dir().map(|p| p.join("themes"))
}

/// Sets up a file watcher for configuration changes that triggers a callback on file modifications.
pub fn watch_config<F>(
    path: &std::path::Path,
    on_change: F,
) -> Result<notify::RecommendedWatcher, notify::Error>
where
    F: Fn() + Send + 'static,
{
    use notify::{Event, EventKind, RecursiveMode, Watcher};

    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            match event.kind {
                EventKind::Modify(_) | EventKind::Create(_) => {
                    on_change();
                }
                _ => {}
            }
        }
    })?;

    if path.exists() {
        watcher.watch(path, RecursiveMode::NonRecursive)?;
    } else if let Some(parent) = path.parent()
        && parent.exists()
    {
        watcher.watch(parent, RecursiveMode::NonRecursive)?;
    }

    Ok(watcher)
}

/// Errors that can occur during configuration loading and parsing.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// File I/O failure when reading the configuration file.
    #[error("failed to read config file '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// TOML parsing failure.
    #[error("failed to parse config TOML: {0}")]
    Toml(#[from] toml::de::Error),
    /// Theme parsing failure.
    #[error("theme error: {0}")]
    Theme(#[from] crate::theme::ThemeError),
}

/// Configurable keyboard shortcuts mapping to terminal actions.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct KeybindingsConfig {
    /// Copy selection to clipboard.
    pub copy: String,
    /// Paste clipboard text into PTY.
    pub paste: String,
    /// Increase font size.
    pub zoom_in: String,
    /// Decrease font size.
    pub zoom_out: String,
    /// Reset font size to default.
    pub zoom_reset: String,
    /// Scroll up in history.
    pub scroll_up: String,
    /// Scroll down in history.
    pub scroll_down: String,
    /// Scroll to top of history.
    pub scroll_to_top: String,
    /// Scroll to bottom of history.
    pub scroll_to_bottom: String,
    /// Cycle to next available theme.
    pub cycle_theme: String,
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        Self {
            copy: "Ctrl+Shift+C".to_string(),
            paste: "Ctrl+Shift+V".to_string(),
            zoom_in: "Ctrl+=".to_string(),
            zoom_out: "Ctrl+-".to_string(),
            zoom_reset: "Ctrl+0".to_string(),
            scroll_up: "Shift+PageUp".to_string(),
            scroll_down: "Shift+PageDown".to_string(),
            scroll_to_top: "Shift+Home".to_string(),
            scroll_to_bottom: "Shift+End".to_string(),
            cycle_theme: "Ctrl+Shift+T".to_string(),
        }
    }
}

/// Matches a string keybinding specification (e.g. `"Ctrl+Shift+C"`) against a `KeyEvent`.
#[must_use]
pub fn matches_keybinding(binding: &str, event: &crate::input::KeyEvent) -> bool {
    if event.state != winit::event::ElementState::Pressed {
        return false;
    }

    let mut need_ctrl = false;
    let mut need_shift = false;
    let mut need_alt = false;
    let mut need_super = false;
    let mut key_part = "";

    for part in binding.split('+') {
        let trimmed = part.trim();
        if trimmed.eq_ignore_ascii_case("ctrl") || trimmed.eq_ignore_ascii_case("control") {
            need_ctrl = true;
        } else if trimmed.eq_ignore_ascii_case("shift") {
            need_shift = true;
        } else if trimmed.eq_ignore_ascii_case("alt")
            || trimmed.eq_ignore_ascii_case("opt")
            || trimmed.eq_ignore_ascii_case("option")
        {
            need_alt = true;
        } else if trimmed.eq_ignore_ascii_case("super")
            || trimmed.eq_ignore_ascii_case("cmd")
            || trimmed.eq_ignore_ascii_case("command")
            || trimmed.eq_ignore_ascii_case("win")
        {
            need_super = true;
        } else {
            key_part = trimmed;
        }
    }

    if event.modifiers.control_key() != need_ctrl
        || event.modifiers.shift_key() != need_shift
        || event.modifiers.alt_key() != need_alt
        || event.modifiers.super_key() != need_super
    {
        return false;
    }

    match &event.logical_key {
        winit::keyboard::Key::Named(named) => match named {
            winit::keyboard::NamedKey::PageUp => {
                key_part.eq_ignore_ascii_case("pageup") || key_part.eq_ignore_ascii_case("page_up")
            }
            winit::keyboard::NamedKey::PageDown => {
                key_part.eq_ignore_ascii_case("pagedown")
                    || key_part.eq_ignore_ascii_case("page_down")
            }
            winit::keyboard::NamedKey::Home => key_part.eq_ignore_ascii_case("home"),
            winit::keyboard::NamedKey::End => key_part.eq_ignore_ascii_case("end"),
            _ => false,
        },
        winit::keyboard::Key::Character(ch) => {
            if ch.eq_ignore_ascii_case(key_part) {
                return true;
            }
            if ((key_part == "+" || key_part == "=") && (ch == "+" || ch == "="))
                || (key_part == "-" && (ch == "-" || ch == "_"))
            {
                return true;
            }
            false
        }
        _ => false,
    }
}

impl KeybindingsConfig {
    /// Maps a key event to an `InputAction` based on configured bindings.
    #[must_use]
    pub fn check_action(
        &self,
        event: &crate::input::KeyEvent,
    ) -> Option<crate::input::InputAction> {
        if matches_keybinding(&self.copy, event) {
            return Some(crate::input::InputAction::Copy);
        }
        if matches_keybinding(&self.paste, event) {
            return Some(crate::input::InputAction::Paste);
        }
        if matches_keybinding(&self.zoom_in, event) {
            return Some(crate::input::InputAction::ZoomIn);
        }
        if matches_keybinding(&self.zoom_out, event) {
            return Some(crate::input::InputAction::ZoomOut);
        }
        if matches_keybinding(&self.zoom_reset, event) {
            return Some(crate::input::InputAction::ZoomReset);
        }
        if matches_keybinding(&self.scroll_up, event) {
            return Some(crate::input::InputAction::ScrollUp(12));
        }
        if matches_keybinding(&self.scroll_down, event) {
            return Some(crate::input::InputAction::ScrollDown(12));
        }
        if matches_keybinding(&self.scroll_to_top, event) {
            return Some(crate::input::InputAction::ScrollToTop);
        }
        if matches_keybinding(&self.scroll_to_bottom, event) {
            return Some(crate::input::InputAction::ScrollToBottom);
        }
        if matches_keybinding(&self.cycle_theme, event) {
            return Some(crate::input::InputAction::CycleTheme);
        }
        None
    }
}

/// Representation of `config.toml` structure for serde deserialization.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ConfigFile {
    /// Active theme name.
    pub theme: String,
    /// Primary font family name (e.g. "monospace", "`JetBrains Mono`").
    pub font_family: String,
    /// Font size in points.
    pub font_size: f32,
    /// Horizontal window padding in pixels.
    pub padding_x: f32,
    /// Vertical window padding in pixels.
    pub padding_y: f32,
    /// Optional custom shell program path or command.
    pub shell: Option<String>,
    /// Optional command-line arguments for the custom shell.
    pub shell_args: Vec<String>,
    /// Scrollback capacity in lines.
    pub scrollback_lines: usize,
    /// Configurable keybindings.
    pub keybindings: KeybindingsConfig,
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self {
            theme: "default-dark".to_string(),
            font_family: "monospace".to_string(),
            font_size: 14.0,
            padding_x: 4.0,
            padding_y: 4.0,
            shell: None,
            shell_args: Vec::new(),
            scrollback_lines: 10_000,
            keybindings: KeybindingsConfig::default(),
        }
    }
}

/// User-facing config.
#[derive(Debug, Clone)]
pub struct Config {
    /// Active color theme.
    pub theme: Theme,
    /// Primary font family name (e.g. "monospace", "`JetBrains Mono`").
    pub font_family: String,
    /// Font size in points.
    pub font_size: f32,
    /// Horizontal window padding in pixels.
    pub padding_x: f32,
    /// Vertical window padding in pixels.
    pub padding_y: f32,
    /// Optional custom shell program path or command.
    pub shell: Option<String>,
    /// Optional command-line arguments for the custom shell.
    pub shell_args: Vec<String>,
    /// Scrollback capacity in lines.
    pub scrollback_lines: usize,
    /// Configurable keybindings.
    pub keybindings: KeybindingsConfig,
}

impl Default for Config {
    /// Built-in defaults.
    fn default() -> Self {
        Self {
            theme: Theme::default_dark(),
            font_family: "monospace".to_string(),
            font_size: 14.0,
            padding_x: 4.0,
            padding_y: 4.0,
            shell: None,
            shell_args: Vec::new(),
            scrollback_lines: 10_000,
            keybindings: KeybindingsConfig::default(),
        }
    }
}

/// Loads a theme by name from either built-in presets or custom theme TOML files.
///
/// Lookups are attempted in the following order:
/// 1. Built-in presets (e.g. `dracula`, `solarized-dark`, etc.)
/// 2. User configuration themes directory (`<config_dir>/themes/<name>.toml`)
/// 3. Application shipped themes directory (`./themes/<name>.toml`)
#[must_use]
pub fn load_theme_by_name(name: &str) -> Option<Theme> {
    if let Some(preset) = Theme::preset_by_name(name) {
        return Some(preset);
    }

    let file_name = if std::path::Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
    {
        name.to_string()
    } else {
        format!("{name}.toml")
    };

    if let Some(mut dir) = themes_dir() {
        dir.push(&file_name);
        if let Ok(content) = std::fs::read_to_string(&dir) {
            match Theme::from_toml(&content) {
                Ok(theme) => return Some(theme),
                Err(err) => log::warn!("failed to parse user theme '{}': {err}", dir.display()),
            }
        }
    }

    let shipped_path = PathBuf::from("themes").join(&file_name);
    if let Ok(content) = std::fs::read_to_string(&shipped_path) {
        match Theme::from_toml(&content) {
            Ok(theme) => return Some(theme),
            Err(err) => {
                log::warn!(
                    "failed to parse shipped theme '{}': {err}",
                    shipped_path.display()
                );
            }
        }
    }

    None
}

/// Discovers all available theme names across built-in presets and theme directories.
#[must_use]
pub fn available_themes() -> Vec<String> {
    let mut names = Vec::new();
    for preset in Theme::builtin_presets() {
        names.push((*preset).to_string());
    }

    let mut scan_dir = |dir: &std::path::Path| {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let is_toml = path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("toml"));
                if let (true, Some(stem)) = (is_toml, path.file_stem().and_then(|s| s.to_str())) {
                    let stem_str = stem.to_string();
                    if !names.contains(&stem_str) {
                        names.push(stem_str);
                    }
                }
            }
        }
    };

    if let Some(user_themes) = themes_dir() {
        scan_dir(&user_themes);
    }
    scan_dir(std::path::Path::new("themes"));

    names
}

impl Config {
    /// Parses configuration from a TOML string.
    pub fn from_toml(content: &str) -> Result<Self, ConfigError> {
        let file: ConfigFile = toml::from_str(content)?;
        let theme = load_theme_by_name(&file.theme).unwrap_or_else(|| {
            log::warn!("theme '{}' not found, using default-dark", file.theme);
            Theme::default_dark()
        });

        Ok(Self {
            theme,
            font_family: file.font_family,
            font_size: file.font_size,
            padding_x: file.padding_x,
            padding_y: file.padding_y,
            shell: file.shell,
            shell_args: file.shell_args,
            scrollback_lines: file.scrollback_lines,
            keybindings: file.keybindings,
        })
    }

    /// Loads configuration from a given file path.
    pub fn load_from_path(path: &std::path::Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_toml(&content)
    }

    /// Loads configuration from the standard platform config file path, falling back
    /// to defaults on missing file or parse error.
    #[must_use]
    pub fn load() -> Self {
        if let Some(path) = config_path()
            && path.exists()
        {
            return Self::load_or_default(&path);
        }
        Self::default()
    }

    /// Loads configuration from a given file path, falling back to defaults and logging
    /// a warning on any I/O or syntax error.
    #[must_use]
    pub fn load_or_default(path: &std::path::Path) -> Self {
        match Self::load_from_path(path) {
            Ok(config) => config,
            Err(err) => {
                log::warn!(
                    "Failed to load configuration from '{}': {err}. Falling back to default configuration.",
                    path.display()
                );
                Self::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that Config default values match expected initial configuration.
    #[test]
    fn config_defaults_match_expected() {
        let config = Config::default();
        assert_eq!(config.theme.name, "default-dark");
        assert_eq!(config.font_family, "monospace");
        assert!((config.font_size - 14.0).abs() < f32::EPSILON);
        assert!((config.padding_x - 4.0).abs() < f32::EPSILON);
        assert!((config.padding_y - 4.0).abs() < f32::EPSILON);
        assert!(config.shell.is_none());
        assert_eq!(config.shell_args, Vec::<String>::new());
        assert_eq!(config.scrollback_lines, 10_000);
    }

    #[test]
    fn config_directory_resolution_succeeds_on_supported_platforms() {
        if let Some(dir) = config_dir() {
            assert!(dir.ends_with("oxidite"));
        }
        if let Some(cfg) = config_path() {
            assert!(cfg.ends_with("config.toml"));
            assert_eq!(
                cfg.file_name().and_then(|n| n.to_str()),
                Some("config.toml")
            );
        }
        if let Some(themes) = themes_dir() {
            assert!(themes.ends_with("themes"));
            assert_eq!(themes.file_name().and_then(|n| n.to_str()), Some("themes"));
        }
    }

    #[test]
    fn config_from_toml_empty_uses_defaults() {
        let config = Config::from_toml("").expect("empty config should parse to defaults");
        assert_eq!(config.theme.name, "default-dark");
        assert_eq!(config.font_family, "monospace");
        assert!((config.font_size - 14.0).abs() < f32::EPSILON);
        assert!((config.padding_x - 4.0).abs() < f32::EPSILON);
        assert!((config.padding_y - 4.0).abs() < f32::EPSILON);
        assert!(config.shell.is_none());
        assert_eq!(config.shell_args, Vec::<String>::new());
        assert_eq!(config.scrollback_lines, 10_000);
    }

    #[test]
    fn config_from_toml_custom_values() {
        let toml_str = r#"
theme = "dracula"
font_family = "JetBrains Mono"
font_size = 16.5
padding_x = 8.0
padding_y = 12.0
shell = "/bin/zsh"
shell_args = ["-l"]
scrollback_lines = 5000
"#;
        let config = Config::from_toml(toml_str).expect("custom config should parse");
        assert_eq!(config.theme.name, "dracula");
        assert_eq!(config.font_family, "JetBrains Mono");
        assert!((config.font_size - 16.5).abs() < f32::EPSILON);
        assert!((config.padding_x - 8.0).abs() < f32::EPSILON);
        assert!((config.padding_y - 12.0).abs() < f32::EPSILON);
        assert_eq!(config.shell, Some("/bin/zsh".to_string()));
        assert_eq!(config.shell_args, vec!["-l".to_string()]);
        assert_eq!(config.scrollback_lines, 5000);
    }

    #[test]
    fn config_from_toml_unknown_theme_falls_back_to_default_dark() {
        let toml_str = r#"
theme = "nonexistent-theme-xyz"
"#;
        let config = Config::from_toml(toml_str).expect("config should parse with fallback theme");
        assert_eq!(config.theme.name, "default-dark");
    }

    #[test]
    fn config_from_toml_invalid_toml_returns_err() {
        let toml_str = "invalid [ toml syntax";
        assert!(Config::from_toml(toml_str).is_err());
    }

    #[test]
    fn available_themes_lists_all_builtins_and_shipped() {
        let themes = available_themes();
        assert!(themes.contains(&"dracula".to_string()));
        assert!(themes.contains(&"solarized-dark".to_string()));
        assert!(themes.contains(&"solarized-light".to_string()));
        assert!(themes.contains(&"gruvbox-dark".to_string()));
        assert!(themes.contains(&"nord".to_string()));
        assert!(themes.contains(&"one-dark".to_string()));
    }

    #[test]
    fn load_theme_by_name_resolves_presets_and_shipped() {
        let dracula = load_theme_by_name("dracula").expect("dracula theme should resolve");
        assert_eq!(dracula.name, "dracula");

        let nord = load_theme_by_name("nord").expect("nord theme should resolve");
        assert_eq!(nord.name, "nord");

        assert!(load_theme_by_name("completely-missing-theme-name").is_none());
    }

    #[test]
    fn config_keybindings_parsing_and_action_dispatch() {
        let toml_str = r#"
[keybindings]
copy = "Ctrl+Shift+X"
paste = "Ctrl+Shift+Y"
cycle_theme = "Ctrl+Shift+Space"
"#;
        let config =
            Config::from_toml(toml_str).expect("config with custom keybindings should parse");
        assert_eq!(config.keybindings.copy, "Ctrl+Shift+X");
        assert_eq!(config.keybindings.paste, "Ctrl+Shift+Y");
        assert_eq!(config.keybindings.cycle_theme, "Ctrl+Shift+Space");

        let mut modifiers = winit::keyboard::ModifiersState::empty();
        modifiers |= winit::keyboard::ModifiersState::CONTROL;
        modifiers |= winit::keyboard::ModifiersState::SHIFT;

        let ev_x = crate::input::KeyEvent::new(
            winit::keyboard::Key::Character("x".into()),
            Some("x".to_string()),
            modifiers,
            winit::event::ElementState::Pressed,
        );
        assert_eq!(
            config.keybindings.check_action(&ev_x),
            Some(crate::input::InputAction::Copy)
        );

        let ev_c = crate::input::KeyEvent::new(
            winit::keyboard::Key::Character("c".into()),
            Some("c".to_string()),
            modifiers,
            winit::event::ElementState::Pressed,
        );
        // Default C is overridden by X in this config
        assert_eq!(config.keybindings.check_action(&ev_c), None);
    }

    #[test]
    fn watch_config_initializes_successfully_on_temp_file() {
        let temp_dir = std::env::temp_dir().join(format!("oxidite_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config_file = temp_dir.join("config.toml");
        std::fs::write(&config_file, "theme = \"nord\"\n").expect("write temp config");

        let watcher = watch_config(&config_file, || {});
        assert!(watcher.is_ok());

        let _ = std::fs::remove_file(&config_file);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn load_or_default_on_missing_file_returns_default() {
        let nonexistent = std::env::temp_dir().join("oxidite_nonexistent_config_file_12345.toml");
        let cfg = Config::load_or_default(&nonexistent);
        assert_eq!(cfg.theme.name, "default-dark");
        assert_eq!(cfg.font_family, "monospace");
        assert_eq!(cfg.scrollback_lines, 10_000);
    }

    #[test]
    fn load_or_default_on_invalid_syntax_returns_default() {
        let temp_dir =
            std::env::temp_dir().join(format!("oxidite_invalid_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config_file = temp_dir.join("invalid_config.toml");
        std::fs::write(&config_file, "invalid [[[ toml syntax ::::").expect("write temp config");

        let cfg = Config::load_or_default(&config_file);
        assert_eq!(cfg.theme.name, "default-dark");
        assert_eq!(cfg.font_family, "monospace");

        let _ = std::fs::remove_file(&config_file);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn load_or_default_on_valid_file_returns_parsed_config() {
        let temp_dir =
            std::env::temp_dir().join(format!("oxidite_valid_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config_file = temp_dir.join("valid_config.toml");
        let toml_data = r#"
theme = "solarized-dark"
font_family = "Consolas"
font_size = 18.0
padding_x = 10.0
padding_y = 10.0
scrollback_lines = 20000
"#;
        std::fs::write(&config_file, toml_data).expect("write valid config");

        let cfg = Config::load_or_default(&config_file);
        assert_eq!(cfg.theme.name, "solarized-dark");
        assert_eq!(cfg.font_family, "Consolas");
        assert!((cfg.font_size - 18.0).abs() < f32::EPSILON);
        assert_eq!(cfg.scrollback_lines, 20_000);

        let _ = std::fs::remove_file(&config_file);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn load_from_path_missing_file_returns_error() {
        let nonexistent = std::env::temp_dir().join("oxidite_missing_file_err_54321.toml");
        assert!(Config::load_from_path(&nonexistent).is_err());
    }

    #[test]
    fn partial_config_preserves_defaults() {
        let toml_data = r"
padding_x = 24.0
";
        let cfg = Config::from_toml(toml_data).expect("partial config parses");
        assert!((cfg.padding_x - 24.0).abs() < f32::EPSILON);
        assert!((cfg.padding_y - 4.0).abs() < f32::EPSILON);
        assert_eq!(cfg.font_family, "monospace");
        assert_eq!(cfg.theme.name, "default-dark");
        assert_eq!(cfg.scrollback_lines, 10_000);
    }
}
