#![no_main]

use libfuzzer_sys::fuzz_target;
use oxidite::parser::{LoggingHandler, Parser};
use oxidite::terminal::Terminal;

fuzz_target!(|data: &[u8]| {
    // 1. Advance parser with LoggingHandler
    let mut parser = Parser::new();
    let mut handler = LoggingHandler::new();
    parser.advance(&mut handler, data);

    // 2. Advance parser with Terminal state to verify invariants
    let mut term_parser = Parser::new();
    let mut term = Terminal::new(80, 24);
    term_parser.advance(&mut term, data);

    assert!(term.grid.cursor.row < term.grid.rows);
    assert!(term.grid.cursor.col < term.grid.cols);
});
