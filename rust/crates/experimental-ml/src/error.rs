//! Error types for the experimental ML crate.

use thiserror::Error;

/// Errors produced by the experimental ML crate.
#[derive(Debug, Error)]
pub enum MlError {
    /// A feature vector had a length that did not match the expected input
    /// dimension of a model.
    #[error("feature dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch {
        /// Number of features the model was configured for.
        expected: usize,
        /// Number of features actually supplied.
        got: usize,
    },

    /// A numerical computation produced a non-finite value (NaN or infinity).
    #[error("numerical error: {0}")]
    Numerical(String),

    /// A required field was missing from the input JSON request.
    #[error("missing field: {0}")]
    MissingField(String),

    /// Catch-all for any other error.
    #[error("ml error: {0}")]
    Other(String),
}
