//! Color themes (stub — Phase 7).

/// 8-bit RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// Named theme palette.
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
    pub selection_bg: Rgb,
    pub ansi: [Rgb; 16],
}

impl Theme {
    /// Minimal dark default used until built-in presets land in Phase 7.
    pub fn default_dark() -> Self {
        let black = Rgb::new(0, 0, 0);
        let white = Rgb::new(255, 255, 255);
        Self {
            name: "default-dark".to_string(),
            foreground: white,
            background: black,
            cursor: white,
            selection_bg: Rgb::new(60, 60, 80),
            ansi: [black; 16],
        }
    }
}
