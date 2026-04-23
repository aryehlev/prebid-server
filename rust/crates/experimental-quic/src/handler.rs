//! Auction handler trait and a stub echo implementation.

use async_trait::async_trait;
use bytes::Bytes;
use thiserror::Error;

/// Error type returned by [`AuctionHandler::handle`].
#[derive(Debug, Error)]
pub enum HandlerError {
    /// The request body was malformed.
    #[error("bad request: {0}")]
    BadRequest(String),

    /// An internal error occurred while processing the request.
    #[error("internal error: {0}")]
    Internal(String),
}

/// Trait implemented by auction request handlers.
///
/// This trait is intentionally minimal – it takes the raw request bytes and
/// returns the raw response bytes. The experimental HTTP/3 server only cares
/// about the request/response body; header handling is left to future work.
#[async_trait]
pub trait AuctionHandler: Send + Sync + 'static {
    /// Handle a POST `/openrtb2/auction` request body.
    async fn handle(&self, request_bytes: Bytes) -> Result<Bytes, HandlerError>;
}

/// A trivial [`AuctionHandler`] that echoes its input back.
///
/// Useful for integration tests and for sanity checking a running server
/// without needing the full exchange pipeline available.
#[derive(Debug, Default, Clone)]
pub struct StubAuctionHandler;

impl StubAuctionHandler {
    /// Construct a new stub handler.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AuctionHandler for StubAuctionHandler {
    async fn handle(&self, request_bytes: Bytes) -> Result<Bytes, HandlerError> {
        Ok(request_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_handler_echoes_payload() {
        let handler = StubAuctionHandler::new();
        let payload = Bytes::from_static(b"{\"id\":\"req-1\"}");
        let response = handler.handle(payload.clone()).await.expect("handler ok");
        assert_eq!(response, payload);
    }

    #[tokio::test]
    async fn stub_handler_echoes_empty_payload() {
        let handler = StubAuctionHandler::new();
        let response = handler.handle(Bytes::new()).await.expect("handler ok");
        assert!(response.is_empty());
    }
}
