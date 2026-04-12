//! Linear bid shader for first-price auctions.
//!
//! A [`BidShader`] applies a single dense layer to a feature vector and
//! squashes the output with a sigmoid to produce a shade factor in
//! `[SHADE_MIN, SHADE_MAX]`, then multiplies that factor by the incoming
//! first-price bid.

use ndarray::Array1;

use super::LcgRng;
use crate::features::FeatureVector;

/// Minimum shade factor. We never shave more than 90% off a bid.
pub const SHADE_MIN: f64 = 0.1;
/// Maximum shade factor. A factor of 1.0 means no shading at all.
pub const SHADE_MAX: f64 = 1.0;

/// Linear bid shader: one dense layer + sigmoid squash.
#[derive(Debug, Clone)]
pub struct BidShader {
    weights: Array1<f32>,
    bias: f32,
    input_dim: usize,
}

impl BidShader {
    /// Creates a new randomly-initialised bid shader with the given input
    /// dimension.
    pub fn new(input_dim: usize) -> Self {
        let mut rng = LcgRng::new(0xBEEFu64 ^ input_dim as u64);
        let scale = 1.0 / (input_dim as f32).sqrt();
        let weights = Array1::from_shape_fn(input_dim, |_| rng.next_f32_centered() * scale);
        // Bias of zero gives an initial shade factor of roughly 0.55, which
        // is a reasonable neutral starting point for first-price shading.
        Self {
            weights,
            bias: 0.2,
            input_dim,
        }
    }

    /// Creates a bid shader with explicit weights. Primarily useful for
    /// loading a pre-trained model or writing deterministic tests.
    pub fn with_params(weights: Vec<f32>, bias: f32) -> Self {
        let input_dim = weights.len();
        Self {
            weights: Array1::from(weights),
            bias,
            input_dim,
        }
    }

    /// Input dimension the shader was configured for.
    #[inline]
    pub fn input_dim(&self) -> usize {
        self.input_dim
    }

    /// Computes the shade factor in `[SHADE_MIN, SHADE_MAX]` for a given
    /// feature vector.
    pub fn shade_factor(&self, features: &FeatureVector) -> f64 {
        let mut x = Array1::<f32>::zeros(self.input_dim);
        for (i, v) in features.as_slice().iter().take(self.input_dim).enumerate() {
            x[i] = *v;
        }
        let z: f32 = self.weights.dot(&x) + self.bias;
        let sig = 1.0 / (1.0 + (-z as f64).exp());
        // Map [0, 1] sigmoid into [SHADE_MIN, SHADE_MAX].
        let factor = SHADE_MIN + sig * (SHADE_MAX - SHADE_MIN);
        factor.clamp(SHADE_MIN, SHADE_MAX)
    }

    /// Returns a suggested shaded bid given the auction features and the
    /// original first-price bid.
    pub fn suggested_bid(&self, features: &FeatureVector, first_price_bid: f64) -> f64 {
        let factor = self.shade_factor(features);
        (first_price_bid * factor).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shade_factor_is_in_bounds() {
        let shader = BidShader::new(16);
        let fv = FeatureVector(vec![0.3; 16]);
        let f = shader.shade_factor(&fv);
        assert!((SHADE_MIN..=SHADE_MAX).contains(&f));
    }

    #[test]
    fn suggested_bid_is_bid_times_factor() {
        let shader = BidShader::new(8);
        let fv = FeatureVector(vec![0.1; 8]);
        let bid = 2.50;
        let shaded = shader.suggested_bid(&fv, bid);
        let factor = shader.shade_factor(&fv);
        assert!((shaded - bid * factor).abs() < 1e-9);
        assert!(shaded <= bid);
        assert!(shaded >= bid * SHADE_MIN);
    }

    #[test]
    fn with_params_round_trip() {
        let shader = BidShader::with_params(vec![0.0; 4], 10.0);
        let fv = FeatureVector(vec![0.0; 4]);
        // Huge positive bias -> sigmoid ~1 -> factor near SHADE_MAX.
        let f = shader.shade_factor(&fv);
        assert!(f > 0.9);
    }
}
