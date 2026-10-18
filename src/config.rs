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

/// User-facing config (defaults only for now).
#[derive(Debug, Clone)]
pub struct Config {
    /// Active color theme.
    pub theme: Theme,
    /// Font size in points.
    pub font_size: f32,
    /// Scrollback capacity in lines.
    pub scrollback_lines: usize,
}

impl Default for Config {
    /// Built-in defaults used until `config.toml` loading lands in Phase 7.
    fn default() -> Self {
        Self {
            theme: Theme::default_dark(),
            font_size: 14.0,
            scrollback_lines: 10_000,
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
        assert!((config.font_size - 14.0).abs() < f32::EPSILON);
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
}
