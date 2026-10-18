//! Integration test exercising the fuzz target harness on standard and adversarial inputs.

use oxidite::parser::{LoggingHandler, Parser};
use oxidite::terminal::Terminal;

fn fuzz_one_input(data: &[u8]) {
    let mut parser = Parser::new();
    let mut handler = LoggingHandler::new();
    parser.advance(&mut handler, data);

    let mut term_parser = Parser::new();
    let mut term = Terminal::new(80, 24);
    term_parser.advance(&mut term, data);

    assert!(term.grid.cursor.row < term.grid.rows);
    assert!(term.grid.cursor.col < term.grid.cols);
}

#[test]
fn fuzz_target_exercises_corpus_and_mutations() {
    let corpus: &[&[u8]] = &[
        b"",
        b"\x1b",
        b"\x1b[",
        b"\x1b[38;2;255;0;128mHello\x1b[0m",
        b"\x1b[?1049h\x1b[H\x1b[2JAlt screen\x1b[?1049l",
        b"\x1b]0;Title with invalid \xff bytes\x07",
        b"\x1b[99999;99999H",
        b"\x1b[100M\x1b[100L",
        b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x0e\x0f\x1b\x7f\x80\xff",
    ];

    for &sample in corpus {
        fuzz_one_input(sample);
    }
}
