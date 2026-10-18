//! Property-based tests for arbitrary byte input on the VT parser.
//!
//! Verifies parser robustness, streaming chunking equivalence, adversarial input handling,
//! state recovery, and UTF-8 multi-byte reconstruction under arbitrary byte distributions.

use super::*;

/// Deterministic linear congruential pseudo-random number generator for reproducible property tests.
struct LcgRng {
    state: u64,
}

impl LcgRng {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    #[allow(clippy::cast_possible_truncation)]
    fn next_u32(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.state >> 32) as u32
    }

    #[allow(clippy::cast_possible_truncation)]
    fn next_u8(&mut self) -> u8 {
        (self.next_u32() & 0xFF) as u8
    }

    fn gen_bytes(&mut self, len: usize) -> Vec<u8> {
        let mut buf = Vec::with_capacity(len);
        for _ in 0..len {
            buf.push(self.next_u8());
        }
        buf
    }

    fn gen_range(&mut self, min: usize, max: usize) -> usize {
        if min >= max {
            return min;
        }
        min + (self.next_u32() as usize % (max - min))
    }
}

/// Property: For any arbitrary byte sequence of any length (from 0 to 65,536 bytes),
/// `Parser::advance` must never panic, loop indefinitely, or abort.
#[test]
fn property_arbitrary_random_bytes_never_panic() {
    let seeds = [
        0x1234_5678_9ABC_DEF0,
        0xDEAD_BEEF_CAFE_BABE,
        0x0123_4567_89AB_CDEF,
        0xFFFF_FFFF_FFFF_FFFF,
        0x0000_0000_0000_0001,
        0x5555_5555_5555_5555,
        0xAAAA_AAAA_AAAA_AAAA,
        0xFEED_FACE_DEAD_BEEF,
    ];

    for &seed in &seeds {
        let mut rng = LcgRng::new(seed);
        for &len in &[0, 1, 2, 7, 16, 64, 255, 1024, 8192, 32768] {
            let bytes = rng.gen_bytes(len);
            let mut parser = Parser::new();
            let mut handler = LoggingHandler::new();

            // Advance all at once
            parser.advance(&mut handler, &bytes);

            // Advance in random chunk sizes
            let mut chunk_parser = Parser::new();
            let mut chunk_handler = LoggingHandler::new();
            let mut offset = 0;
            while offset < bytes.len() {
                let chunk_size = rng.gen_range(1, 64).min(bytes.len() - offset);
                chunk_parser.advance(&mut chunk_handler, &bytes[offset..offset + chunk_size]);
                offset += chunk_size;
            }
        }
    }
}

/// Property: Streaming chunk invariance.
/// Feeding arbitrary structured terminal byte streams all at once vs byte-by-byte
/// must yield the identical sequence of Handler events.
#[test]
fn property_streaming_chunk_invariance_on_structured_sequences() {
    let mut rng = LcgRng::new(0x4242_4242_4242_4242);

    let snippets: &[&[u8]] = &[
        b"Hello, world!\r\n",
        b"\x1b[31mRed text\x1b[0m",
        b"\x1b[1;34mBold Blue\x1b[m",
        b"\x1b[38;2;255;100;50mTruecolor\x1b[0m",
        b"\x1b[38;5;196mIndexed 256\x1b[0m",
        b"\x1b[10;20H\x1b[2J\x1b[K",
        b"\x1b[?25h\x1b[?25l\x1b[?1049h\x1b[?1049l",
        b"\x1b]0;Dynamic Window Title\x07",
        b"\x1b[1 q\x1b[2 q\x1b[3 q\x1b[4 q\x1b[5 q\x1b[6 q",
        b"\x1b[2L\x1b[3M\x1b[4@\x1b[5P",
        b"\x1b7\x1b8\x1bM\x1b[5;20r",
        "Rust 🦀 日本語 🌟 Accent: é, ü, ç".as_bytes(),
    ];

    for _iteration in 0..50 {
        // Construct a synthetic compound stream from snippets
        let mut stream = Vec::new();
        let snippet_count = rng.gen_range(3, 10);
        for _ in 0..snippet_count {
            let idx = rng.gen_range(0, snippets.len());
            stream.extend_from_slice(snippets[idx]);
        }

        // Run 1: feed all at once
        let mut p1 = Parser::new();
        let mut h1 = LoggingHandler::new();
        p1.advance(&mut h1, &stream);

        // Run 2: feed byte-by-byte
        let mut p2 = Parser::new();
        let mut h2 = LoggingHandler::new();
        for &b in &stream {
            p2.advance(&mut h2, &[b]);
        }

        // Run 3: feed in arbitrary random chunks
        let mut p3 = Parser::new();
        let mut h3 = LoggingHandler::new();
        let mut offset = 0;
        while offset < stream.len() {
            let chunk = rng.gen_range(1, 13).min(stream.len() - offset);
            p3.advance(&mut h3, &stream[offset..offset + chunk]);
            offset += chunk;
        }

        assert_eq!(h1.events, h2.events, "byte-by-byte must match full stream");
        assert_eq!(
            h1.events, h3.events,
            "random chunking must match full stream"
        );
    }
}

/// Property: Truncated sequence safety and prefix isolation.
/// Truncating any valid escape sequence at any prefix byte must never panic or crash.
#[test]
fn property_truncated_sequences_never_panic() {
    let test_sequences: &[&[u8]] = &[
        b"\x1b[38;2;255;200;100m",
        b"\x1b[48;5;214m",
        b"\x1b[1;2;3;4;7;9m",
        b"\x1b[100;200H",
        b"\x1b[?1049h",
        b"\x1b]0;Very Long Title Sequence That Ends With Bell\x07",
        b"\x1b[12;34r",
        b"\x1b[3 q",
        "Multi-byte UTF-8 character: 🦀 (four bytes)".as_bytes(),
    ];

    for seq in test_sequences {
        for prefix_len in 0..seq.len() {
            let prefix = &seq[..prefix_len];
            let mut parser = Parser::new();
            let mut handler = LoggingHandler::new();
            parser.advance(&mut handler, prefix);
        }
    }
}

/// Property: Parser recovery after arbitrary corrupt input.
/// After receiving arbitrary malformed or noisy byte sequences, feeding a standard
/// sentinel string must reliably produce output without getting stuck.
#[test]
fn property_recovery_after_corrupted_stream() {
    let mut rng = LcgRng::new(0x9876_5432_10FE_DCBA);

    for _ in 0..100 {
        let noise_len = rng.gen_range(1, 512);
        let noise = rng.gen_bytes(noise_len);

        let mut parser = Parser::new();
        let mut handler = LoggingHandler::new();

        // Feed garbage noise
        parser.advance(&mut handler, &noise);

        // Feed recovery sentinel: reset attributes, carriage return, and text
        let sentinel = b"\r\n\x1b[0mOK";
        parser.advance(&mut handler, sentinel);

        // Verify the sentinel's print events were received at the tail
        let tail_events: Vec<_> = handler
            .events
            .iter()
            .rev()
            .take(4)
            .map(String::as_str)
            .collect();
        assert!(
            tail_events.contains(&"print('K')") && tail_events.contains(&"print('O')"),
            "parser must successfully recover and print characters after noise, tail events: {tail_events:?}"
        );
    }
}

/// Property: UTF-8 codepoint boundary splitting across arbitrary chunk sizes.
/// Feeding multi-byte UTF-8 text partitioned across every possible boundary must
/// reconstruct every codepoint in exact order.
#[test]
fn property_utf8_multibyte_arbitrary_fragmentation() {
    let text = "Hello 🦀 Rust 🚀 世界 — é, ü, ñ, 𝄞, 🌟!";
    let bytes = text.as_bytes();

    for split_point in 1..bytes.len() {
        let mut parser = Parser::new();
        let mut handler = LoggingHandler::new();

        parser.advance(&mut handler, &bytes[..split_point]);
        parser.advance(&mut handler, &bytes[split_point..]);

        let printed_chars: String = handler
            .events
            .iter()
            .filter_map(|e| {
                if let Some(ch_str) = e.strip_prefix("print('").and_then(|s| s.strip_suffix("')")) {
                    ch_str.chars().next()
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(
            printed_chars, text,
            "reconstructed text at split {split_point} must match original"
        );
    }
}
