//! Application configuration loaded from TOML (stub — Phase 7).

use crate::theme::Theme;

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
}
