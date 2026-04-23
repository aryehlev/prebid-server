//! Crate-wide error type.

use thiserror::Error;

/// Top-level error type for the parity crate.
#[derive(Debug, Error)]
pub enum ParityError {
    /// Wrapper around `std::io::Error` (e.g. from [`crate::reporter::FileReporter`]).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Wrapper around `serde_json::Error`.
    #[error("serde_json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Wrapper around a regex compilation error.
    #[error("regex error: {0}")]
    Regex(#[from] regex::Error),

    /// Wrapper around a reqwest HTTP error.
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    /// Miscellaneous parity error with a free-form message.
    #[error("parity error: {0}")]
    Other(String),
}
