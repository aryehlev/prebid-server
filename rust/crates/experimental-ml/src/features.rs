//! Feature extraction for ML inputs.
//!
//! The [`FeatureExtractor`] trait turns a raw OpenRTB bid request (represented
//! as a [`serde_json::Value`]) into a dense numeric [`FeatureVector`].
//!
//! [`BasicFeatureExtractor`] is a reference implementation that extracts a
//! small, fixed-size vector suitable for quick experimentation.

use chrono::{Datelike, Timelike, Utc};
use serde_json::Value;

/// Dimension contributed by the hour-of-day feature.
const HOUR_DIM: usize = 1;
/// Dimension contributed by the day-of-week feature.
const DOW_DIM: usize = 1;
/// Dimension contributed by the one-hot device type feature.
const DEVICE_DIM: usize = 7;
/// Dimension contributed by site_flag + app_flag.
const CHANNEL_DIM: usize = 2;
/// Dimension contributed by the country hash-bucket one-hot.
const COUNTRY_BUCKETS: usize = 16;
/// Dimension contributed by the num_imps feature.
const NUM_IMPS_DIM: usize = 1;
/// Dimension contributed by has_video + has_banner + has_native.
const MEDIA_DIM: usize = 3;
/// Dimension contributed by bidder_count.
const BIDDER_COUNT_DIM: usize = 1;

/// Total feature dimension used by [`BasicFeatureExtractor`].
pub const FEATURE_DIM: usize = HOUR_DIM
    + DOW_DIM
    + DEVICE_DIM
    + CHANNEL_DIM
    + COUNTRY_BUCKETS
    + NUM_IMPS_DIM
    + MEDIA_DIM
    + BIDDER_COUNT_DIM;

/// A dense numeric feature vector used as input to all ML models in this
/// crate.
#[derive(Debug, Clone, PartialEq)]
pub struct FeatureVector(pub Vec<f32>);

impl FeatureVector {
    /// Returns the number of features (same as [`FeatureVector::dim`]).
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the feature vector is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the dimension of the feature vector.
    #[inline]
    pub fn dim(&self) -> usize {
        self.0.len()
    }

    /// Returns a slice view over the underlying data.
    #[inline]
    pub fn as_slice(&self) -> &[f32] {
        &self.0
    }
}

/// Trait for components that turn a raw request into a feature vector.
pub trait FeatureExtractor: Send + Sync {
    /// Extract a feature vector from a raw OpenRTB-shaped request.
    fn extract(&self, request: &Value) -> FeatureVector;
}

/// Reference feature extractor that produces a fixed [`FEATURE_DIM`]-sized
/// vector from an OpenRTB-shaped request.
#[derive(Debug, Default, Clone, Copy)]
pub struct BasicFeatureExtractor;

impl BasicFeatureExtractor {
    /// Creates a new [`BasicFeatureExtractor`].
    pub const fn new() -> Self {
        Self
    }
}

impl FeatureExtractor for BasicFeatureExtractor {
    fn extract(&self, request: &Value) -> FeatureVector {
        let mut out = Vec::with_capacity(FEATURE_DIM);

        // Hour of day (0..=23) normalised into [0, 1].
        let now = Utc::now();
        out.push(now.hour() as f32 / 23.0);

        // Day of week as 0..=6 normalised into [0, 1].
        // chrono::Weekday::num_days_from_monday returns 0..=6.
        out.push(now.weekday().num_days_from_monday() as f32 / 6.0);

        // Device type: 7-dim one-hot. OpenRTB device types 1..=7 map to
        // indices 0..=6; anything else collapses to "Unknown" (index 0).
        let device_type = request
            .get("device")
            .and_then(|d| d.get("devicetype"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let mut device_oh = [0.0f32; DEVICE_DIM];
        if (1..=DEVICE_DIM as i64).contains(&device_type) {
            device_oh[(device_type - 1) as usize] = 1.0;
        } else {
            device_oh[0] = 1.0;
        }
        out.extend_from_slice(&device_oh);

        // Site / app flags.
        let site_flag = if request.get("site").is_some() { 1.0 } else { 0.0 };
        let app_flag = if request.get("app").is_some() { 1.0 } else { 0.0 };
        out.push(site_flag);
        out.push(app_flag);

        // Country: 16-dim hash-bucket one-hot.
        let country = request
            .get("device")
            .and_then(|d| d.get("geo"))
            .and_then(|g| g.get("country"))
            .and_then(|v| v.as_str())
            .or_else(|| {
                request
                    .get("user")
                    .and_then(|u| u.get("geo"))
                    .and_then(|g| g.get("country"))
                    .and_then(|v| v.as_str())
            })
            .unwrap_or("");
        let mut country_oh = [0.0f32; COUNTRY_BUCKETS];
        let bucket = simple_hash(country) as usize % COUNTRY_BUCKETS;
        country_oh[bucket] = 1.0;
        out.extend_from_slice(&country_oh);

        // Number of impressions (log-scaled).
        let imps = request
            .get("imp")
            .and_then(|v| v.as_array())
            .map(|a| a.as_slice())
            .unwrap_or(&[]);
        let num_imps = imps.len() as f32;
        out.push((1.0 + num_imps).ln());

        // Media type flags across all impressions.
        let mut has_video = 0.0;
        let mut has_banner = 0.0;
        let mut has_native = 0.0;
        for imp in imps {
            if imp.get("video").is_some() {
                has_video = 1.0;
            }
            if imp.get("banner").is_some() {
                has_banner = 1.0;
            }
            if imp.get("native").is_some() {
                has_native = 1.0;
            }
        }
        out.push(has_video);
        out.push(has_banner);
        out.push(has_native);

        // Bidder count: number of distinct keys under imp[*].ext.prebid.bidder.
        let mut bidder_count: usize = 0;
        for imp in imps {
            if let Some(bidders) = imp
                .get("ext")
                .and_then(|e| e.get("prebid"))
                .and_then(|p| p.get("bidder"))
                .and_then(|b| b.as_object())
            {
                bidder_count = bidder_count.max(bidders.len());
            }
        }
        out.push(bidder_count as f32);

        debug_assert_eq!(out.len(), FEATURE_DIM);
        FeatureVector(out)
    }
}

/// A tiny FNV-like hash used for bucketing countries without pulling in a
/// dedicated hashing crate.
fn simple_hash(s: &str) -> u32 {
    let mut hash: u32 = 2166136261;
    for b in s.as_bytes() {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn feature_dim_constant_matches_components() {
        assert_eq!(FEATURE_DIM, 1 + 1 + 7 + 2 + 16 + 1 + 3 + 1);
    }

    #[test]
    fn basic_extractor_produces_correct_dim() {
        let extractor = BasicFeatureExtractor::new();
        let req = json!({
            "site": {"id": "s1"},
            "device": {"devicetype": 2, "geo": {"country": "USA"}},
            "imp": [
                {"id": "1", "banner": {}, "ext": {"prebid": {"bidder": {"a": {}, "b": {}}}}}
            ]
        });
        let fv = extractor.extract(&req);
        assert_eq!(fv.dim(), FEATURE_DIM);
        assert_eq!(fv.len(), FEATURE_DIM);
    }

    #[test]
    fn empty_request_still_yields_correct_dim() {
        let extractor = BasicFeatureExtractor::new();
        let fv = extractor.extract(&json!({}));
        assert_eq!(fv.dim(), FEATURE_DIM);
    }
}
