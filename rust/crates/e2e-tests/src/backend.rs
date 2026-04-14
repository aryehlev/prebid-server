//! Pluggable auction backend used by [`crate::ScenarioRunner`].

use async_trait::async_trait;
use serde_json::{json, Value};
use thiserror::Error;

/// Errors produced by an [`AuctionBackend`] implementation.
#[derive(Debug, Error)]
pub enum BackendError {
    /// The request could not be processed.
    #[error("invalid auction request: {0}")]
    InvalidRequest(String),

    /// A generic backend failure.
    #[error("{0}")]
    Other(String),
}

/// Pluggable auction backend that the runner invokes once per scenario.
#[async_trait]
pub trait AuctionBackend: Send + Sync {
    /// Execute the auction and return the resulting `BidResponse` JSON.
    async fn run_auction(&self, req: &Value) -> Result<Value, BackendError>;
}

/// Minimal backend useful for unit tests and CI bring-up.
///
/// If the request's first imp carries a `mock_response` field, that value is
/// returned verbatim. Otherwise an empty `BidResponse` (`{"id": <req.id>,
/// "seatbid": []}`) is produced.
#[derive(Debug, Default, Clone)]
pub struct StubAuctionBackend;

impl StubAuctionBackend {
    /// Construct a new stub backend.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AuctionBackend for StubAuctionBackend {
    async fn run_auction(&self, req: &Value) -> Result<Value, BackendError> {
        // Prefer a top-level `mock_response` on the request…
        if let Some(m) = req.get("mock_response") {
            return Ok(m.clone());
        }
        // …then fall back to the first imp's `mock_response`.
        if let Some(imps) = req.get("imp").and_then(|v| v.as_array()) {
            for imp in imps {
                if let Some(m) = imp.get("mock_response") {
                    return Ok(m.clone());
                }
            }
        }
        // Otherwise return an empty BidResponse carrying the request id.
        let id = req.get("id").cloned().unwrap_or(Value::String(String::new()));
        Ok(json!({ "id": id, "seatbid": [] }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_echoes_top_level_mock_response() {
        let req = json!({"id": "r", "mock_response": {"id": "r", "seatbid": [{"x": 1}]}});
        let out = StubAuctionBackend::new().run_auction(&req).await.unwrap();
        assert_eq!(out["seatbid"][0]["x"], 1);
    }

    #[tokio::test]
    async fn stub_returns_empty_bid_response_when_absent() {
        let req = json!({"id": "r", "imp": [{"id": "i"}]});
        let out = StubAuctionBackend::new().run_auction(&req).await.unwrap();
        assert_eq!(out["id"], "r");
        assert!(out["seatbid"].as_array().unwrap().is_empty());
    }
}
