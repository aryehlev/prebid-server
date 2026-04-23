//! Micro-benchmark helpers for the experimental parser.
//!
//! These utilities are intentionally tiny and live in the main crate
//! (rather than behind `#[cfg(bench)]`) so they can be invoked from regular
//! binaries during ad-hoc experimentation.

use std::time::{Duration, Instant};

use crate::parser::parse_request_owned;

/// Parse `input` `iters` times, cloning the byte buffer on every iteration
/// because `simd-json` destructively modifies its input. Returns the total
/// wall-clock time spent inside the parse loop.
pub fn bench_parse(input: &[u8], iters: usize) -> Duration {
    let start = Instant::now();
    for _ in 0..iters {
        let mut buf = input.to_vec();
        // Ignore the result: bench loop should not early-return on malformed
        // payloads supplied by a caller — we want consistent iteration counts.
        let _ = parse_request_owned(&mut buf);
    }
    start.elapsed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bench_parse_runs_without_panic() {
        let input = br#"{"id":"r","imp":[{"id":"i"}]}"#;
        let d = bench_parse(input, 16);
        // `Duration` is always non-negative; just sanity-check that it ran.
        assert!(d >= Duration::from_nanos(0));
    }

    #[test]
    fn bench_parse_handles_zero_iters() {
        let d = bench_parse(b"{}", 0);
        assert!(d >= Duration::from_nanos(0));
    }
}
