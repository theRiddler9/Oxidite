//! Throughput benchmark for Oxidite terminal parser and grid pipeline.
//!
//! Simulates `cat` of large files and `seq 1 100000` numerical dumps, measuring
//! throughput in megabytes per second (MB/s) and lines per second.

#![allow(
    clippy::cast_precision_loss,
    clippy::naive_bytecount,
    clippy::uninlined_format_args
)]

use oxidite::parser::Parser;
use oxidite::terminal::Terminal;
use std::time::Instant;

/// Generates a numerical sequence payload simulating `seq 1 100000`.
fn generate_seq_payload(count: usize) -> Vec<u8> {
    let mut payload = Vec::with_capacity(count * 8);
    for i in 1..=count {
        use std::io::Write;
        let _ = writeln!(payload, "{i}");
    }
    payload
}

/// Generates a realistic ANSI colored log file payload (~5 MB).
fn generate_colored_log_payload(lines: usize) -> Vec<u8> {
    let mut payload = Vec::with_capacity(lines * 80);
    let levels = [
        "\x1b[32mINFO\x1b[0m",
        "\x1b[33mWARN\x1b[0m",
        "\x1b[31;1mERROR\x1b[0m",
        "\x1b[34mDEBUG\x1b[0m",
        "\x1b[38;2;255;100;0mTRACE\x1b[0m",
    ];

    for i in 0..lines {
        let level = levels[i % levels.len()];
        let line = format!(
            "2026-10-19T00:00:{:02}.{:03}Z [{level}] Request #{i} processed in \x1b[1m{:.2}ms\x1b[0m - status: \x1b[32m200 OK\x1b[0m\r\n",
            i % 60,
            (i * 17) % 1000,
            (i as f64) * 0.123
        );
        payload.extend_from_slice(line.as_bytes());
    }
    payload
}

/// Runs a throughput benchmark with a specific chunk size.
fn run_benchmark(name: &str, data: &[u8], chunk_size: usize, rows: usize, cols: usize) {
    let mut parser = Parser::new();
    let mut terminal = Terminal::new(cols, rows);

    let start = Instant::now();
    let mut offset = 0;
    while offset < data.len() {
        let chunk = &data[offset..(offset + chunk_size).min(data.len())];
        parser.advance(&mut terminal, chunk);
        offset += chunk.len();
    }
    let elapsed = start.elapsed();
    let elapsed_secs = elapsed.as_secs_f64();

    let mbytes = data.len() as f64 / (1024.0 * 1024.0);
    let mb_per_sec = mbytes / elapsed_secs;
    let line_count = data.iter().filter(|&&b| b == b'\n').count();
    let lines_per_sec = line_count as f64 / elapsed_secs;

    println!(
        "{name:<35} | size: {:>6.2} MB | time: {:>7.3}s | throughput: {:>7.2} MB/s ({:>9.0} lines/s)",
        mbytes, elapsed_secs, mb_per_sec, lines_per_sec
    );
}

fn main() {
    println!("=== Oxidite Throughput Benchmarks ===");
    println!("{:-<95}", "");

    // 1. Numerical dump: seq 1 100000 (~770 KB)
    let seq_data = generate_seq_payload(100_000);
    run_benchmark("seq 1 100000 (chunk 4KB)", &seq_data, 4096, 24, 80);
    run_benchmark("seq 1 100000 (chunk 64KB)", &seq_data, 65536, 24, 80);

    // 2. Realistic ANSI colored logs (50,000 lines, ~5.2 MB)
    let log_data = generate_colored_log_payload(50_000);
    run_benchmark("ANSI colored logs 50k (chunk 4KB)", &log_data, 4096, 24, 80);
    run_benchmark(
        "ANSI colored logs 50k (chunk 16KB)",
        &log_data,
        16384,
        24,
        80,
    );
    run_benchmark(
        "ANSI colored logs 50k (chunk 64KB)",
        &log_data,
        65536,
        24,
        80,
    );

    println!("{:-<95}", "");
    println!("Benchmark completed successfully.");
}
