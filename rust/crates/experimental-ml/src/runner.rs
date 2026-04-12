//! High-level pipeline that ties feature extraction and the individual
//! models together into a single [`PredictionPipeline`].

use std::collections::HashMap;

use serde_json::Value;

use crate::features::{BasicFeatureExtractor, FeatureExtractor, FEATURE_DIM};
use crate::models::bid_shader::BidShader;
use crate::models::floor_predictor::FloorPredictor;
use crate::models::timeout_predictor::TimeoutPredictor;

/// The result of running the [`PredictionPipeline`] against a request.
#[derive(Debug, Clone)]
pub struct Prediction {
    /// Predicted floor price (USD CPM).
    pub floor: f64,
    /// Suggested first-price shade factor in `[0.1, 1.0]`.
    pub shade_factor: f64,
    /// Suggested per-bidder timeouts in milliseconds.
    pub timeouts: HashMap<String, u64>,
}

/// End-to-end ML prediction pipeline: feature extraction + floor predictor
/// + bid shader + timeout predictor.
pub struct PredictionPipeline {
    /// Feature extractor used to build inputs for both the floor predictor
    /// and the bid shader.
    pub extractor: Box<dyn FeatureExtractor>,
    /// Floor price MLP.
    pub floor: FloorPredictor,
    /// First-price bid shader.
    pub shader: BidShader,
    /// Per-bidder timeout predictor.
    pub timeouts: TimeoutPredictor,
}

impl PredictionPipeline {
    /// Creates a new pipeline with default feature extractor and freshly
    /// initialised models sized to match [`FEATURE_DIM`].
    pub fn new_default() -> Self {
        Self {
            extractor: Box::new(BasicFeatureExtractor::new()),
            floor: FloorPredictor::new(FEATURE_DIM, 32),
            shader: BidShader::new(FEATURE_DIM),
            timeouts: TimeoutPredictor::new(),
        }
    }

    /// Construct a pipeline from explicit components.
    pub fn new(
        extractor: Box<dyn FeatureExtractor>,
        floor: FloorPredictor,
        shader: BidShader,
        timeouts: TimeoutPredictor,
    ) -> Self {
        Self {
            extractor,
            floor,
            shader,
            timeouts,
        }
    }

    /// Runs the full pipeline against a raw OpenRTB request.
    pub fn run(&self, request: &Value) -> Prediction {
        let features = self.extractor.extract(request);
        let floor = self.floor.predict(&features);
        let shade_factor = self.shader.shade_factor(&features);

        // Suggest a timeout for every bidder mentioned in
        // `imp[*].ext.prebid.bidder`.
        let mut timeouts = HashMap::new();
        if let Some(imps) = request.get("imp").and_then(|v| v.as_array()) {
            for imp in imps {
                if let Some(bidders) = imp
                    .get("ext")
                    .and_then(|e| e.get("prebid"))
                    .and_then(|p| p.get("bidder"))
                    .and_then(|b| b.as_object())
                {
                    for name in bidders.keys() {
                        timeouts
                            .entry(name.clone())
                            .or_insert_with(|| self.timeouts.suggest_timeout(name));
                    }
                }
            }
        }

        Prediction {
            floor,
            shade_factor,
            timeouts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::bid_shader::{SHADE_MAX, SHADE_MIN};
    use serde_json::json;

    fn dummy_request() -> Value {
        json!({
            "id": "req-1",
            "site": {"id": "siteA", "domain": "example.com"},
            "device": {
                "devicetype": 2,
                "geo": {"country": "USA"}
            },
            "imp": [
                {
                    "id": "imp-1",
                    "banner": {"w": 300, "h": 250},
                    "ext": {"prebid": {"bidder": {"appnexus": {}, "rubicon": {}}}}
                },
                {
                    "id": "imp-2",
                    "video": {"w": 640, "h": 480},
                    "ext": {"prebid": {"bidder": {"appnexus": {}}}}
                }
            ]
        })
    }

    #[test]
    fn end_to_end_produces_finite_values() {
        let pipeline = PredictionPipeline::new_default();
        let pred = pipeline.run(&dummy_request());

        assert!(pred.floor.is_finite());
        assert!(pred.floor >= 0.0);

        assert!((SHADE_MIN..=SHADE_MAX).contains(&pred.shade_factor));

        // Both impressions mention appnexus; one mentions rubicon.
        assert!(pred.timeouts.contains_key("appnexus"));
        assert!(pred.timeouts.contains_key("rubicon"));
        assert_eq!(pred.timeouts.len(), 2);
    }

    #[test]
    fn pipeline_works_with_empty_request() {
        let pipeline = PredictionPipeline::new_default();
        let pred = pipeline.run(&json!({}));
        assert!(pred.floor.is_finite());
        assert!(pred.timeouts.is_empty());
    }
}
