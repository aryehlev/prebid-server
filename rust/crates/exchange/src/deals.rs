//! Deal support logic for the exchange.
//!
//! Ports the Go deal-related functions from `exchange/exchange.go` and
//! `openrtb_ext/deal_tier.go`.

use std::collections::HashMap;

use openrtb::Imp;
use openrtb_ext::{BidderName, DealTier, ExtMultiBid};
use serde::Deserialize;

/// Default maximum number of bids to update per bidder when no multibid
/// configuration is present.
const DEFAULT_BID_LIMIT: usize = 1;

/// A map from bidder name to its `DealTier` configuration for a single impression.
pub type DealTierBidderMap = HashMap<BidderName, DealTier>;

/// A PBS-enriched bid object that carries deal-related metadata alongside
/// the raw OpenRTB bid.
#[derive(Debug, Clone, Default)]
pub struct PbsOrtbBid {
    /// The underlying OpenRTB bid.
    pub bid: openrtb::Bid,
    /// Deal priority extracted from the bidder adapter response.
    pub deal_priority: i32,
    /// Whether the deal tier requirement was satisfied.
    pub deal_tier_satisfied: bool,
}

// ---------------------------------------------------------------------------
// Reading deal tiers from the impression
// ---------------------------------------------------------------------------

/// Internal deserialization helper mirroring the Go anonymous struct in
/// `ReadDealTiersFromImp`.
#[derive(Deserialize, Default)]
struct ImpPrebidExt {
    #[serde(default)]
    prebid: ImpPrebidBidders,
}

#[derive(Deserialize, Default)]
struct ImpPrebidBidders {
    #[serde(default)]
    bidder: HashMap<String, BidderDealTierWrapper>,
}

#[derive(Deserialize, Default)]
struct BidderDealTierWrapper {
    #[serde(rename = "dealTier")]
    deal_tier: Option<DealTier>,
}

/// Extract per-bidder deal tier configuration from a single impression's ext.
///
/// Reads `imp.ext.prebid.bidder.<name>.dealTier` for every bidder present.
/// Bidder names are normalized to lowercase.
///
/// Equivalent of Go `ReadDealTiersFromImp`.
pub fn get_deal_tiers(imp: &Imp) -> Result<DealTierBidderMap, String> {
    let mut deal_tiers = DealTierBidderMap::new();

    let ext_val = match &imp.ext {
        Some(v) => v,
        None => return Ok(deal_tiers),
    };

    // An empty object / null value is valid -- just means no deal tiers.
    if ext_val.is_null() {
        return Ok(deal_tiers);
    }

    let imp_prebid: ImpPrebidExt = serde_json::from_value(ext_val.clone())
        .map_err(|e| format!("failed to unmarshal imp ext for deal tiers: {e}"))?;

    for (bidder_raw, wrapper) in imp_prebid.prebid.bidder {
        if let Some(dt) = wrapper.deal_tier {
            // Normalize the bidder name to lowercase (mirrors Go NormalizeBidderName).
            let normalized = BidderName::new(bidder_raw.to_lowercase());
            deal_tiers.insert(normalized, dt);
        }
    }

    Ok(deal_tiers)
}

/// Build a map of imp ID -> `DealTierBidderMap` for all impressions in a request.
///
/// Equivalent of Go `getDealTiers`.
pub fn get_deal_tiers_from_request(imps: &[Imp]) -> HashMap<String, DealTierBidderMap> {
    let mut imp_deal_map: HashMap<String, DealTierBidderMap> = HashMap::new();

    for imp in imps {
        match get_deal_tiers(imp) {
            Ok(dt_map) => {
                imp_deal_map.insert(imp.id.clone(), dt_map);
            }
            Err(_) => {
                // Malformed ext -- skip this impression, matches Go behaviour.
                continue;
            }
        }
    }

    imp_deal_map
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Returns `true` if the deal tier has a non-empty prefix and a positive
/// `min_deal_tier`.
///
/// Equivalent of Go `validateDealTier`.
pub fn validate_deal_tier(deal_tier: &DealTier) -> bool {
    let prefix_ok = deal_tier
        .prefix
        .as_ref()
        .map_or(false, |p| !p.is_empty());
    let tier_ok = deal_tier.min_deal_tier.map_or(false, |t| t > 0);
    prefix_ok && tier_ok
}

// ---------------------------------------------------------------------------
// Applying deal support
// ---------------------------------------------------------------------------

/// Update the `hb_pb_cat_dur` targeting key value by prepending the deal
/// prefix and priority.
///
/// Equivalent of Go `updateHbPbCatDur`.
pub fn update_hb_pb_cat_dur(
    bid: &mut PbsOrtbBid,
    deal_tier: &DealTier,
    bid_category: &mut HashMap<String, String>,
) {
    let min_deal_tier = deal_tier.min_deal_tier.unwrap_or(0);
    if bid.deal_priority >= min_deal_tier {
        let prefix = deal_tier.prefix.as_deref().unwrap_or("");
        let prefix_tier = format!("{}{}_", prefix, bid.deal_priority);
        bid.deal_tier_satisfied = true;

        if let Some(old_cat_dur) = bid_category.get(&bid.bid.id) {
            let new_cat_dur = match old_cat_dur.find('_') {
                Some(idx) => {
                    // Replace everything before the first '_' (inclusive) with the prefix tier.
                    format!("{}{}", prefix_tier, &old_cat_dur[idx + 1..])
                }
                None => {
                    // No underscore -- just prepend.
                    format!("{}{}", prefix_tier, old_cat_dur)
                }
            };
            bid_category.insert(bid.bid.id.clone(), new_cat_dur);
        }
    }
}

/// Determine the maximum number of bids to update for a given bidder based
/// on the multi-bid configuration.
///
/// Equivalent of Go `bidsToUpdate`.
fn bids_to_update(multi_bid: &HashMap<String, ExtMultiBid>, bidder: &str) -> usize {
    if let Some(mb) = multi_bid.get(bidder) {
        if mb.target_bidder_code_prefix.as_ref().map_or(false, |p| !p.is_empty()) {
            return mb.maxbids.unwrap_or(DEFAULT_BID_LIMIT as i32) as usize;
        }
    }
    DEFAULT_BID_LIMIT
}

/// Represents all bids for a single impression, organized by bidder.
/// Key: bidder name, Value: ordered list of bids (highest priority first).
pub type BidsByBidder = HashMap<BidderName, Vec<PbsOrtbBid>>;

/// Represents all bids across all impressions.
/// Key: impression ID, Value: `BidsByBidder`.
pub type AllBidsByBidder = HashMap<String, BidsByBidder>;

/// Apply deal support by updating targeting keys with deal prefixes when
/// the bid's deal priority meets or exceeds the minimum deal tier.
///
/// Returns a list of error messages for invalid deal tier configurations.
///
/// Equivalent of Go `applyDealSupport`.
pub fn apply_deal_support(
    imps: &[Imp],
    all_bids: &mut AllBidsByBidder,
    bid_category: &mut HashMap<String, String>,
    multi_bid: &HashMap<String, ExtMultiBid>,
) -> Vec<String> {
    let mut errs = Vec::new();
    let imp_deal_map = get_deal_tiers_from_request(imps);

    for (imp_id, bids_per_imp) in all_bids.iter_mut() {
        let imp_deal = match imp_deal_map.get(imp_id) {
            Some(d) => d,
            None => continue,
        };

        for (bidder, top_bids) in bids_per_imp.iter_mut() {
            let bidder_normalized = BidderName::new(bidder.as_str().to_lowercase());
            let max_bid = bids_to_update(multi_bid, bidder_normalized.as_str());

            for (i, top_bid) in top_bids.iter_mut().enumerate() {
                if i >= max_bid {
                    break;
                }
                if top_bid.deal_priority > 0 {
                    if let Some(tier) = imp_deal.get(&bidder_normalized) {
                        if validate_deal_tier(tier) {
                            update_hb_pb_cat_dur(top_bid, tier, bid_category);
                        } else {
                            errs.push(format!(
                                "dealTier configuration invalid for bidder '{}', imp ID '{}'",
                                bidder, imp_id
                            ));
                        }
                    }
                }
            }
        }
    }

    errs
}

/// Look up a deal-based bid adjustment factor.
///
/// Given a map of `bidder -> deal_id -> adjustment_factor`, returns the
/// adjustment factor for the specified bidder and deal. Falls back to the
/// wildcard deal ID (`"*"`) if an exact match is not found.
pub fn deal_adjustment_factor(
    adjustments: &HashMap<String, HashMap<String, f64>>,
    bidder: &str,
    deal_id: &str,
) -> Option<f64> {
    let bidder_adjustments = adjustments.get(bidder)?;
    bidder_adjustments
        .get(deal_id)
        .or_else(|| bidder_adjustments.get("*"))
        .copied()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use openrtb::Imp;

    #[test]
    fn test_get_deal_tiers_none_ext() {
        let imp = Imp {
            id: "imp1".to_string(),
            ext: None,
            ..Default::default()
        };
        let result = get_deal_tiers(&imp).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_get_deal_tiers_empty_object() {
        let imp = Imp {
            id: "imp1".to_string(),
            ext: Some(serde_json::json!({})),
            ..Default::default()
        };
        let result = get_deal_tiers(&imp).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_get_deal_tiers_one_bidder() {
        let imp = Imp {
            id: "imp1".to_string(),
            ext: Some(serde_json::json!({
                "prebid": {
                    "bidder": {
                        "appnexus": {
                            "dealTier": {
                                "prefix": "tier",
                                "minDealTier": 5
                            },
                            "placementId": 12345
                        }
                    }
                }
            })),
            ..Default::default()
        };

        let result = get_deal_tiers(&imp).unwrap();
        assert_eq!(result.len(), 1);
        let dt = result.get(&BidderName::new("appnexus")).unwrap();
        assert_eq!(dt.prefix.as_deref(), Some("tier"));
        assert_eq!(dt.min_deal_tier, Some(5));
    }

    #[test]
    fn test_get_deal_tiers_multiple_bidders() {
        let imp = Imp {
            id: "imp1".to_string(),
            ext: Some(serde_json::json!({
                "prebid": {
                    "bidder": {
                        "appnexus": {
                            "dealTier": {"prefix": "apnx", "minDealTier": 10}
                        },
                        "rubicon": {
                            "dealTier": {"prefix": "rubi", "minDealTier": 3}
                        }
                    }
                }
            })),
            ..Default::default()
        };

        let result = get_deal_tiers(&imp).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_get_deal_tiers_no_deal_tier_key() {
        let imp = Imp {
            id: "imp1".to_string(),
            ext: Some(serde_json::json!({
                "prebid": {
                    "bidder": {
                        "appnexus": {"placementId": 12345}
                    }
                }
            })),
            ..Default::default()
        };

        let result = get_deal_tiers(&imp).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_get_deal_tiers_dedupe_case_insensitive() {
        let imp = Imp {
            id: "imp1".to_string(),
            ext: Some(serde_json::json!({
                "prebid": {
                    "bidder": {
                        "APPNEXUS": {"dealTier": {"minDealTier": 100}},
                        "APpNExUS": {"dealTier": {"minDealTier": 5}}
                    }
                }
            })),
            ..Default::default()
        };

        let result = get_deal_tiers(&imp).unwrap();
        // Both normalize to "appnexus", last one wins in HashMap iteration order
        assert_eq!(result.len(), 1);
        assert!(result.contains_key(&BidderName::new("appnexus")));
    }

    #[test]
    fn test_get_deal_tiers_invalid_ext() {
        let imp = Imp {
            id: "imp1".to_string(),
            ext: Some(serde_json::json!({
                "prebid": {
                    "bidder": {
                        "appnexus": {"dealTier": "wrong type"}
                    }
                }
            })),
            ..Default::default()
        };

        let result = get_deal_tiers(&imp);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_deal_tier_valid() {
        let dt = DealTier {
            prefix: Some("deal".to_string()),
            min_deal_tier: Some(5),
        };
        assert!(validate_deal_tier(&dt));
    }

    #[test]
    fn test_validate_deal_tier_empty_prefix() {
        let dt = DealTier {
            prefix: Some(String::new()),
            min_deal_tier: Some(5),
        };
        assert!(!validate_deal_tier(&dt));
    }

    #[test]
    fn test_validate_deal_tier_none_prefix() {
        let dt = DealTier {
            prefix: None,
            min_deal_tier: Some(5),
        };
        assert!(!validate_deal_tier(&dt));
    }

    #[test]
    fn test_validate_deal_tier_zero_min() {
        let dt = DealTier {
            prefix: Some("deal".to_string()),
            min_deal_tier: Some(0),
        };
        assert!(!validate_deal_tier(&dt));
    }

    #[test]
    fn test_validate_deal_tier_none_min() {
        let dt = DealTier {
            prefix: Some("deal".to_string()),
            min_deal_tier: None,
        };
        assert!(!validate_deal_tier(&dt));
    }

    #[test]
    fn test_update_hb_pb_cat_dur_meets_tier() {
        let mut bid = PbsOrtbBid {
            bid: openrtb::Bid {
                id: "bid1".to_string(),
                impid: "imp1".to_string(),
                ..Default::default()
            },
            deal_priority: 10,
            deal_tier_satisfied: false,
        };
        let dt = DealTier {
            prefix: Some("tier".to_string()),
            min_deal_tier: Some(5),
        };
        let mut bid_category = HashMap::new();
        bid_category.insert("bid1".to_string(), "oldprefix_catdur".to_string());

        update_hb_pb_cat_dur(&mut bid, &dt, &mut bid_category);

        assert!(bid.deal_tier_satisfied);
        assert_eq!(bid_category.get("bid1").unwrap(), "tier10_catdur");
    }

    #[test]
    fn test_update_hb_pb_cat_dur_below_tier() {
        let mut bid = PbsOrtbBid {
            bid: openrtb::Bid {
                id: "bid1".to_string(),
                impid: "imp1".to_string(),
                ..Default::default()
            },
            deal_priority: 3,
            deal_tier_satisfied: false,
        };
        let dt = DealTier {
            prefix: Some("tier".to_string()),
            min_deal_tier: Some(5),
        };
        let mut bid_category = HashMap::new();
        bid_category.insert("bid1".to_string(), "oldprefix_catdur".to_string());

        update_hb_pb_cat_dur(&mut bid, &dt, &mut bid_category);

        assert!(!bid.deal_tier_satisfied);
        assert_eq!(bid_category.get("bid1").unwrap(), "oldprefix_catdur");
    }

    #[test]
    fn test_update_hb_pb_cat_dur_no_category_entry() {
        let mut bid = PbsOrtbBid {
            bid: openrtb::Bid {
                id: "bid1".to_string(),
                impid: "imp1".to_string(),
                ..Default::default()
            },
            deal_priority: 10,
            deal_tier_satisfied: false,
        };
        let dt = DealTier {
            prefix: Some("tier".to_string()),
            min_deal_tier: Some(5),
        };
        let mut bid_category = HashMap::new();

        update_hb_pb_cat_dur(&mut bid, &dt, &mut bid_category);

        // deal_tier_satisfied should be true but no category to update
        assert!(bid.deal_tier_satisfied);
        assert!(!bid_category.contains_key("bid1"));
    }

    #[test]
    fn test_apply_deal_support_basic() {
        let imps = vec![Imp {
            id: "imp1".to_string(),
            ext: Some(serde_json::json!({
                "prebid": {
                    "bidder": {
                        "appnexus": {
                            "dealTier": {"prefix": "pfx", "minDealTier": 5}
                        }
                    }
                }
            })),
            ..Default::default()
        }];

        let bidder = BidderName::new("appnexus");
        let mut bids_by_bidder: BidsByBidder = HashMap::new();
        bids_by_bidder.insert(
            bidder.clone(),
            vec![PbsOrtbBid {
                bid: openrtb::Bid {
                    id: "bid1".to_string(),
                    impid: "imp1".to_string(),
                    ..Default::default()
                },
                deal_priority: 10,
                deal_tier_satisfied: false,
            }],
        );

        let mut all_bids: AllBidsByBidder = HashMap::new();
        all_bids.insert("imp1".to_string(), bids_by_bidder);

        let mut bid_category = HashMap::new();
        bid_category.insert("bid1".to_string(), "old_catdur".to_string());

        let multi_bid = HashMap::new();

        let errs = apply_deal_support(&imps, &mut all_bids, &mut bid_category, &multi_bid);

        assert!(errs.is_empty());
        assert_eq!(bid_category.get("bid1").unwrap(), "pfx10_catdur");
        let bids = all_bids.get("imp1").unwrap().get(&bidder).unwrap();
        assert!(bids[0].deal_tier_satisfied);
    }

    #[test]
    fn test_apply_deal_support_invalid_tier() {
        let imps = vec![Imp {
            id: "imp1".to_string(),
            ext: Some(serde_json::json!({
                "prebid": {
                    "bidder": {
                        "appnexus": {
                            "dealTier": {"prefix": "", "minDealTier": 0}
                        }
                    }
                }
            })),
            ..Default::default()
        }];

        let bidder = BidderName::new("appnexus");
        let mut bids_by_bidder: BidsByBidder = HashMap::new();
        bids_by_bidder.insert(
            bidder,
            vec![PbsOrtbBid {
                bid: openrtb::Bid {
                    id: "bid1".to_string(),
                    impid: "imp1".to_string(),
                    ..Default::default()
                },
                deal_priority: 10,
                deal_tier_satisfied: false,
            }],
        );

        let mut all_bids: AllBidsByBidder = HashMap::new();
        all_bids.insert("imp1".to_string(), bids_by_bidder);

        let mut bid_category = HashMap::new();
        let multi_bid = HashMap::new();

        let errs = apply_deal_support(&imps, &mut all_bids, &mut bid_category, &multi_bid);

        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("dealTier configuration invalid"));
    }

    #[test]
    fn test_deal_adjustment_factor_exact() {
        let mut adjustments = HashMap::new();
        let mut deals = HashMap::new();
        deals.insert("deal123".to_string(), 1.5);
        deals.insert("*".to_string(), 1.0);
        adjustments.insert("appnexus".to_string(), deals);

        assert_eq!(
            deal_adjustment_factor(&adjustments, "appnexus", "deal123"),
            Some(1.5)
        );
    }

    #[test]
    fn test_deal_adjustment_factor_wildcard() {
        let mut adjustments = HashMap::new();
        let mut deals = HashMap::new();
        deals.insert("*".to_string(), 0.9);
        adjustments.insert("appnexus".to_string(), deals);

        assert_eq!(
            deal_adjustment_factor(&adjustments, "appnexus", "unknown_deal"),
            Some(0.9)
        );
    }

    #[test]
    fn test_deal_adjustment_factor_no_bidder() {
        let adjustments: HashMap<String, HashMap<String, f64>> = HashMap::new();

        assert_eq!(
            deal_adjustment_factor(&adjustments, "appnexus", "deal123"),
            None
        );
    }

    #[test]
    fn test_deal_adjustment_factor_no_deal_no_wildcard() {
        let mut adjustments = HashMap::new();
        let mut deals = HashMap::new();
        deals.insert("other_deal".to_string(), 1.2);
        adjustments.insert("appnexus".to_string(), deals);

        assert_eq!(
            deal_adjustment_factor(&adjustments, "appnexus", "deal123"),
            None
        );
    }

    #[test]
    fn test_bids_to_update_default() {
        let multi_bid = HashMap::new();
        assert_eq!(bids_to_update(&multi_bid, "appnexus"), DEFAULT_BID_LIMIT);
    }

    #[test]
    fn test_bids_to_update_with_multibid() {
        let mut multi_bid = HashMap::new();
        multi_bid.insert(
            "appnexus".to_string(),
            ExtMultiBid {
                bidder: Some("appnexus".to_string()),
                bidders: None,
                maxbids: Some(3),
                target_bidder_code_prefix: Some("apnx".to_string()),
            },
        );

        assert_eq!(bids_to_update(&multi_bid, "appnexus"), 3);
    }

    #[test]
    fn test_bids_to_update_multibid_no_prefix() {
        let mut multi_bid = HashMap::new();
        multi_bid.insert(
            "appnexus".to_string(),
            ExtMultiBid {
                bidder: Some("appnexus".to_string()),
                bidders: None,
                maxbids: Some(3),
                target_bidder_code_prefix: None,
            },
        );

        // Without a prefix the multibid entry is not "fully defined".
        assert_eq!(bids_to_update(&multi_bid, "appnexus"), DEFAULT_BID_LIMIT);
    }

    #[test]
    fn test_get_deal_tiers_from_request() {
        let imps = vec![
            Imp {
                id: "imp1".to_string(),
                ext: Some(serde_json::json!({
                    "prebid": {"bidder": {"appnexus": {"dealTier": {"prefix": "a", "minDealTier": 1}}}}
                })),
                ..Default::default()
            },
            Imp {
                id: "imp2".to_string(),
                ext: None,
                ..Default::default()
            },
        ];

        let result = get_deal_tiers_from_request(&imps);
        assert_eq!(result.len(), 2);
        assert!(result.get("imp1").unwrap().contains_key(&BidderName::new("appnexus")));
        assert!(result.get("imp2").unwrap().is_empty());
    }
}
