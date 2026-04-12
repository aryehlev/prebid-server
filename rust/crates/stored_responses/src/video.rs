use crate::types::ImpBidderStoredResp;

/// Placeholder for future video-specific stored-response flushing logic.
///
/// The Go port flushes matched stored video creatives into the final
/// `BidResponse`. Once the exchange pipeline is in place this will walk the
/// stored bid responses and splice them into the appropriate seatbids.
pub fn flush_stored_video_responses(_stored: &ImpBidderStoredResp) {
    // TODO: port flushStoredVideoResponses from Go once the exchange
    // pipeline is available in Rust.
}
