//! `culla-bench`: benchmark harness (docs/benchmarks.md). Benchmarks arrive in M1.
//!
//! This crate is outside the removal and rename rules (ADR 0020): it only creates and
//! cleans up its own temporary directories.
#![allow(clippy::disallowed_methods)]

fn main() {
    println!(
        "culla-bench {} (no benchmarks yet)",
        culla_adapters::VERSION
    );
}
