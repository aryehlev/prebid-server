//! Error types for the experimental QUIC auction endpoint.

use thiserror::Error;

/// Errors produced by [`crate::server::QuicAuctionServer`] and related helpers.
#[derive(Debug, Error)]
pub enum QuicError {
    /// I/O error binding or accepting on the UDP socket.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    /// The provided certificate or key material could not be parsed.
    #[error("invalid tls material: {0}")]
    Tls(String),

    /// rustls returned an error while building its `ServerConfig`.
    #[error("rustls error: {0}")]
    Rustls(String),

    /// A lower level `quinn` connection error.
    #[error("quinn connection error: {0}")]
    Connection(String),

    /// An HTTP/3 protocol error surfaced from `h3`.
    #[error("h3 protocol error: {0}")]
    H3(String),

    /// The handler returned an error.
    #[error("handler error: {0}")]
    Handler(String),

    /// Misc catch-all.
    #[error("{0}")]
    Other(String),
}

impl From<rustls::Error> for QuicError {
    fn from(e: rustls::Error) -> Self {
        QuicError::Rustls(e.to_string())
    }
}
