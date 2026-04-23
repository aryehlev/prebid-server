//! Adaptive per-bidder timeout predictor.
//!
//! Maintains an exponentially-weighted moving mean and variance of observed
//! bidder response times and suggests a timeout of `mean + 2 * stddev`,
//! clamped into a sensible range.

use std::collections::HashMap;

/// EWMA smoothing factor for the running mean / variance.
const ALPHA: f64 = 0.2;
/// Minimum timeout the predictor will ever suggest (ms).
pub const MIN_TIMEOUT_MS: u64 = 50;
/// Maximum timeout the predictor will ever suggest (ms).
pub const MAX_TIMEOUT_MS: u64 = 5_000;
/// Default timeout returned for bidders we have no history for.
pub const DEFAULT_TIMEOUT_MS: u64 = 500;

/// Rolling EWMA statistics for a single bidder.
#[derive(Debug, Clone, Copy, Default)]
struct BidderStats {
    mean: f64,
    variance: f64,
    samples: u64,
}

impl BidderStats {
    fn observe(&mut self, elapsed_ms: f64) {
        if self.samples == 0 {
            self.mean = elapsed_ms;
            self.variance = 0.0;
        } else {
            // Welford-style EWMA update for mean and variance.
            let diff = elapsed_ms - self.mean;
            let new_mean = self.mean + ALPHA * diff;
            let new_var = (1.0 - ALPHA) * (self.variance + ALPHA * diff * diff);
            self.mean = new_mean;
            self.variance = new_var;
        }
        self.samples += 1;
    }

    fn stddev(&self) -> f64 {
        self.variance.max(0.0).sqrt()
    }
}

/// Adaptive timeout predictor across bidders.
#[derive(Debug, Default, Clone)]
pub struct TimeoutPredictor {
    bidders: HashMap<String, BidderStats>,
}

impl TimeoutPredictor {
    /// Creates a new empty timeout predictor.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an observed response time for a bidder, in milliseconds.
    pub fn observe(&mut self, bidder: &str, elapsed_ms: u64) {
        let entry = self.bidders.entry(bidder.to_string()).or_default();
        entry.observe(elapsed_ms as f64);
    }

    /// Returns `true` if any observations have been recorded for the given
    /// bidder.
    pub fn has_observations(&self, bidder: &str) -> bool {
        self.bidders
            .get(bidder)
            .map(|s| s.samples > 0)
            .unwrap_or(false)
    }

    /// Suggests a per-bidder timeout in milliseconds.
    ///
    /// For bidders with no history the [`DEFAULT_TIMEOUT_MS`] fallback is
    /// returned. Otherwise the predictor returns
    /// `mean + 2 * stddev`, clamped into `[MIN_TIMEOUT_MS, MAX_TIMEOUT_MS]`.
    pub fn suggest_timeout(&self, bidder: &str) -> u64 {
        match self.bidders.get(bidder) {
            Some(stats) if stats.samples > 0 => {
                let raw = stats.mean + 2.0 * stats.stddev();
                let rounded = raw.round().max(0.0) as u64;
                rounded.clamp(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS)
            }
            _ => DEFAULT_TIMEOUT_MS,
        }
    }

    /// Returns the current running mean response time for a bidder, if any.
    pub fn mean(&self, bidder: &str) -> Option<f64> {
        self.bidders
            .get(bidder)
            .filter(|s| s.samples > 0)
            .map(|s| s.mean)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_for_unknown_bidder() {
        let p = TimeoutPredictor::new();
        assert_eq!(p.suggest_timeout("unknown"), DEFAULT_TIMEOUT_MS);
    }

    #[test]
    fn converges_after_repeated_observations() {
        let mut p = TimeoutPredictor::new();
        for _ in 0..200 {
            p.observe("appnexus", 300);
        }
        let mean = p.mean("appnexus").unwrap();
        assert!((mean - 300.0).abs() < 1.0, "mean did not converge: {mean}");
        let t = p.suggest_timeout("appnexus");
        // With zero variance after convergence the suggestion should equal
        // the rounded mean, clamped into the allowed range.
        assert!(t >= MIN_TIMEOUT_MS);
        assert!(t <= MAX_TIMEOUT_MS);
        assert!(
            (t as i64 - 300).abs() <= 5,
            "timeout not near mean: got {t}"
        );
    }

    #[test]
    fn suggestion_accounts_for_variance() {
        let mut p = TimeoutPredictor::new();
        // Alternating fast/slow responses => higher variance => larger
        // suggested timeout than either sample alone.
        for i in 0..200 {
            let v = if i % 2 == 0 { 100 } else { 500 };
            p.observe("rubicon", v);
        }
        let t = p.suggest_timeout("rubicon");
        let mean = p.mean("rubicon").unwrap();
        assert!(t as f64 >= mean, "timeout {t} should cover mean {mean}");
    }

    #[test]
    fn suggestion_is_always_clamped() {
        let mut p = TimeoutPredictor::new();
        // Outlandishly large observations should still get clamped.
        p.observe("slowpoke", 1_000_000);
        let t = p.suggest_timeout("slowpoke");
        assert!(t <= MAX_TIMEOUT_MS);
    }
}
