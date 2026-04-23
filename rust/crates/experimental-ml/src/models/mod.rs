//! Individual ML models used by the experimental pipeline.
//!
//! Each submodule is self-contained and can be used independently from the
//! rest of the crate.

pub mod bid_shader;
pub mod floor_predictor;
pub mod timeout_predictor;

/// A tiny deterministic linear-congruential PRNG used for weight
/// initialisation.  Having our own PRNG avoids pulling in the `rand` crate
/// just so we can shuffle a few floats.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LcgRng {
    state: u64,
}

impl LcgRng {
    /// Numerical Recipes constants.
    const A: u64 = 6_364_136_223_846_793_005;
    const C: u64 = 1_442_695_040_888_963_407;

    pub(crate) fn new(seed: u64) -> Self {
        // Avoid a zero state which is a fixed point for some generators.
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    /// Returns a pseudo-random `u64`.
    pub(crate) fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(Self::A).wrapping_add(Self::C);
        self.state
    }

    /// Returns a pseudo-random `f32` in `[-1.0, 1.0)`.
    pub(crate) fn next_f32_centered(&mut self) -> f32 {
        // Take the top 24 bits for the mantissa.
        let bits = (self.next_u64() >> 40) as u32; // 24 bits
        let unit = bits as f32 / (1u32 << 24) as f32; // [0, 1)
        unit * 2.0 - 1.0
    }
}
