//! Stage identifiers.
//!
//! Corresponds to the `Stage` type declared in `hooks/plan.go`.

use serde::{Deserialize, Serialize};
use std::fmt;

/// The set of well-known stages at which hooks can be invoked during
/// request/response processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Invoked at the very beginning of request processing before the
    /// body has been parsed.
    EntrypointStage,
    /// Invoked with the raw (unparsed) auction request.
    RawAuctionStage,
    /// Invoked with the parsed/processed auction request.
    ProcessedAuctionStage,
    /// Invoked once per bidder, with the bidder-specific request.
    BidderRequestStage,
    /// Invoked with the raw bidder response.
    RawBidderResponseStage,
    /// Invoked with the collection of all processed bid responses.
    AllProcessedBidResponsesStage,
    /// Invoked just before the final auction response is returned.
    AuctionResponseStage,
}

impl Stage {
    /// The textual name of this stage, matching the Go constants.
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::EntrypointStage => "entrypoint",
            Stage::RawAuctionStage => "raw_auction_request",
            Stage::ProcessedAuctionStage => "processed_auction_request",
            Stage::BidderRequestStage => "bidder_request",
            Stage::RawBidderResponseStage => "raw_bidder_response",
            Stage::AllProcessedBidResponsesStage => "all_processed_bid_responses",
            Stage::AuctionResponseStage => "auction_response",
        }
    }

    /// Whether a hook at this stage is allowed to reject the request.
    ///
    /// Mirrors `Stage.IsRejectable` in the Go implementation.
    pub fn is_rejectable(self) -> bool {
        !matches!(
            self,
            Stage::AllProcessedBidResponsesStage | Stage::AuctionResponseStage
        )
    }
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejectable_stages() {
        assert!(Stage::EntrypointStage.is_rejectable());
        assert!(Stage::RawAuctionStage.is_rejectable());
        assert!(!Stage::AuctionResponseStage.is_rejectable());
        assert!(!Stage::AllProcessedBidResponsesStage.is_rejectable());
    }

    #[test]
    fn stage_names() {
        assert_eq!(Stage::EntrypointStage.as_str(), "entrypoint");
        assert_eq!(Stage::BidderRequestStage.as_str(), "bidder_request");
    }
}
