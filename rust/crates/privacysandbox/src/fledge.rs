//! FLEDGE / Protected Audience interest-group and auction-config types.
//!
//! The Go tree does not yet expose a first-class FLEDGE module outside the
//! OpenRTB extensions, so the shapes below are modelled after the
//! `PAAuctionSupport` fields found in the prebid.js specification and the
//! IAB OpenRTB Audience auction config response shape.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Top-level configuration for FLEDGE on a single impression.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FledgeConfig {
    /// Whether the host is allowed to run on-device interest group auctions.
    #[serde(default)]
    pub enabled: bool,

    /// List of buyers (bidder names) eligible for interest group bidding.
    #[serde(default, rename = "interestGroupAuctionBuyers")]
    pub interest_group_auction_buyers: Vec<InterestGroupAuctionBuyers>,
}

/// A single buyer entry in a FLEDGE auction config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterestGroupAuctionBuyers {
    /// The bidder name.
    pub bidder: String,
    /// The bidder's origin as it should appear in the browser auction config.
    #[serde(default)]
    pub origin: String,
    /// Whether this buyer is allowed to participate in the auction.
    #[serde(default)]
    pub allowed: bool,
}

/// Signals passed to a bidder as part of a FLEDGE auction config.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuctionSignals {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub per_buyer_signals: BTreeMap<String, serde_json::Value>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub per_buyer_timeouts: BTreeMap<String, u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seller_signals: Option<serde_json::Value>,
}

/// The auction config returned to a FLEDGE-compatible bidder.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FledgeAuctionConfig {
    /// Bidder this config belongs to.
    pub bidder: String,

    /// OpenRTB impression id this config is associated with.
    #[serde(rename = "impId", default)]
    pub imp_id: String,

    /// Raw auction config, opaque to the server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fledge_config_roundtrip() {
        let cfg = FledgeConfig {
            enabled: true,
            interest_group_auction_buyers: vec![InterestGroupAuctionBuyers {
                bidder: "rubicon".into(),
                origin: "https://rubicon.example".into(),
                allowed: true,
            }],
        };
        let s = serde_json::to_string(&cfg).unwrap();
        let back: FledgeConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn auction_signals_empty_defaults() {
        let s = AuctionSignals::default();
        assert!(s.per_buyer_signals.is_empty());
        assert!(s.per_buyer_timeouts.is_empty());
        assert!(s.seller_signals.is_none());

        let out = serde_json::to_string(&s).unwrap();
        // All fields skip when empty, so the JSON is an empty object.
        assert_eq!(out, "{}");
    }

    #[test]
    fn fledge_auction_config_wraps_opaque_value() {
        let cfg = FledgeAuctionConfig {
            bidder: "appnexus".into(),
            imp_id: "imp-1".into(),
            config: Some(json!({"seller": "https://appnexus.example"})),
        };
        let s = serde_json::to_string(&cfg).unwrap();
        assert!(s.contains("\"bidder\":\"appnexus\""));
        assert!(s.contains("\"impId\":\"imp-1\""));
        assert!(s.contains("\"seller\":\"https://appnexus.example\""));
    }
}
