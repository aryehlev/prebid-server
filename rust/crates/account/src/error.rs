use thiserror::Error;

/// Errors returned when resolving an account configuration.
#[derive(Debug, Error)]
pub enum AccountError {
    #[error("account not found: {0}")]
    NotFound(String),

    #[error("account is disabled: {0}")]
    Disabled(String),

    #[error("malformed account config for id `{id}`: {message}")]
    Malformed { id: String, message: String },

    #[error("account backend error: {0}")]
    Backend(String),
}
