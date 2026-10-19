//! Color themes, RGB representation, and palette presets.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// Error occurring when parsing an RGB color from a hex string.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseRgbError {
    /// The string length is invalid for a hex color (expected 6 or 7 characters).
    #[error("invalid hex color length (expected 6 or 7 characters, got {0})")]
    InvalidLength(usize),
    /// Hex byte parsing failed for one of the color channels.
    #[error("failed to parse hex channel in color '{0}'")]
    InvalidHex(String),
}

/// Error occurring when deserializing or validating a theme from TOML.
#[derive(Debug, Error)]
pub enum ThemeError {
    /// Failed to parse the TOML string.
    #[error("failed to parse theme TOML: {0}")]
    TomlParse(#[from] toml::de::Error),
    /// Required color field is missing.
    #[error("missing required color field in theme: {0}")]
    MissingField(&'static str),
    /// An ANSI color list was provided with a length other than 16.
    #[error("invalid ANSI palette size (expected 16 colors, got {0})")]
    InvalidAnsiCount(usize),
}

/// 8-bit RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Converts to a normalized RGBA float array with the specified alpha.
    #[must_use]
    pub fn to_rgba_array(self, alpha: f32) -> [f32; 4] {
        [
            f32::from(self.r) / 255.0,
            f32::from(self.g) / 255.0,
            f32::from(self.b) / 255.0,
            alpha,
        ]
    }

    /// Parses an RGB color from a hex string (e.g. `"#RRGGBB"` or `"RRGGBB"`).
    pub fn from_hex(s: &str) -> Result<Self, ParseRgbError> {
        let clean = s.trim();
        let hex = clean.strip_prefix('#').unwrap_or(clean);
        if hex.len() != 6 {
            return Err(ParseRgbError::InvalidLength(clean.len()));
        }

        let r = u8::from_str_radix(&hex[0..2], 16)
            .map_err(|_| ParseRgbError::InvalidHex(clean.to_string()))?;
        let g = u8::from_str_radix(&hex[2..4], 16)
            .map_err(|_| ParseRgbError::InvalidHex(clean.to_string()))?;
        let b = u8::from_str_radix(&hex[4..6], 16)
            .map_err(|_| ParseRgbError::InvalidHex(clean.to_string()))?;

        Ok(Self::new(r, g, b))
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

impl FromStr for Rgb {
    type Err = ParseRgbError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

impl Serialize for Rgb {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

/// Named theme palette with foreground, background, cursor, selection, and 16 ANSI colors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
struct ThemeFile {
    name: Option<String>,
    colors: Option<ColorsTable>,
    foreground: Option<Rgb>,
    background: Option<Rgb>,
    cursor: Option<Rgb>,
    selection_bg: Option<Rgb>,
    ansi: Option<AnsiColorsInput>,
}

#[derive(Debug, Clone, Deserialize)]
struct ColorsTable {
    foreground: Option<Rgb>,
    background: Option<Rgb>,
    cursor: Option<Rgb>,
    selection_bg: Option<Rgb>,
    ansi: Option<AnsiColorsInput>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum AnsiColorsInput {
    List(Vec<Rgb>),
    Named(NamedAnsiTable),
}

#[derive(Debug, Clone, Deserialize, Default)]
struct NamedAnsiTable {
    black: Option<Rgb>,
    red: Option<Rgb>,
    green: Option<Rgb>,
    yellow: Option<Rgb>,
    blue: Option<Rgb>,
    magenta: Option<Rgb>,
    cyan: Option<Rgb>,
    white: Option<Rgb>,
    bright_black: Option<Rgb>,
    bright_red: Option<Rgb>,
    bright_green: Option<Rgb>,
    bright_yellow: Option<Rgb>,
    bright_blue: Option<Rgb>,
    bright_magenta: Option<Rgb>,
    bright_cyan: Option<Rgb>,
    bright_white: Option<Rgb>,
}

impl Theme {
    /// Parses a `Theme` from a TOML string representation.
    pub fn from_toml(content: &str) -> Result<Self, ThemeError> {
        let file: ThemeFile = toml::from_str(content)?;
        let name = file.name.unwrap_or_else(|| "custom".to_string());

        let (fg, bg, cursor, sel_bg, ansi_input) = if let Some(colors) = file.colors {
            (
                colors.foreground.or(file.foreground),
                colors.background.or(file.background),
                colors.cursor.or(file.cursor),
                colors.selection_bg.or(file.selection_bg),
                colors.ansi.or(file.ansi),
            )
        } else {
            (
                file.foreground,
                file.background,
                file.cursor,
                file.selection_bg,
                file.ansi,
            )
        };

        let default_theme = Self::default_dark();

        let foreground = fg.unwrap_or(default_theme.foreground);
        let background = bg.unwrap_or(default_theme.background);
        let cursor = cursor.unwrap_or(default_theme.cursor);
        let selection_bg = sel_bg.unwrap_or(default_theme.selection_bg);

        let ansi = match ansi_input {
            Some(AnsiColorsInput::List(list)) => {
                if list.len() != 16 {
                    return Err(ThemeError::InvalidAnsiCount(list.len()));
                }
                let mut arr = [Rgb::default(); 16];
                arr.copy_from_slice(&list);
                arr
            }
            Some(AnsiColorsInput::Named(table)) => {
                let d = default_theme.ansi;
                [
                    table.black.unwrap_or(d[0]),
                    table.red.unwrap_or(d[1]),
                    table.green.unwrap_or(d[2]),
                    table.yellow.unwrap_or(d[3]),
                    table.blue.unwrap_or(d[4]),
                    table.magenta.unwrap_or(d[5]),
                    table.cyan.unwrap_or(d[6]),
                    table.white.unwrap_or(d[7]),
                    table.bright_black.unwrap_or(d[8]),
                    table.bright_red.unwrap_or(d[9]),
                    table.bright_green.unwrap_or(d[10]),
                    table.bright_yellow.unwrap_or(d[11]),
                    table.bright_blue.unwrap_or(d[12]),
                    table.bright_magenta.unwrap_or(d[13]),
                    table.bright_cyan.unwrap_or(d[14]),
                    table.bright_white.unwrap_or(d[15]),
                ]
            }
            None => default_theme.ansi,
        };

        Ok(Self {
            name,
            foreground,
            background,
            cursor,
            selection_bg,
            ansi,
        })
    }

    /// Serializes the theme to a formatted TOML string.
    pub fn to_toml_string(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// Converts the theme's 16 ANSI colors into float RGBA arrays for renderer draw passes.
    #[must_use]
    pub fn to_f32_palette(&self) -> [[f32; 4]; 16] {
        let mut palette = [[0.0; 4]; 16];
        for (i, rgb) in self.ansi.iter().enumerate() {
            palette[i] = rgb.to_rgba_array(1.0);
        }
        palette
    }

    /// Built-in default dark theme: pitch black retro phosphor aesthetic.
    #[must_use]
    pub fn default_dark() -> Self {
        let ansi = [
            Rgb::new(0x12, 0x14, 0x1a), // 0 black
            Rgb::new(0xff, 0x33, 0x66), // 1 red (arcade neon red)
            Rgb::new(0x00, 0xff, 0x88), // 2 green (phosphor CRT green)
            Rgb::new(0xff, 0xd0, 0x00), // 3 yellow (arcade amber)
            Rgb::new(0x38, 0x98, 0xff), // 4 blue (electric cyber blue)
            Rgb::new(0xd9, 0x46, 0xef), // 5 magenta (neon cyberpunk magenta)
            Rgb::new(0x00, 0xf0, 0xff), // 6 cyan (electric neon cyan)
            Rgb::new(0xe2, 0xe8, 0xf0), // 7 white (phosphor white)
            Rgb::new(0x47, 0x55, 0x69), // 8 bright black
            Rgb::new(0xff, 0x55, 0x88), // 9 bright red
            Rgb::new(0x38, 0xef, 0x7d), // 10 bright green
            Rgb::new(0xff, 0xe0, 0x66), // 11 bright yellow
            Rgb::new(0x60, 0xa5, 0xfa), // 12 bright blue
            Rgb::new(0xfa, 0x58, 0xb6), // 13 bright magenta
            Rgb::new(0x38, 0xbd, 0xf8), // 14 bright cyan
            Rgb::new(0xff, 0xff, 0xff), // 15 bright white
        ];
        Self {
            name: "default-dark".to_string(),
            foreground: ansi[7],
            background: Rgb::new(0x00, 0x00, 0x00), // Pure Pitch Black
            cursor: Rgb::new(0x00, 0xff, 0x88),     // Retro neon phosphor cursor
            selection_bg: Rgb::new(0x1a, 0x24, 0x38), // High-contrast retro deep indigo
            ansi,
        }
    }

    /// Built-in Dracula preset theme.
    #[must_use]
    pub fn dracula() -> Self {
        Self {
            name: "dracula".to_string(),
            foreground: Rgb::new(0xf8, 0xf8, 0xf2),
            background: Rgb::new(0x28, 0x2a, 0x36),
            cursor: Rgb::new(0xf8, 0xf8, 0xf2),
            selection_bg: Rgb::new(0x44, 0x47, 0x5a),
            ansi: [
                Rgb::new(0x21, 0x22, 0x2c), // 0 black
                Rgb::new(0xff, 0x55, 0x55), // 1 red
                Rgb::new(0x50, 0xfa, 0x7b), // 2 green
                Rgb::new(0xf1, 0xfa, 0x8c), // 3 yellow
                Rgb::new(0xbd, 0x93, 0xf9), // 4 blue
                Rgb::new(0xff, 0x79, 0xc6), // 5 magenta
                Rgb::new(0x8b, 0xe9, 0xfd), // 6 cyan
                Rgb::new(0xf8, 0xf8, 0xf2), // 7 white
                Rgb::new(0x62, 0x72, 0xa4), // 8 bright black
                Rgb::new(0xff, 0x6e, 0x6e), // 9 bright red
                Rgb::new(0x69, 0xff, 0x94), // 10 bright green
                Rgb::new(0xff, 0xff, 0xa5), // 11 bright yellow
                Rgb::new(0xd6, 0xac, 0xff), // 12 bright blue
                Rgb::new(0xff, 0x92, 0xdf), // 13 bright magenta
                Rgb::new(0xa4, 0xff, 0xff), // 14 bright cyan
                Rgb::new(0xff, 0xff, 0xff), // 15 bright white
            ],
        }
    }

    /// Built-in Solarized Dark preset theme.
    #[must_use]
    pub fn solarized_dark() -> Self {
        Self {
            name: "solarized-dark".to_string(),
            foreground: Rgb::new(0x83, 0x94, 0x96),
            background: Rgb::new(0x00, 0x2b, 0x36),
            cursor: Rgb::new(0x83, 0x94, 0x96),
            selection_bg: Rgb::new(0x07, 0x36, 0x42),
            ansi: [
                Rgb::new(0x07, 0x36, 0x42), // 0 black
                Rgb::new(0xdc, 0x32, 0x2f), // 1 red
                Rgb::new(0x85, 0x99, 0x00), // 2 green
                Rgb::new(0xb5, 0x89, 0x00), // 3 yellow
                Rgb::new(0x26, 0x8b, 0xd2), // 4 blue
                Rgb::new(0xd3, 0x36, 0x82), // 5 magenta
                Rgb::new(0x2a, 0xa1, 0x98), // 6 cyan
                Rgb::new(0xee, 0xe8, 0xd5), // 7 white
                Rgb::new(0x00, 0x2b, 0x36), // 8 bright black
                Rgb::new(0xcb, 0x4b, 0x16), // 9 bright red
                Rgb::new(0x58, 0x6e, 0x75), // 10 bright green
                Rgb::new(0x65, 0x7b, 0x83), // 11 bright yellow
                Rgb::new(0x83, 0x94, 0x96), // 12 bright blue
                Rgb::new(0x6c, 0x71, 0xc4), // 13 bright magenta
                Rgb::new(0x93, 0xa1, 0xa1), // 14 bright cyan
                Rgb::new(0xfd, 0xf6, 0xe3), // 15 bright white
            ],
        }
    }

    /// Built-in Solarized Light preset theme.
    #[must_use]
    pub fn solarized_light() -> Self {
        Self {
            name: "solarized-light".to_string(),
            foreground: Rgb::new(0x65, 0x7b, 0x83),
            background: Rgb::new(0xfd, 0xf6, 0xe3),
            cursor: Rgb::new(0x65, 0x7b, 0x83),
            selection_bg: Rgb::new(0xee, 0xe8, 0xd5),
            ansi: [
                Rgb::new(0x07, 0x36, 0x42), // 0 black
                Rgb::new(0xdc, 0x32, 0x2f), // 1 red
                Rgb::new(0x85, 0x99, 0x00), // 2 green
                Rgb::new(0xb5, 0x89, 0x00), // 3 yellow
                Rgb::new(0x26, 0x8b, 0xd2), // 4 blue
                Rgb::new(0xd3, 0x36, 0x82), // 5 magenta
                Rgb::new(0x2a, 0xa1, 0x98), // 6 cyan
                Rgb::new(0xee, 0xe8, 0xd5), // 7 white
                Rgb::new(0x00, 0x2b, 0x36), // 8 bright black
                Rgb::new(0xcb, 0x4b, 0x16), // 9 bright red
                Rgb::new(0x58, 0x6e, 0x75), // 10 bright green
                Rgb::new(0x65, 0x7b, 0x83), // 11 bright yellow
                Rgb::new(0x83, 0x94, 0x96), // 12 bright blue
                Rgb::new(0x6c, 0x71, 0xc4), // 13 bright magenta
                Rgb::new(0x93, 0xa1, 0xa1), // 14 bright cyan
                Rgb::new(0xfd, 0xf6, 0xe3), // 15 bright white
            ],
        }
    }

    /// Built-in Gruvbox Dark preset theme.
    #[must_use]
    pub fn gruvbox_dark() -> Self {
        Self {
            name: "gruvbox-dark".to_string(),
            foreground: Rgb::new(0xeb, 0xdb, 0xb2),
            background: Rgb::new(0x28, 0x28, 0x28),
            cursor: Rgb::new(0xeb, 0xdb, 0xb2),
            selection_bg: Rgb::new(0x50, 0x49, 0x45),
            ansi: [
                Rgb::new(0x28, 0x28, 0x28), // 0 black
                Rgb::new(0xcc, 0x24, 0x1d), // 1 red
                Rgb::new(0x98, 0x97, 0x1a), // 2 green
                Rgb::new(0xd7, 0x99, 0x21), // 3 yellow
                Rgb::new(0x45, 0x85, 0x88), // 4 blue
                Rgb::new(0xb1, 0x62, 0x86), // 5 magenta
                Rgb::new(0x68, 0x9d, 0x6a), // 6 cyan
                Rgb::new(0xa8, 0x99, 0x84), // 7 white
                Rgb::new(0x92, 0x83, 0x74), // 8 bright black
                Rgb::new(0xfb, 0x49, 0x34), // 9 bright red
                Rgb::new(0xb8, 0xbb, 0x26), // 10 bright green
                Rgb::new(0xfa, 0xbd, 0x2f), // 11 bright yellow
                Rgb::new(0x83, 0xa5, 0x98), // 12 bright blue
                Rgb::new(0xd3, 0x86, 0x9b), // 13 bright magenta
                Rgb::new(0x8e, 0xc0, 0x7c), // 14 bright cyan
                Rgb::new(0xeb, 0xdb, 0xb2), // 15 bright white
            ],
        }
    }

    /// Built-in Nord preset theme.
    #[must_use]
    pub fn nord() -> Self {
        Self {
            name: "nord".to_string(),
            foreground: Rgb::new(0xd8, 0xde, 0xe9),
            background: Rgb::new(0x2e, 0x34, 0x40),
            cursor: Rgb::new(0xd8, 0xde, 0xe9),
            selection_bg: Rgb::new(0x43, 0x4c, 0x5e),
            ansi: [
                Rgb::new(0x3b, 0x42, 0x52), // 0 black
                Rgb::new(0xbf, 0x61, 0x6a), // 1 red
                Rgb::new(0xa3, 0xbe, 0x8c), // 2 green
                Rgb::new(0xeb, 0xcb, 0x8b), // 3 yellow
                Rgb::new(0x81, 0xa1, 0xc1), // 4 blue
                Rgb::new(0xb4, 0x8e, 0xad), // 5 magenta
                Rgb::new(0x88, 0xc0, 0xd0), // 6 cyan
                Rgb::new(0xe5, 0xe9, 0xf0), // 7 white
                Rgb::new(0x4c, 0x56, 0x6a), // 8 bright black
                Rgb::new(0xbf, 0x61, 0x6a), // 9 bright red
                Rgb::new(0xa3, 0xbe, 0x8c), // 10 bright green
                Rgb::new(0xeb, 0xcb, 0x8b), // 11 bright yellow
                Rgb::new(0x81, 0xa1, 0xc1), // 12 bright blue
                Rgb::new(0xb4, 0x8e, 0xad), // 13 bright magenta
                Rgb::new(0x8f, 0xbc, 0xbb), // 14 bright cyan
                Rgb::new(0xec, 0xef, 0xf4), // 15 bright white
            ],
        }
    }

    /// Built-in One Dark preset theme.
    #[must_use]
    pub fn one_dark() -> Self {
        Self {
            name: "one-dark".to_string(),
            foreground: Rgb::new(0xab, 0xb2, 0xbf),
            background: Rgb::new(0x28, 0x2c, 0x34),
            cursor: Rgb::new(0x52, 0x8b, 0xff),
            selection_bg: Rgb::new(0x3e, 0x44, 0x51),
            ansi: [
                Rgb::new(0x28, 0x2c, 0x34), // 0 black
                Rgb::new(0xe0, 0x6c, 0x75), // 1 red
                Rgb::new(0x98, 0xc3, 0x79), // 2 green
                Rgb::new(0xe5, 0xc0, 0x7b), // 3 yellow
                Rgb::new(0x61, 0xaf, 0xef), // 4 blue
                Rgb::new(0xc6, 0x78, 0xdd), // 5 magenta
                Rgb::new(0x56, 0xb6, 0xc2), // 6 cyan
                Rgb::new(0xab, 0xb2, 0xbf), // 7 white
                Rgb::new(0x5c, 0x63, 0x70), // 8 bright black
                Rgb::new(0xe0, 0x6c, 0x75), // 9 bright red
                Rgb::new(0x98, 0xc3, 0x79), // 10 bright green
                Rgb::new(0xe5, 0xc0, 0x7b), // 11 bright yellow
                Rgb::new(0x61, 0xaf, 0xef), // 12 bright blue
                Rgb::new(0xc6, 0x78, 0xdd), // 13 bright magenta
                Rgb::new(0x56, 0xb6, 0xc2), // 14 bright cyan
                Rgb::new(0xff, 0xff, 0xff), // 15 bright white
            ],
        }
    }

    /// Returns a list of all built-in preset theme names.
    #[must_use]
    pub fn builtin_presets() -> &'static [&'static str] {
        &[
            "default-dark",
            "dracula",
            "solarized-dark",
            "solarized-light",
            "gruvbox-dark",
            "nord",
            "one-dark",
        ]
    }

    /// Looks up a built-in preset theme by its name.
    #[must_use]
    pub fn preset_by_name(name: &str) -> Option<Self> {
        match name {
            "default-dark" => Some(Self::default_dark()),
            "dracula" => Some(Self::dracula()),
            "solarized-dark" => Some(Self::solarized_dark()),
            "solarized-light" => Some(Self::solarized_light()),
            "gruvbox-dark" => Some(Self::gruvbox_dark()),
            "nord" => Some(Self::nord()),
            "one-dark" => Some(Self::one_dark()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_from_hex_valid_and_invalid() {
        assert_eq!(Rgb::from_hex("#ffffff"), Ok(Rgb::new(255, 255, 255)));
        assert_eq!(Rgb::from_hex("000000"), Ok(Rgb::new(0, 0, 0)));
        assert_eq!(Rgb::from_hex("#1c1c22"), Ok(Rgb::new(28, 28, 34)));
        assert_eq!(Rgb::from_hex("#1C1C22"), Ok(Rgb::new(28, 28, 34)));

        assert_eq!(Rgb::from_hex("#fff"), Err(ParseRgbError::InvalidLength(4)));
        assert_eq!(
            Rgb::from_hex("#gggggg"),
            Err(ParseRgbError::InvalidHex("#gggggg".to_string()))
        );
    }

    #[test]
    fn rgb_display_and_from_str() {
        let color = Rgb::new(0xab, 0xcd, 0xef);
        assert_eq!(color.to_string(), "#abcdef");
        assert_eq!(Rgb::from_str("#abcdef"), Ok(color));
    }

    #[test]
    fn rgb_serde_toml_roundtrip() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Wrapper {
            color: Rgb,
        }
        let original = Wrapper {
            color: Rgb::new(10, 20, 30),
        };
        let toml_str = toml::to_string(&original).unwrap();
        let decoded: Wrapper = toml::from_str(&toml_str).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn theme_from_toml_with_colors_table() {
        let toml_content = r##"
name = "test-theme"

[colors]
foreground = "#ffffff"
background = "#111111"
cursor = "#ff0000"
selection_bg = "#333333"

[colors.ansi]
black = "#000000"
red = "#ff0000"
green = "#00ff00"
yellow = "#ffff00"
blue = "#0000ff"
magenta = "#ff00ff"
cyan = "#00ffff"
white = "#ffffff"
bright_black = "#888888"
bright_red = "#ff8888"
bright_green = "#88ff88"
bright_yellow = "#ffff88"
bright_blue = "#8888ff"
bright_magenta = "#ff88ff"
bright_cyan = "#88ffff"
bright_white = "#ffffff"
"##;
        let theme = Theme::from_toml(toml_content).unwrap();
        assert_eq!(theme.name, "test-theme");
        assert_eq!(theme.foreground, Rgb::new(255, 255, 255));
        assert_eq!(theme.background, Rgb::new(17, 17, 17));
        assert_eq!(theme.cursor, Rgb::new(255, 0, 0));
        assert_eq!(theme.selection_bg, Rgb::new(51, 51, 51));
        assert_eq!(theme.ansi[1], Rgb::new(255, 0, 0));
        assert_eq!(theme.ansi[2], Rgb::new(0, 255, 0));

        let palette = theme.to_f32_palette();
        let color_1 = palette[1];
        assert!((color_1[0] - 1.0).abs() < 1e-4);
        assert!(color_1[1].abs() < 1e-4);
        assert!(color_1[2].abs() < 1e-4);
        assert!((color_1[3] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn theme_from_toml_with_array_ansi() {
        let toml_content = r##"
name = "array-theme"
foreground = "#aaaaaa"
background = "#222222"
cursor = "#ffffff"
selection_bg = "#444444"
ansi = [
    "#000000", "#111111", "#222222", "#333333",
    "#444444", "#555555", "#666666", "#777777",
    "#888888", "#999999", "#aaaaaa", "#bbbbbb",
    "#cccccc", "#dddddd", "#eeeeee", "#ffffff"
]
"##;
        let theme = Theme::from_toml(toml_content).unwrap();
        assert_eq!(theme.name, "array-theme");
        assert_eq!(theme.ansi[0], Rgb::new(0, 0, 0));
        assert_eq!(theme.ansi[15], Rgb::new(255, 255, 255));
    }

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

    #[test]
    fn dracula_preset_valid_and_distinct() {
        let theme = Theme::dracula();
        assert_eq!(theme.name, "dracula");
        assert_eq!(theme.ansi.len(), 16);
        for (i, color) in theme.ansi.iter().enumerate() {
            assert_ne!(
                *color, theme.background,
                "ANSI color {i} must not match background"
            );
        }
        assert_ne!(theme.foreground, theme.background);
    }

    #[test]
    fn dracula_toml_matches_dracula_preset() {
        let toml_str = include_str!("../themes/dracula.toml");
        let parsed = Theme::from_toml(toml_str).expect("dracula.toml should parse");
        let expected = Theme::dracula();
        assert_eq!(parsed.name, expected.name);
        assert_eq!(parsed.foreground, expected.foreground);
        assert_eq!(parsed.background, expected.background);
        assert_eq!(parsed.cursor, expected.cursor);
        assert_eq!(parsed.selection_bg, expected.selection_bg);
        assert_eq!(parsed.ansi, expected.ansi);
    }

    #[test]
    fn solarized_dark_toml_matches_preset() {
        let toml_str = include_str!("../themes/solarized-dark.toml");
        let parsed = Theme::from_toml(toml_str).expect("solarized-dark.toml should parse");
        let expected = Theme::solarized_dark();
        assert_eq!(parsed.name, expected.name);
        assert_eq!(parsed.foreground, expected.foreground);
        assert_eq!(parsed.background, expected.background);
        assert_eq!(parsed.ansi, expected.ansi);
        assert_ne!(parsed.foreground, parsed.background);
    }

    #[test]
    fn solarized_light_toml_matches_preset() {
        let toml_str = include_str!("../themes/solarized-light.toml");
        let parsed = Theme::from_toml(toml_str).expect("solarized-light.toml should parse");
        let expected = Theme::solarized_light();
        assert_eq!(parsed.name, expected.name);
        assert_eq!(parsed.foreground, expected.foreground);
        assert_eq!(parsed.background, expected.background);
        assert_eq!(parsed.ansi, expected.ansi);
        assert_ne!(parsed.foreground, parsed.background);
    }

    #[test]
    fn gruvbox_dark_toml_matches_preset() {
        let toml_str = include_str!("../themes/gruvbox-dark.toml");
        let parsed = Theme::from_toml(toml_str).expect("gruvbox-dark.toml should parse");
        let expected = Theme::gruvbox_dark();
        assert_eq!(parsed.name, expected.name);
        assert_eq!(parsed.foreground, expected.foreground);
        assert_eq!(parsed.background, expected.background);
        assert_eq!(parsed.ansi, expected.ansi);
        assert_ne!(parsed.foreground, parsed.background);
    }

    #[test]
    fn nord_toml_matches_preset() {
        let toml_str = include_str!("../themes/nord.toml");
        let parsed = Theme::from_toml(toml_str).expect("nord.toml should parse");
        let expected = Theme::nord();
        assert_eq!(parsed.name, expected.name);
        assert_eq!(parsed.foreground, expected.foreground);
        assert_eq!(parsed.background, expected.background);
        assert_eq!(parsed.ansi, expected.ansi);
        assert_ne!(parsed.foreground, parsed.background);
    }

    #[test]
    fn one_dark_toml_matches_preset() {
        let toml_str = include_str!("../themes/one-dark.toml");
        let parsed = Theme::from_toml(toml_str).expect("one-dark.toml should parse");
        let expected = Theme::one_dark();
        assert_eq!(parsed.name, expected.name);
        assert_eq!(parsed.foreground, expected.foreground);
        assert_eq!(parsed.background, expected.background);
        assert_eq!(parsed.ansi, expected.ansi);
        assert_ne!(parsed.foreground, parsed.background);
    }

    #[test]
    fn all_builtin_presets_are_valid_and_resolvable() {
        let presets = Theme::builtin_presets();
        assert_eq!(presets.len(), 7);
        for name in presets {
            let theme = Theme::preset_by_name(name).expect("preset should resolve");
            assert_eq!(theme.name, *name);
            assert_eq!(theme.ansi.len(), 16);
            assert_ne!(theme.foreground, theme.background);
        }
        assert!(Theme::preset_by_name("nonexistent-theme").is_none());
    }
}
