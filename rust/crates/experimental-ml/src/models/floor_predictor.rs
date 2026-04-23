//! Neural-network based floor price predictor.
//!
//! A small 3-layer multilayer perceptron (input -> hidden -> output) with
//! ReLU activations on the hidden layer and a sigmoid on the output. The
//! output is scaled into a reasonable floor-price range.

use ndarray::{Array1, Array2};

use super::LcgRng;
use crate::features::FeatureVector;

/// Maximum floor price (USD CPM) the predictor will ever produce.
const MAX_FLOOR: f64 = 10.0;

/// A tiny MLP that produces a predicted floor price from a feature vector.
#[derive(Debug, Clone)]
pub struct FloorPredictor {
    /// `[hidden_dim, input_dim]`
    w1: Array2<f32>,
    /// `[hidden_dim]`
    b1: Array1<f32>,
    /// `[1, hidden_dim]`
    w2: Array2<f32>,
    /// scalar bias for the output layer
    b2: f32,
    input_dim: usize,
    hidden_dim: usize,
}

impl FloorPredictor {
    /// Creates a new [`FloorPredictor`] with randomly initialised weights.
    ///
    /// Weights are scaled by `1 / sqrt(fan_in)` (Xavier-ish) to keep the
    /// forward pass numerically well-behaved without any explicit
    /// normalisation layers.
    pub fn new(input_dim: usize, hidden_dim: usize) -> Self {
        let mut rng = LcgRng::new(0xC0FFEE_u64 ^ (input_dim as u64) ^ ((hidden_dim as u64) << 32));

        let scale1 = 1.0 / (input_dim as f32).sqrt();
        let w1 = Array2::from_shape_fn((hidden_dim, input_dim), |_| {
            rng.next_f32_centered() * scale1
        });
        let b1 = Array1::zeros(hidden_dim);

        let scale2 = 1.0 / (hidden_dim as f32).sqrt();
        let w2 = Array2::from_shape_fn((1, hidden_dim), |_| rng.next_f32_centered() * scale2);
        let b2 = 0.0f32;

        Self {
            w1,
            b1,
            w2,
            b2,
            input_dim,
            hidden_dim,
        }
    }

    /// Input dimension the predictor was configured for.
    #[inline]
    pub fn input_dim(&self) -> usize {
        self.input_dim
    }

    /// Hidden dimension the predictor was configured for.
    #[inline]
    pub fn hidden_dim(&self) -> usize {
        self.hidden_dim
    }

    /// Forward pass. Returns a predicted floor price clamped to
    /// `[0.0, MAX_FLOOR]`.
    ///
    /// If the feature vector has the wrong dimension, the input is either
    /// truncated or zero-padded so the call never panics — this keeps the
    /// predictor robust to feature-pipeline changes during experimentation.
    pub fn predict(&self, features: &FeatureVector) -> f64 {
        let mut x = Array1::<f32>::zeros(self.input_dim);
        for (i, v) in features.as_slice().iter().take(self.input_dim).enumerate() {
            x[i] = *v;
        }

        // hidden = ReLU(W1 * x + b1)
        let mut hidden = self.w1.dot(&x) + &self.b1;
        for v in hidden.iter_mut() {
            if *v < 0.0 {
                *v = 0.0;
            }
        }

        // output = sigmoid(W2 * hidden + b2)
        let z: f32 = self.w2.dot(&hidden)[0] + self.b2;
        let sigmoid = 1.0 / (1.0 + (-z as f64).exp());

        // Scale into the floor range.
        (sigmoid * MAX_FLOOR).clamp(0.0, MAX_FLOOR)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_pass_is_finite_and_in_range() {
        let model = FloorPredictor::new(32, 16);
        let fv = FeatureVector(vec![0.1; 32]);
        let y = model.predict(&fv);
        assert!(y.is_finite());
        assert!((0.0..=MAX_FLOOR).contains(&y));
    }

    #[test]
    fn handles_mismatched_feature_vector() {
        let model = FloorPredictor::new(10, 4);
        // Too short: should be zero-padded internally.
        let short = FeatureVector(vec![1.0; 3]);
        let y1 = model.predict(&short);
        assert!(y1.is_finite());
        // Too long: extra values should be ignored.
        let long = FeatureVector(vec![1.0; 50]);
        let y2 = model.predict(&long);
        assert!(y2.is_finite());
    }

    #[test]
    fn deterministic_init_for_same_dims() {
        let a = FloorPredictor::new(8, 4);
        let b = FloorPredictor::new(8, 4);
        let fv = FeatureVector(vec![0.5; 8]);
        assert_eq!(a.predict(&fv), b.predict(&fv));
    }
}
