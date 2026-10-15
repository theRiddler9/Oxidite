//! Oxidite — cross-platform terminal emulator.
//!
//! Phase 0: contract stubs only. Real PTY passthrough starts in Phase 1.

// Contract stubs are intentionally unused until later phases wire them up.
#![allow(dead_code)]

mod app;
mod config;
mod grid;
mod handler;
mod input;
mod parser;
mod pty;
mod renderer;
mod terminal;
mod theme;

/// Entry point for Oxidite.
fn main() {
    println!("Oxidite — Phase 0 scaffold ready");
}
