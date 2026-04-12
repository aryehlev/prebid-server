//! Error types for the fuzz crate.

use thiserror::Error;

/// Errors that can be produced by fuzz generators, mutators, and oracles.
#[derive(Debug, Error)]
pub enum FuzzError {
    /// A JSON-path lookup failed to locate the requested node.
    #[error("path not found: {0}")]
    PathNotFound(String),

    /// A type-mismatch was encountered while applying a mutation.
    #[error("type mismatch at {path}: expected {expected}, got {got}")]
    TypeMismatch {
        path: String,
        expected: &'static str,
        got: &'static str,
    },

    /// The generated value was structurally invalid.
    #[error("invalid generated value: {0}")]
    InvalidValue(String),

    /// Wrapped serde_json error.
    #[error("serde_json error: {0}")]
    SerdeJson(#[from] serde_json::Error),
}
