//! VT/ANSI parser wrapping `vte` (stub — Phase 2).

use crate::handler::Handler;

/// Parser that feeds bytes into a [`Handler`].
pub struct Parser {
    inner: vte::Parser,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    /// Creates an empty parser with no pending escape state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: vte::Parser::new(),
        }
    }

    /// Advances the parser with raw PTY bytes, calling `handler` as sequences complete.
    pub fn advance(&mut self, handler: &mut dyn Handler, bytes: &[u8]) {
        let mut dispatcher = Dispatcher { handler };
        self.inner.advance(&mut dispatcher, bytes);
    }
}

/// Helper returning parameter at `index` or `default` if missing or zero.
fn param_or(params: &vte::Params, index: usize, default: u16) -> u16 {
    params
        .iter()
        .nth(index)
        .and_then(|subparams| subparams.first().copied())
        .map_or(default, |val| if val == 0 { default } else { val })
}

/// Converts a 16-bit parameter to an `isize` displacement without wrapping.
fn to_isize(val: u16) -> isize {
    isize::try_from(val).unwrap_or(isize::MAX)
}

/// Internal dispatcher connecting [`vte::Perform`] events to a [`Handler`].
struct Dispatcher<'a> {
    handler: &'a mut dyn Handler,
}

impl vte::Perform for Dispatcher<'_> {
    fn print(&mut self, c: char) {
        self.handler.print(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x07 => self.handler.bell(),
            0x08 => self.handler.backspace(),
            0x09 => self.handler.tab(),
            0x0A..=0x0C => self.handler.linefeed(),
            0x0D => self.handler.carriage_return(),
            _ => {}
        }
    }

    fn hook(&mut self, _params: &vte::Params, _intermediates: &[u8], _ignore: bool, _action: char) {
    }

    fn put(&mut self, _byte: u8) {}

    fn unhook(&mut self) {}

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if matches!(params.first().copied(), Some(b"0" | b"2")) {
            let title = if params.len() > 1 {
                let title_bytes = params[1..].join(&b';');
                String::from_utf8_lossy(&title_bytes).into_owned()
            } else {
                String::new()
            };
            self.handler.set_title(title);
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        if intermediates.is_empty() {
            match action {
                // CUU — Cursor Up (CSI Ps A)
                'A' => {
                    let count = to_isize(param_or(params, 0, 1));
                    self.handler.move_cursor_by(-count, 0);
                }
                // CUD — Cursor Down (CSI Ps B)
                'B' => {
                    let count = to_isize(param_or(params, 0, 1));
                    self.handler.move_cursor_by(count, 0);
                }
                // CUF — Cursor Forward (CSI Ps C)
                'C' => {
                    let count = to_isize(param_or(params, 0, 1));
                    self.handler.move_cursor_by(0, count);
                }
                // CUB — Cursor Backward (CSI Ps D)
                'D' => {
                    let count = to_isize(param_or(params, 0, 1));
                    self.handler.move_cursor_by(0, -count);
                }
                // CUP / HVP — Cursor Position (CSI Ps ; Ps H / f)
                'H' | 'f' => {
                    let row = usize::from(param_or(params, 0, 1).saturating_sub(1));
                    let col = usize::from(param_or(params, 1, 1).saturating_sub(1));
                    self.handler.move_cursor_to(row, col);
                }
                // ED — Erase in Display (CSI Ps J)
                'J' => {
                    let mode = match param_or(params, 0, 0) {
                        1 => crate::handler::EraseMode::ToStart,
                        2 => crate::handler::EraseMode::All,
                        3 => crate::handler::EraseMode::AllIncludingScrollback,
                        _ => crate::handler::EraseMode::ToEnd,
                    };
                    self.handler.erase_in_display(mode);
                }
                // EL — Erase in Line (CSI Ps K)
                'K' => {
                    let mode = match param_or(params, 0, 0) {
                        1 => crate::handler::EraseMode::ToStart,
                        2 => crate::handler::EraseMode::All,
                        _ => crate::handler::EraseMode::ToEnd,
                    };
                    self.handler.erase_in_line(mode);
                }
                // SGR — Select Graphic Rendition (CSI Ps m)
                'm' => {
                    self.dispatch_sgr(params);
                }
                // DECSTBM — Set Top and Bottom Margins (CSI Pt ; Pb r)
                'r' => {
                    let top = usize::from(param_or(params, 0, 1).saturating_sub(1));
                    let bottom_param = param_or(params, 1, 0);
                    let bottom = if bottom_param == 0 {
                        0
                    } else {
                        usize::from(bottom_param.saturating_sub(1))
                    };
                    self.handler.set_scroll_region(top, bottom);
                }
                _ => {}
            }
        } else if intermediates == b"?" {
            // DECSET (h) / DECRST (l)
            let enabled = match action {
                'h' => true,
                'l' => false,
                _ => return,
            };
            for subparams in params {
                let code = subparams.first().copied().unwrap_or(0);
                let mode = match code {
                    1 => crate::handler::Mode::ApplicationCursorKeys,
                    7 => crate::handler::Mode::Autowrap,
                    25 => crate::handler::Mode::CursorVisible,
                    1000 | 1002 | 1003 | 1006 => crate::handler::Mode::MouseReporting,
                    47 | 1047 | 1049 => crate::handler::Mode::AlternateScreen,
                    2004 => crate::handler::Mode::BracketedPaste,
                    _ => continue,
                };
                self.handler.set_mode(mode, enabled);
            }
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}

impl Dispatcher<'_> {
    fn dispatch_sgr(&mut self, params: &vte::Params) {
        if params.is_empty() {
            self.handler.set_sgr(crate::handler::SgrAttr::Reset);
            return;
        }

        let mut iter = params.into_iter();
        while let Some(subparams) = iter.next() {
            let code = subparams.first().copied().unwrap_or(0);
            match code {
                0 => self.handler.set_sgr(crate::handler::SgrAttr::Reset),
                1 => self.handler.set_sgr(crate::handler::SgrAttr::Bold),
                2 => self.handler.set_sgr(crate::handler::SgrAttr::Dim),
                3 => self.handler.set_sgr(crate::handler::SgrAttr::Italic),
                4 => self.handler.set_sgr(crate::handler::SgrAttr::Underline),
                7 => self.handler.set_sgr(crate::handler::SgrAttr::Inverse),
                8 => self.handler.set_sgr(crate::handler::SgrAttr::Hidden),
                9 => self.handler.set_sgr(crate::handler::SgrAttr::Strikethrough),
                22 => self
                    .handler
                    .set_sgr(crate::handler::SgrAttr::NormalIntensity),
                23 => self.handler.set_sgr(crate::handler::SgrAttr::NotItalic),
                24 => self.handler.set_sgr(crate::handler::SgrAttr::NotUnderline),
                27 => self.handler.set_sgr(crate::handler::SgrAttr::NotInverse),
                28 => self.handler.set_sgr(crate::handler::SgrAttr::NotHidden),
                29 => self
                    .handler
                    .set_sgr(crate::handler::SgrAttr::NotStrikethrough),
                // 16-color standard foreground (30–37)
                30..=37 => {
                    let idx = u8::try_from(code - 30).unwrap_or(0);
                    self.handler.set_sgr(crate::handler::SgrAttr::Foreground(
                        crate::grid::Color::Indexed(idx),
                    ));
                }
                // Extended foreground (256-color / truecolor)
                38 => {
                    if let Some(color) = parse_extended_color(subparams, &mut iter) {
                        self.handler
                            .set_sgr(crate::handler::SgrAttr::Foreground(color));
                    }
                }
                // Default foreground (39)
                39 => {
                    self.handler.set_sgr(crate::handler::SgrAttr::Foreground(
                        crate::grid::Color::Default,
                    ));
                }
                // 16-color standard background (40–47)
                40..=47 => {
                    let idx = u8::try_from(code - 40).unwrap_or(0);
                    self.handler.set_sgr(crate::handler::SgrAttr::Background(
                        crate::grid::Color::Indexed(idx),
                    ));
                }
                // Extended background (256-color / truecolor)
                48 => {
                    if let Some(color) = parse_extended_color(subparams, &mut iter) {
                        self.handler
                            .set_sgr(crate::handler::SgrAttr::Background(color));
                    }
                }
                // Default background (49)
                49 => {
                    self.handler.set_sgr(crate::handler::SgrAttr::Background(
                        crate::grid::Color::Default,
                    ));
                }
                // 16-color bright foreground (90–97)
                90..=97 => {
                    let idx = u8::try_from(code - 90 + 8).unwrap_or(8);
                    self.handler.set_sgr(crate::handler::SgrAttr::Foreground(
                        crate::grid::Color::Indexed(idx),
                    ));
                }
                // 16-color bright background (100–107)
                100..=107 => {
                    let idx = u8::try_from(code - 100 + 8).unwrap_or(8);
                    self.handler.set_sgr(crate::handler::SgrAttr::Background(
                        crate::grid::Color::Indexed(idx),
                    ));
                }
                _ => {}
            }
        }
    }
}

/// Parses extended 256-color or 24-bit truecolor from semicolon or colon subparameters.
fn parse_extended_color<'a, I>(subparams: &[u16], iter: &mut I) -> Option<crate::grid::Color>
where
    I: Iterator<Item = &'a [u16]>,
{
    if subparams.len() > 1 {
        // Colon-delimited format (e.g. 38:5:123 or 38:2::r:g:b or 38:2:r:g:b)
        let mode = subparams.get(1).copied()?;
        match mode {
            5 => {
                let idx = u8::try_from(*subparams.get(2)?).ok()?;
                Some(crate::grid::Color::Indexed(idx))
            }
            2 => {
                if subparams.len() >= 6 {
                    let r = u8::try_from(subparams[3]).ok()?;
                    let g = u8::try_from(subparams[4]).ok()?;
                    let b = u8::try_from(subparams[5]).ok()?;
                    Some(crate::grid::Color::Rgb(r, g, b))
                } else if subparams.len() >= 5 {
                    let r = u8::try_from(subparams[2]).ok()?;
                    let g = u8::try_from(subparams[3]).ok()?;
                    let b = u8::try_from(subparams[4]).ok()?;
                    Some(crate::grid::Color::Rgb(r, g, b))
                } else {
                    None
                }
            }
            _ => None,
        }
    } else {
        // Semicolon-delimited format: subsequent tokens are consumed from `iter`.
        let mode = iter.next()?.first().copied()?;
        match mode {
            5 => {
                let idx_val = iter.next()?.first().copied()?;
                let idx = u8::try_from(idx_val).ok()?;
                Some(crate::grid::Color::Indexed(idx))
            }
            2 => {
                let r_val = iter.next()?.first().copied()?;
                let g_val = iter.next()?.first().copied()?;
                let b_val = iter.next()?.first().copied()?;
                let r = u8::try_from(r_val).ok()?;
                let g = u8::try_from(g_val).ok()?;
                let b = u8::try_from(b_val).ok()?;
                Some(crate::grid::Color::Rgb(r, g, b))
            }
            _ => None,
        }
    }
}

/// A [`Handler`] implementation that logs method calls and records them for debugging and tests.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LoggingHandler {
    /// Recorded call trace formatted as human-readable strings.
    pub events: Vec<String>,
}

impl LoggingHandler {
    /// Creates a new, empty logging handler.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears recorded events.
    pub fn clear(&mut self) {
        self.events.clear();
    }
}

impl Handler for LoggingHandler {
    fn print(&mut self, c: char) {
        self.events.push(format!("print({c:?})"));
    }

    fn linefeed(&mut self) {
        self.events.push("linefeed".into());
    }

    fn carriage_return(&mut self) {
        self.events.push("carriage_return".into());
    }

    fn backspace(&mut self) {
        self.events.push("backspace".into());
    }

    fn tab(&mut self) {
        self.events.push("tab".into());
    }

    fn bell(&mut self) {
        self.events.push("bell".into());
    }

    fn move_cursor_to(&mut self, row: usize, col: usize) {
        self.events.push(format!("move_cursor_to({row}, {col})"));
    }

    fn move_cursor_by(&mut self, d_row: isize, d_col: isize) {
        self.events
            .push(format!("move_cursor_by({d_row}, {d_col})"));
    }

    fn erase_in_display(&mut self, mode: crate::handler::EraseMode) {
        self.events.push(format!("erase_in_display({mode:?})"));
    }

    fn erase_in_line(&mut self, mode: crate::handler::EraseMode) {
        self.events.push(format!("erase_in_line({mode:?})"));
    }

    fn set_sgr(&mut self, attr: crate::handler::SgrAttr) {
        self.events.push(format!("set_sgr({attr:?})"));
    }

    fn set_mode(&mut self, mode: crate::handler::Mode, enabled: bool) {
        self.events.push(format!("set_mode({mode:?}, {enabled})"));
    }

    fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        self.events
            .push(format!("set_scroll_region({top}, {bottom})"));
    }

    fn set_title(&mut self, title: String) {
        self.events.push(format!("set_title({title:?})"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logging_handler_records_calls() {
        let mut handler = LoggingHandler::new();
        handler.bell();
        handler.carriage_return();
        handler.linefeed();
        assert_eq!(handler.events, vec!["bell", "carriage_return", "linefeed"]);
        handler.clear();
        assert_eq!(handler.events, Vec::<String>::new());
    }

    #[test]
    fn parser_handles_print_and_c0() {
        let mut parser = Parser::new();
        let mut handler = LoggingHandler::new();

        parser.advance(&mut handler, b"Hi\r\n\t\x08\x07");
        assert_eq!(
            handler.events,
            vec![
                "print('H')",
                "print('i')",
                "carriage_return",
                "linefeed",
                "tab",
                "backspace",
                "bell",
            ]
        );
    }

    #[test]
    fn parser_handles_csi_cursor_movement() {
        let mut parser = Parser::new();
        let mut handler = LoggingHandler::new();

        // CUU, CUD, CUF, CUB with default and explicit parameters
        parser.advance(&mut handler, b"\x1b[A\x1b[3B\x1b[5C\x1b[2D");
        // CUP and HVP
        parser.advance(&mut handler, b"\x1b[H\x1b[12;34H\x1b[5;10f");

        assert_eq!(
            handler.events,
            vec![
                "move_cursor_by(-1, 0)",
                "move_cursor_by(3, 0)",
                "move_cursor_by(0, 5)",
                "move_cursor_by(0, -2)",
                "move_cursor_to(0, 0)",
                "move_cursor_to(11, 33)",
                "move_cursor_to(4, 9)",
            ]
        );
    }

    #[test]
    fn parser_handles_csi_erase() {
        let mut parser = Parser::new();
        let mut handler = LoggingHandler::new();

        // ED (J): default (0), 1, 2, 3
        parser.advance(&mut handler, b"\x1b[J\x1b[1J\x1b[2J\x1b[3J");
        // EL (K): default (0), 1, 2
        parser.advance(&mut handler, b"\x1b[K\x1b[1K\x1b[2K");

        assert_eq!(
            handler.events,
            vec![
                "erase_in_display(ToEnd)",
                "erase_in_display(ToStart)",
                "erase_in_display(All)",
                "erase_in_display(AllIncludingScrollback)",
                "erase_in_line(ToEnd)",
                "erase_in_line(ToStart)",
                "erase_in_line(All)",
            ]
        );
    }

    #[test]
    fn parser_handles_sgr_16_color() {
        let mut parser = Parser::new();
        let mut handler = LoggingHandler::new();

        // 31 (red fg), 42 (green bg), 39 (default fg), 49 (default bg)
        parser.advance(&mut handler, b"\x1b[31;42;39;49m");
        // 94 (bright blue fg), 105 (bright magenta bg)
        parser.advance(&mut handler, b"\x1b[94;105m");

        assert_eq!(
            handler.events,
            vec![
                "set_sgr(Foreground(Indexed(1)))",
                "set_sgr(Background(Indexed(2)))",
                "set_sgr(Foreground(Default))",
                "set_sgr(Background(Default))",
                "set_sgr(Foreground(Indexed(12)))",
                "set_sgr(Background(Indexed(13)))",
            ]
        );
    }

    #[test]
    fn parser_handles_sgr_256_and_truecolor() {
        let mut parser = Parser::new();
        let mut handler = LoggingHandler::new();

        // Semicolon 256-color: fg 123, bg 42
        parser.advance(&mut handler, b"\x1b[38;5;123;48;5;42m");
        // Semicolon truecolor: fg rgb(255,100,0), bg rgb(10,20,30)
        parser.advance(&mut handler, b"\x1b[38;2;255;100;0;48;2;10;20;30m");
        // Colon 256-color and truecolor
        parser.advance(&mut handler, b"\x1b[38:5:200m\x1b[38:2::50:60:70m");

        assert_eq!(
            handler.events,
            vec![
                "set_sgr(Foreground(Indexed(123)))",
                "set_sgr(Background(Indexed(42)))",
                "set_sgr(Foreground(Rgb(255, 100, 0)))",
                "set_sgr(Background(Rgb(10, 20, 30)))",
                "set_sgr(Foreground(Indexed(200)))",
                "set_sgr(Foreground(Rgb(50, 60, 70)))",
            ]
        );
    }

    #[test]
    fn parser_handles_sgr_text_styles() {
        let mut parser = Parser::new();
        let mut handler = LoggingHandler::new();

        // bold, dim, italic, underline, inverse, hidden, strikethrough
        parser.advance(&mut handler, b"\x1b[1;2;3;4;7;8;9m");
        // resets: normal intensity, not italic, not underline, not inverse, not hidden, not strikethrough, full reset
        parser.advance(&mut handler, b"\x1b[22;23;24;27;28;29;0m");
        // empty param reset
        parser.advance(&mut handler, b"\x1b[m");

        assert_eq!(
            handler.events,
            vec![
                "set_sgr(Bold)",
                "set_sgr(Dim)",
                "set_sgr(Italic)",
                "set_sgr(Underline)",
                "set_sgr(Inverse)",
                "set_sgr(Hidden)",
                "set_sgr(Strikethrough)",
                "set_sgr(NormalIntensity)",
                "set_sgr(NotItalic)",
                "set_sgr(NotUnderline)",
                "set_sgr(NotInverse)",
                "set_sgr(NotHidden)",
                "set_sgr(NotStrikethrough)",
                "set_sgr(Reset)",
                "set_sgr(Reset)",
            ]
        );
    }

    #[test]
    fn parser_handles_decset_decrst() {
        let mut parser = Parser::new();
        let mut handler = LoggingHandler::new();

        // Enable: autowrap (?7), cursor visible (?25), alt screen (?1049), bracketed paste (?2004)
        parser.advance(&mut handler, b"\x1b[?7;25;1049;2004h");
        // Disable: app cursor keys (?1), mouse reporting (?1006)
        parser.advance(&mut handler, b"\x1b[?1;1006l");

        assert_eq!(
            handler.events,
            vec![
                "set_mode(Autowrap, true)",
                "set_mode(CursorVisible, true)",
                "set_mode(AlternateScreen, true)",
                "set_mode(BracketedPaste, true)",
                "set_mode(ApplicationCursorKeys, false)",
                "set_mode(MouseReporting, false)",
            ]
        );
    }

    #[test]
    fn parser_handles_scroll_region() {
        let mut parser = Parser::new();
        let mut handler = LoggingHandler::new();

        // Default / reset scroll region
        parser.advance(&mut handler, b"\x1b[r");
        // Explicit scroll region lines 5 to 24 (1-indexed) -> 4 to 23 (0-indexed)
        parser.advance(&mut handler, b"\x1b[5;24r");

        assert_eq!(
            handler.events,
            vec!["set_scroll_region(0, 0)", "set_scroll_region(4, 23)",]
        );
    }

    #[test]
    fn parser_handles_osc_title() {
        let mut parser = Parser::new();
        let mut handler = LoggingHandler::new();

        // OSC 0 with BEL terminator
        parser.advance(&mut handler, b"\x1b]0;Oxidite Shell\x07");
        // OSC 2 with ST terminator (\x1b\\)
        parser.advance(&mut handler, b"\x1b]2;Project - Oxidite; v1\x1b\\");

        assert_eq!(
            handler.events,
            vec![
                "set_title(\"Oxidite Shell\")",
                "set_title(\"Project - Oxidite; v1\")",
            ]
        );
    }
}
