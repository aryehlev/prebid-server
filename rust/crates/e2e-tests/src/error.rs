//! Crate-wide error type.

use thiserror::Error;

/// Errors produced by scenario loading, runner execution, and the test suite.
#[derive(Debug, Error)]
pub enum E2eError {
    /// A YAML file failed to deserialize into a [`crate::Scenario`].
    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// A filesystem I/O error occurred while loading a scenario or suite.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The [`crate::AuctionBackend`] returned an error while running the
    /// auction.
    #[error("auction backend error: {0}")]
    Backend(String),

    /// An error occurred while diffing the actual vs expected responses.
    #[error("diff error: {0}")]
    Diff(String),

    /// An error occurred while priming the in-memory setup (stored
    /// requests/imps/accounts/etc.).
    #[error("setup error: {0}")]
    Setup(String),
}
