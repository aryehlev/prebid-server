//! Tmax (timeout) adjustments for bidder requests.
//!
//! Mirrors the Go implementation in `exchange/tmax_adjustments.go`.
//!
//! The tmax adjustment logic allows PBS to subtract time from the request's tmax
//! to account for network latency and response preparation overhead, giving
//! bidders a more accurate timeout for their own processing.

use std::time::Instant;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Raw tmax adjustment configuration (from config file / environment).
#[derive(Debug, Clone, Default)]
pub struct TmaxAdjustmentsConfig {
    /// Whether tmax adjustments are enabled.
    pub enabled: bool,
    /// Estimated network latency buffer (in ms) between PBS and bidders.
    pub bidder_network_latency_buffer_ms: u64,
    /// Time (in ms) PBS needs after bidder responses to prepare the final response.
    pub pbs_response_preparation_duration_ms: u64,
    /// Minimum allowed bidder response duration (in ms). If the calculated tmax
    /// falls below this, the original request tmax is used instead.
    pub bidder_response_duration_min_ms: u64,
}

/// Pre-processed tmax adjustments ready for use during the auction.
///
/// Created from [`TmaxAdjustmentsConfig`] via [`TmaxAdjustments::from_config`].
/// If adjustments are not enabled or not enforceable, this will be `None`.
#[derive(Debug, Clone)]
pub struct TmaxAdjustments {
    /// Network latency buffer in ms.
    pub bidder_network_latency_buffer: u64,
    /// PBS response preparation time in ms.
    pub pbs_response_preparation_duration: u64,
    /// Minimum bidder response duration in ms.
    pub bidder_response_duration_min: u64,
}

impl TmaxAdjustments {
    /// Process the config into a usable adjustments struct.
    ///
    /// Returns `None` if adjustments are disabled or not enforceable.
    /// Enforcement requires `bidder_response_duration_min_ms > 0` and at least one
    /// of the latency/preparation durations to be non-zero.
    pub fn from_config(config: &TmaxAdjustmentsConfig) -> Option<Self> {
        if !config.enabled {
            return None;
        }

        let is_enforced = config.bidder_response_duration_min_ms > 0
            && (config.bidder_network_latency_buffer_ms > 0
                || config.pbs_response_preparation_duration_ms > 0);

        if !is_enforced {
            return None;
        }

        Some(Self {
            bidder_network_latency_buffer: config.bidder_network_latency_buffer_ms,
            pbs_response_preparation_duration: config.pbs_response_preparation_duration_ms,
            bidder_response_duration_min: config.bidder_response_duration_min_ms,
        })
    }
}

// ---------------------------------------------------------------------------
// Bidder tmax calculation
// ---------------------------------------------------------------------------

/// Calculates the effective tmax for a bidder request.
///
/// When tmax adjustments are active and a deadline is provided, the function
/// computes the remaining time until the deadline and subtracts the network
/// latency buffer and PBS response preparation duration.
///
/// If the result falls below `bidder_response_duration_min`, the original
/// `request_tmax_ms` is returned instead.
///
/// # Arguments
/// * `deadline` - Optional deadline instant for the auction context
/// * `start` - The instant from which remaining time is measured (typically `Instant::now()`)
/// * `request_tmax_ms` - The original tmax from the bid request (in ms)
/// * `adjustments` - Optional pre-processed tmax adjustments
pub fn get_bidder_tmax(
    deadline: Option<Instant>,
    start: Instant,
    request_tmax_ms: i64,
    adjustments: Option<&TmaxAdjustments>,
) -> i64 {
    if let Some(adj) = adjustments {
        if let Some(dl) = deadline {
            if dl > start {
                let remaining_ms = dl.duration_since(start).as_millis() as i64;
                let adjusted = remaining_ms
                    - adj.bidder_network_latency_buffer as i64
                    - adj.pbs_response_preparation_duration as i64;

                if adjusted >= adj.bidder_response_duration_min as i64 {
                    return adjusted;
                }
            }
        }
    }
    request_tmax_ms
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_from_config_disabled() {
        let config = TmaxAdjustmentsConfig {
            enabled: false,
            bidder_network_latency_buffer_ms: 10,
            pbs_response_preparation_duration_ms: 20,
            bidder_response_duration_min_ms: 50,
        };
        assert!(TmaxAdjustments::from_config(&config).is_none());
    }

    #[test]
    fn test_from_config_no_min_duration() {
        let config = TmaxAdjustmentsConfig {
            enabled: true,
            bidder_network_latency_buffer_ms: 10,
            pbs_response_preparation_duration_ms: 20,
            bidder_response_duration_min_ms: 0,
        };
        assert!(TmaxAdjustments::from_config(&config).is_none());
    }

    #[test]
    fn test_from_config_no_buffer_or_prep() {
        let config = TmaxAdjustmentsConfig {
            enabled: true,
            bidder_network_latency_buffer_ms: 0,
            pbs_response_preparation_duration_ms: 0,
            bidder_response_duration_min_ms: 50,
        };
        assert!(TmaxAdjustments::from_config(&config).is_none());
    }

    #[test]
    fn test_from_config_valid() {
        let config = TmaxAdjustmentsConfig {
            enabled: true,
            bidder_network_latency_buffer_ms: 10,
            pbs_response_preparation_duration_ms: 20,
            bidder_response_duration_min_ms: 50,
        };
        let adj = TmaxAdjustments::from_config(&config);
        assert!(adj.is_some());
        let adj = adj.unwrap();
        assert_eq!(adj.bidder_network_latency_buffer, 10);
        assert_eq!(adj.pbs_response_preparation_duration, 20);
        assert_eq!(adj.bidder_response_duration_min, 50);
    }

    #[test]
    fn test_get_bidder_tmax_no_adjustments() {
        let now = Instant::now();
        let result = get_bidder_tmax(None, now, 500, None);
        assert_eq!(result, 500);
    }

    #[test]
    fn test_get_bidder_tmax_no_deadline() {
        let adj = TmaxAdjustments {
            bidder_network_latency_buffer: 10,
            pbs_response_preparation_duration: 20,
            bidder_response_duration_min: 50,
        };
        let now = Instant::now();
        let result = get_bidder_tmax(None, now, 500, Some(&adj));
        assert_eq!(result, 500);
    }

    #[test]
    fn test_get_bidder_tmax_with_deadline() {
        let adj = TmaxAdjustments {
            bidder_network_latency_buffer: 10,
            pbs_response_preparation_duration: 20,
            bidder_response_duration_min: 50,
        };
        let now = Instant::now();
        // Deadline 500ms from now
        let deadline = now + Duration::from_millis(500);
        let result = get_bidder_tmax(Some(deadline), now, 500, Some(&adj));
        // remaining (500) - latency (10) - prep (20) = 470
        assert_eq!(result, 470);
    }

    #[test]
    fn test_get_bidder_tmax_below_minimum_falls_back() {
        let adj = TmaxAdjustments {
            bidder_network_latency_buffer: 200,
            pbs_response_preparation_duration: 250,
            bidder_response_duration_min: 100,
        };
        let now = Instant::now();
        // Deadline 500ms from now -> remaining 500 - 200 - 250 = 50, which is < min 100
        let deadline = now + Duration::from_millis(500);
        let result = get_bidder_tmax(Some(deadline), now, 500, Some(&adj));
        // Falls back to original request tmax
        assert_eq!(result, 500);
    }

    #[test]
    fn test_get_bidder_tmax_deadline_in_past() {
        let adj = TmaxAdjustments {
            bidder_network_latency_buffer: 10,
            pbs_response_preparation_duration: 20,
            bidder_response_duration_min: 50,
        };
        let now = Instant::now();
        // Deadline is at `now`, not in the future when measured from `now`
        // Since dl > start would be false (dl == start), falls back
        let result = get_bidder_tmax(Some(now), now, 500, Some(&adj));
        assert_eq!(result, 500);
    }

    #[test]
    fn test_get_bidder_tmax_exactly_at_minimum() {
        let adj = TmaxAdjustments {
            bidder_network_latency_buffer: 50,
            pbs_response_preparation_duration: 50,
            bidder_response_duration_min: 100,
        };
        let now = Instant::now();
        // Deadline 200ms from now -> remaining 200 - 50 - 50 = 100, which == min 100
        let deadline = now + Duration::from_millis(200);
        let result = get_bidder_tmax(Some(deadline), now, 300, Some(&adj));
        assert_eq!(result, 100);
    }
}
