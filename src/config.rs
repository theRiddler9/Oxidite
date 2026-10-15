//! Application configuration loaded from TOML (stub — Phase 7).

use crate::theme::Theme;

/// User-facing config (defaults only for now).
#[derive(Debug, Clone)]
pub struct Config {
    pub theme: Theme,
    pub font_size: f32,
    pub scrollback_lines: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: Theme::default_dark(),
            font_size: 14.0,
            scrollback_lines: 10_000,
        }
    }
}
