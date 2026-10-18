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
        assert!(config.shell_args.is_empty());
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
        assert!(config.shell_args.is_empty());
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
}
