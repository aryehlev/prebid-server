//! Error types for the experimental simd-json parser.

use thiserror::Error;

/// Errors emitted by the experimental simd-json parser.
#[derive(Debug, Error)]
pub enum ParseError {
    /// Underlying simd-json failure (malformed JSON, type issues, etc.).
    #[error("simd-json parse error: {0}")]
    SimdJson(#[from] simd_json::Error),

    /// The top-level JSON value was not an object.
    #[error("expected top-level JSON object")]
    NotAnObject,
}
