//! Seat non-bids tracking for the auction.
//!
//! Tracks bids that were rejected or filtered during the auction process,
//! mirroring the Go implementation in `exchange/seat_non_bids.go` and
//! `exchange/non_bid_reason.go`.

use std::collections::HashMap;

use openrtb_ext::{NonBid, NonBidExt, NonBidObject, ExtResponseNonBidPrebid, SeatNonBid};
use pbs_adapters::TypedBid;

// ---------------------------------------------------------------------------
// NonBidReason codes
// ---------------------------------------------------------------------------

/// Reasons why a bid was not resulted in a positive bid.
///
/// Reference: <https://github.com/InteractiveAdvertisingBureau/openrtb/blob/master/extensions/community_extensions/seat-non-bid.md#list-non-bid-status-codes>
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum NonBidReason {
    /// Error - General
    ErrorGeneral = 100,
    /// Error - Timeout
    ErrorTimeout = 101,
    /// Error - Bidder Unreachable
    ErrorBidderUnreachable = 103,
    /// Response Rejected - General
    ResponseRejectedGeneral = 300,
    /// Response Rejected - Below Floor
    ResponseRejectedBelowFloor = 301,
    /// Response Rejected - Category Mapping Invalid
    ResponseRejectedCategoryMappingInvalid = 303,
    /// Response Rejected - Bid was Below Deal Floor
    ResponseRejectedBelowDealFloor = 304,
    /// Response Rejected - Invalid Creative (Size Not Allowed)
    ResponseRejectedCreativeSizeNotAllowed = 351,
    /// Response Rejected - Invalid Creative (Not Secure)
    ResponseRejectedCreativeNotSecure = 352,
}

impl NonBidReason {
    /// Returns the integer status code for this reason.
    pub fn code(self) -> i32 {
        self as i32
    }
}

impl From<NonBidReason> for i32 {
    fn from(reason: NonBidReason) -> Self {
        reason as i32
    }
}

/// Determine the non-bid reason from an error.
///
/// Currently maps timeout errors; all other errors are treated as general errors.
pub fn error_to_non_bid_reason(is_timeout: bool) -> NonBidReason {
    if is_timeout {
        NonBidReason::ErrorTimeout
    } else {
        NonBidReason::ErrorGeneral
    }
}

// ---------------------------------------------------------------------------
// SeatNonBidBuilder
// ---------------------------------------------------------------------------

/// Builder that accumulates non-bids across the auction keyed by seat (bidder name).
///
/// This mirrors Go's `SeatNonBidBuilder` which is `map[string][]openrtb_ext.NonBid`.
#[derive(Debug, Clone, Default)]
pub struct SeatNonBidBuilder {
    inner: HashMap<String, Vec<NonBid>>,
}

impl SeatNonBidBuilder {
    /// Creates an empty builder.
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    /// Adds a rejected bid to the builder, extracting fields from the typed bid
    /// to populate the NonBid ext.
    pub fn reject_bid(&mut self, typed_bid: &TypedBid, reason: NonBidReason, seat: &str) {
        let bid = &typed_bid.bid;
        let non_bid = NonBid {
            impid: bid.impid.clone(),
            statuscode: reason.code(),
            ext: Some(NonBidExt {
                prebid: ExtResponseNonBidPrebid {
                    bid: NonBidObject {
                        price: Some(bid.price),
                        adomain: bid.adomain.clone(),
                        cat: bid.cat.clone(),
                        dealid: bid.dealid.clone(),
                        w: bid.w.map(|v| v as i64),
                        h: bid.h.map(|v| v as i64),
                        dur: bid.dur.map(|v| v as i64),
                        orig_bid_cpm: Some(typed_bid.orig_bid_cpm),
                        orig_bid_cur: Some(typed_bid.orig_bid_cur.clone()),
                    },
                },
            }),
        };
        self.inner.entry(seat.to_string()).or_default().push(non_bid);
    }

    /// Adds a non-bid entry for each impression ID with a given reason (no bid ext).
    pub fn reject_imps(&mut self, imp_ids: &[String], reason: NonBidReason, seat: &str) {
        if imp_ids.is_empty() {
            return;
        }
        let non_bids: Vec<NonBid> = imp_ids
            .iter()
            .map(|imp_id| NonBid {
                impid: imp_id.clone(),
                statuscode: reason.code(),
                ext: None,
            })
            .collect();
        self.inner.entry(seat.to_string()).or_default().extend(non_bids);
    }

    /// Merges another builder into this one.
    pub fn append(&mut self, other: &SeatNonBidBuilder) {
        for (seat, non_bids) in &other.inner {
            self.inner
                .entry(seat.clone())
                .or_default()
                .extend(non_bids.iter().cloned());
        }
    }

    /// Converts the accumulated non-bids into a `Vec<SeatNonBid>` suitable
    /// for the auction response.
    pub fn to_seat_non_bids(&self) -> Vec<SeatNonBid> {
        self.inner
            .iter()
            .map(|(seat, non_bids)| SeatNonBid {
                seat: seat.clone(),
                nonbid: non_bids.clone(),
                ext: None,
            })
            .collect()
    }

    /// Returns `true` if there are no accumulated non-bids.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns the total number of non-bid entries across all seats.
    pub fn len(&self) -> usize {
        self.inner.values().map(|v| v.len()).sum()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use openrtb::Bid;
    use openrtb_ext::BidType;
    use pbs_adapters::TypedBid;

    fn make_typed_bid(imp_id: &str, price: f64) -> TypedBid {
        let bid = Bid {
            id: "bid-1".to_string(),
            impid: imp_id.to_string(),
            price,
            adomain: Some(vec!["example.com".to_string()]),
            dealid: Some("deal-1".to_string()),
            w: Some(300),
            h: Some(250),
            ..Default::default()
        };
        TypedBid {
            bid,
            bid_type: BidType::Banner,
            bid_meta: None,
            bid_video: None,
            deal_priority: 0,
            orig_bid_cpm: price,
            orig_bid_cur: "USD".to_string(),
            orig_bid_cpm_usd: price,
        }
    }

    #[test]
    fn test_non_bid_reason_codes() {
        assert_eq!(NonBidReason::ErrorGeneral.code(), 100);
        assert_eq!(NonBidReason::ErrorTimeout.code(), 101);
        assert_eq!(NonBidReason::ErrorBidderUnreachable.code(), 103);
        assert_eq!(NonBidReason::ResponseRejectedBelowFloor.code(), 301);
        assert_eq!(NonBidReason::ResponseRejectedCreativeSizeNotAllowed.code(), 351);
    }

    #[test]
    fn test_error_to_non_bid_reason() {
        assert_eq!(error_to_non_bid_reason(true), NonBidReason::ErrorTimeout);
        assert_eq!(error_to_non_bid_reason(false), NonBidReason::ErrorGeneral);
    }

    #[test]
    fn test_reject_bid() {
        let mut builder = SeatNonBidBuilder::new();
        let typed_bid = make_typed_bid("imp-1", 1.5);
        builder.reject_bid(&typed_bid, NonBidReason::ResponseRejectedBelowFloor, "appnexus");

        let result = builder.to_seat_non_bids();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].seat, "appnexus");
        assert_eq!(result[0].nonbid.len(), 1);
        assert_eq!(result[0].nonbid[0].impid, "imp-1");
        assert_eq!(result[0].nonbid[0].statuscode, 301);

        let ext = result[0].nonbid[0].ext.as_ref().unwrap();
        assert_eq!(ext.prebid.bid.price, Some(1.5));
        assert_eq!(ext.prebid.bid.w, Some(300));
        assert_eq!(ext.prebid.bid.h, Some(250));
        assert_eq!(ext.prebid.bid.dealid.as_deref(), Some("deal-1"));
        assert_eq!(ext.prebid.bid.orig_bid_cpm, Some(1.5));
        assert_eq!(ext.prebid.bid.orig_bid_cur.as_deref(), Some("USD"));
    }

    #[test]
    fn test_reject_imps() {
        let mut builder = SeatNonBidBuilder::new();
        let imp_ids = vec!["imp-1".to_string(), "imp-2".to_string()];
        builder.reject_imps(&imp_ids, NonBidReason::ErrorTimeout, "rubicon");

        let result = builder.to_seat_non_bids();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].nonbid.len(), 2);
        assert_eq!(result[0].nonbid[0].statuscode, 101);
        assert_eq!(result[0].nonbid[1].statuscode, 101);
        assert!(result[0].nonbid[0].ext.is_none());
    }

    #[test]
    fn test_reject_imps_empty() {
        let mut builder = SeatNonBidBuilder::new();
        builder.reject_imps(&[], NonBidReason::ErrorGeneral, "rubicon");
        assert!(builder.is_empty());
    }

    #[test]
    fn test_append() {
        let mut builder1 = SeatNonBidBuilder::new();
        builder1.reject_imps(
            &["imp-1".to_string()],
            NonBidReason::ErrorGeneral,
            "appnexus",
        );

        let mut builder2 = SeatNonBidBuilder::new();
        builder2.reject_imps(
            &["imp-2".to_string()],
            NonBidReason::ErrorTimeout,
            "appnexus",
        );
        builder2.reject_imps(
            &["imp-3".to_string()],
            NonBidReason::ErrorGeneral,
            "rubicon",
        );

        builder1.append(&builder2);

        let result = builder1.to_seat_non_bids();
        // Two seats: appnexus (2 non-bids) and rubicon (1 non-bid)
        assert_eq!(builder1.len(), 3);
        let appnexus = result.iter().find(|s| s.seat == "appnexus").unwrap();
        assert_eq!(appnexus.nonbid.len(), 2);
        let rubicon = result.iter().find(|s| s.seat == "rubicon").unwrap();
        assert_eq!(rubicon.nonbid.len(), 1);
    }

    #[test]
    fn test_to_seat_non_bids_empty() {
        let builder = SeatNonBidBuilder::new();
        let result = builder.to_seat_non_bids();
        assert!(result.is_empty());
    }

    #[test]
    fn test_len_and_is_empty() {
        let mut builder = SeatNonBidBuilder::new();
        assert!(builder.is_empty());
        assert_eq!(builder.len(), 0);

        builder.reject_imps(
            &["imp-1".to_string(), "imp-2".to_string()],
            NonBidReason::ErrorGeneral,
            "bidder-a",
        );
        assert!(!builder.is_empty());
        assert_eq!(builder.len(), 2);
    }
}
