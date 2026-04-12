use thiserror::Error;

/// Errors produced by the stored responses module.
#[derive(Debug, Error)]
pub enum StoredRespError {
    #[error("stored response id not found: {0}")]
    NotFound(String),

    #[error("malformed stored response request: {0}")]
    Malformed(String),

    #[error("stored response backend error: {0}")]
    Backend(String),

    #[error("invalid imp[{index}]: {message}")]
    InvalidImp { index: usize, message: String },
}
