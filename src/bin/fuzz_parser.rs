//! Standalone fuzz target and fuzzing harness for Oxidite's VT parser and Terminal state.
//!
//! Can be run directly:
//!   `cargo run --bin fuzz_parser -- --iterations 1000`
//! Or used with stdin to test specific crash reproduction files:
//!   `cargo run --bin fuzz_parser < crash_input.bin`

use oxidite::parser::{LoggingHandler, Parser};
use oxidite::terminal::Terminal;
use std::io::Read;

/// Mutates an input byte slice to produce variations for fuzz exploration.
#[allow(clippy::cast_possible_truncation)]
fn mutate_bytes(input: &[u8], seed: u64) -> Vec<u8> {
    let mut out = input.to_vec();
    let mut state = seed;
    let next_u32 = |st: &mut u64| {
        *st = st
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (*st >> 32) as u32
    };

    if out.is_empty() {
        let len = (next_u32(&mut state) % 64) as usize + 1;
        for _ in 0..len {
            out.push((next_u32(&mut state) & 0xFF) as u8);
        }
        return out;
    }

    let mutations = (next_u32(&mut state) % 4) + 1;
    for _ in 0..mutations {
        let op = next_u32(&mut state) % 4;
        let idx = (next_u32(&mut state) as usize) % out.len();
        match op {
            0 => {
                // Flip random bit
                let bit = 1u8 << (next_u32(&mut state) % 8);
                out[idx] ^= bit;
            }
            1 => {
                // Insert random byte
                let byte = (next_u32(&mut state) & 0xFF) as u8;
                out.insert(idx, byte);
            }
            2 if out.len() > 1 => {
                // Delete byte
                out.remove(idx);
            }
            _ => {
                // Replace with control byte or escape sequence artifact
                let specials = [0x00, 0x1B, 0x9B, 0x07, 0x08, 0x0A, 0x0D, b'[', b';', b'm'];
                let chosen = specials[(next_u32(&mut state) as usize) % specials.len()];
                out[idx] = chosen;
            }
        }
    }
    out
}

/// Fuzz harness: exercises both `LoggingHandler` and `Terminal` state with arbitrary byte data.
pub fn fuzz_one_input(data: &[u8]) {
    // 1. Test parser with LoggingHandler
    let mut parser = Parser::new();
    let mut handler = LoggingHandler::new();
    parser.advance(&mut handler, data);

    // 2. Test parser with real Terminal grid state to verify invariant preservation
    let mut term_parser = Parser::new();
    let mut term = Terminal::new(80, 24);
    term_parser.advance(&mut term, data);

    // Verify grid cursor invariants
    assert!(
        term.grid.cursor.row < term.grid.rows,
        "Cursor row must remain within bounds"
    );
    assert!(
        term.grid.cursor.col < term.grid.cols,
        "Cursor col must remain within bounds"
    );
}

#[allow(clippy::cast_possible_truncation)]
fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Check if --stdin flag was passed
    if args.iter().any(|a| a == "--stdin") {
        let mut buffer = Vec::new();
        if std::io::stdin().read_to_end(&mut buffer).is_ok() && !buffer.is_empty() {
            fuzz_one_input(&buffer);
            println!(
                "Fuzz target successfully executed {} bytes from stdin without panics.",
                buffer.len()
            );
            return;
        }
    }

    // Check if --file <path> was passed
    if let Some(file_idx) = args.iter().position(|a| a == "--file")
        && let Some(path) = args.get(file_idx + 1)
    {
        let data = std::fs::read(path).expect("failed to read target file");
        fuzz_one_input(&data);
        println!(
            "Fuzz target successfully executed {} bytes from file '{path}' without panics.",
            data.len()
        );
        return;
    }

    // Parse iteration count argument
    let iterations: usize = args
        .iter()
        .position(|a| a == "--iterations" || a == "-n")
        .and_then(|idx| args.get(idx + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(2_000);

    println!("Starting Oxidite parser fuzz harness for {iterations} iterations...");

    // Seed corpus with interesting VT sequences
    let mut corpus: Vec<Vec<u8>> = vec![
        b"Hello world\r\n".to_vec(),
        b"\x1b[31;1;4mUnderline Red Bold\x1b[0m".to_vec(),
        b"\x1b[38;2;255;128;64mTruecolor\x1b[0m".to_vec(),
        b"\x1b[10;20H\x1b[2J\x1b[1K".to_vec(),
        b"\x1b]0;OSC Window Title Test\x07".to_vec(),
        b"\x1b[?1049h\x1b[?25l".to_vec(),
        "🦀 🚀 日本語 🌟 Unicode multibyte".as_bytes().to_vec(),
    ];

    for i in 0..iterations {
        let seed = (i as u64).wrapping_mul(0x517C_C1B7_2722_0A95);
        let parent_idx = (seed as usize) % corpus.len();
        let mutated = mutate_bytes(&corpus[parent_idx], seed);

        fuzz_one_input(&mutated);

        if mutated.len() < 1024 && corpus.len() < 500 && seed.is_multiple_of(10) {
            corpus.push(mutated);
        }

        if (i + 1) % 500 == 0 || i + 1 == iterations {
            println!("  [{}/{}] iterations completed cleanly.", i + 1, iterations);
        }
    }

    println!(
        "Fuzzing complete: 0 crashes or invariant violations found across {iterations} inputs."
    );
}
