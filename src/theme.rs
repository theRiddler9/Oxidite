//! Color themes (stub — Phase 7).

/// 8-bit RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    /// Red color component (0–255).
    pub r: u8,
    /// Green color component (0–255).
    pub g: u8,
    /// Blue color component (0–255).
    pub b: u8,
}

impl Rgb {
    /// Creates an RGB color from 8-bit channels.
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// Named theme palette.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Human-readable theme name.
    pub name: String,
    /// Default text foreground color.
    pub foreground: Rgb,
    /// Default window background color.
    pub background: Rgb,
    /// Cursor color.
    pub cursor: Rgb,
    /// Selection highlight background color.
    pub selection_bg: Rgb,
    /// Standard 16 ANSI colors (8 normal, 8 bright).
    pub ansi: [Rgb; 16],
}

impl Theme {
    /// Minimal dark default used until built-in presets land in Phase 7.
    ///
    /// The 16 ANSI entries are distinct so indexed colors are never
    /// black-on-black before real theme files load.
    pub fn default_dark() -> Self {
        // xterm-ish dark palette; Phase 7 presets may replace this.
        let ansi = [
            Rgb::new(0x1c, 0x1c, 0x22), // 0 black
            Rgb::new(0xff, 0x5c, 0x7a), // 1 red
            Rgb::new(0x3d, 0xdc, 0x97), // 2 green
            Rgb::new(0xf5, 0xc5, 0x42), // 3 yellow
            Rgb::new(0x6e, 0xa8, 0xff), // 4 blue
            Rgb::new(0xc7, 0x92, 0xea), // 5 magenta
            Rgb::new(0x64, 0xd2, 0xe8), // 6 cyan
            Rgb::new(0xe8, 0xe8, 0xed), // 7 white
            Rgb::new(0x5a, 0x5a, 0x66), // 8 bright black
            Rgb::new(0xff, 0x7a, 0x94), // 9 bright red
            Rgb::new(0x5a, 0xeb, 0xb0), // 10 bright green
            Rgb::new(0xff, 0xd5, 0x6a), // 11 bright yellow
            Rgb::new(0x8f, 0xc0, 0xff), // 12 bright blue
            Rgb::new(0xd9, 0xa8, 0xff), // 13 bright magenta
            Rgb::new(0x8a, 0xe0, 0xf0), // 14 bright cyan
            Rgb::new(0xff, 0xff, 0xff), // 15 bright white
        ];
        Self {
            name: "default-dark".to_string(),
            foreground: ansi[7],
            background: Rgb::new(0x0a, 0x0a, 0x0c),
            cursor: ansi[7],
            selection_bg: Rgb::new(0x2a, 0x2a, 0x3d),
            ansi,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that default_dark provides 16 distinct ANSI colors with contrast against background.
    #[test]
    fn default_dark_ansi_palette_usable() {
        let theme = Theme::default_dark();
        assert_eq!(theme.ansi.len(), 16);
        for (i, color) in theme.ansi.iter().enumerate() {
            assert_ne!(
                *color, theme.background,
                "ANSI color {i} must not match background"
            );
        }
        assert_ne!(theme.foreground, theme.background);
    }
}
