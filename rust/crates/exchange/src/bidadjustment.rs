/// Bid adjustment rules for modifying bid prices based on bidder, deal, and media type.
///
/// This module mirrors the Go `bidadjustment` package, supporting multiplier, CPM, and
/// static adjustment types. Rules are keyed by a composite string of
/// `mediatype|biddername|dealid` and looked up in priority order from most specific to
/// wildcard.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ── Constants ────────────────────────────────────────────────────────────────

pub const ADJUSTMENT_TYPE_CPM: &str = "cpm";
pub const ADJUSTMENT_TYPE_MULTIPLIER: &str = "multiplier";
pub const ADJUSTMENT_TYPE_STATIC: &str = "static";
pub const WILDCARD: &str = "*";
pub const DELIMITER: &str = "|";

const MAX_NUM_OF_COMBOS: usize = 8;
const PRICE_PRECISION: f64 = 10_000.0;
const MIN_BID: f64 = 0.1;

const VIDEO_INSTREAM: &str = "video-instream";
const VIDEO_OUTSTREAM: &str = "video-outstream";

// ── Types ────────────────────────────────────────────────────────────────────

/// The type of price adjustment to apply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdjustmentType {
    /// Multiply the bid price by a factor.
    Multiplier,
    /// Subtract a CPM value (possibly currency-converted) from the bid price.
    #[serde(rename = "cpm")]
    CPM,
    /// Replace the bid price with a static value.
    Static,
}

impl AdjustmentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Multiplier => ADJUSTMENT_TYPE_MULTIPLIER,
            Self::CPM => ADJUSTMENT_TYPE_CPM,
            Self::Static => ADJUSTMENT_TYPE_STATIC,
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            ADJUSTMENT_TYPE_MULTIPLIER => Some(Self::Multiplier),
            ADJUSTMENT_TYPE_CPM => Some(Self::CPM),
            ADJUSTMENT_TYPE_STATIC => Some(Self::Static),
            _ => None,
        }
    }
}

/// A single adjustment rule specifying how to modify a bid price.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdjustmentRule {
    /// The type of adjustment: "multiplier", "cpm", or "static".
    #[serde(rename = "adjtype")]
    pub adjustment_type: String,
    /// The numeric value of the adjustment.
    pub value: f64,
    /// The currency for CPM/static adjustments (required for those types).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub currency: String,
}

/// Maps deal IDs to a list of adjustments.
pub type AdjustmentsByDealId = HashMap<String, Vec<AdjustmentRule>>;

/// Media type breakdown for bid adjustments, matching Go `MediaType` struct.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MediaType {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub banner: HashMap<String, AdjustmentsByDealId>,
    #[serde(
        default,
        rename = "video-instream",
        skip_serializing_if = "HashMap::is_empty"
    )]
    pub video_instream: HashMap<String, AdjustmentsByDealId>,
    #[serde(
        default,
        rename = "video-outstream",
        skip_serializing_if = "HashMap::is_empty"
    )]
    pub video_outstream: HashMap<String, AdjustmentsByDealId>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub audio: HashMap<String, AdjustmentsByDealId>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub native: HashMap<String, AdjustmentsByDealId>,
    #[serde(default, rename = "*", skip_serializing_if = "HashMap::is_empty")]
    pub wildcard: HashMap<String, AdjustmentsByDealId>,
}

/// Top-level bid adjustments object from `req.ext.prebid.bidadjustments`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BidAdjustmentRules {
    #[serde(default)]
    pub mediatype: MediaType,
}

// ── Rule Building ────────────────────────────────────────────────────────────

/// Build a flat lookup map from the structured `BidAdjustmentRules`.
///
/// Keys are of the form `mediatype|biddername|dealid`.
pub fn build_rules(bid_adjustments: &BidAdjustmentRules) -> HashMap<String, Vec<AdjustmentRule>> {
    let mut rules = HashMap::new();

    build_rules_for_media_type("banner", &bid_adjustments.mediatype.banner, &mut rules);
    build_rules_for_media_type("audio", &bid_adjustments.mediatype.audio, &mut rules);
    build_rules_for_media_type("native", &bid_adjustments.mediatype.native, &mut rules);
    build_rules_for_media_type(VIDEO_INSTREAM, &bid_adjustments.mediatype.video_instream, &mut rules);
    build_rules_for_media_type(VIDEO_OUTSTREAM, &bid_adjustments.mediatype.video_outstream, &mut rules);
    build_rules_for_media_type(WILDCARD, &bid_adjustments.mediatype.wildcard, &mut rules);

    rules
}

fn build_rules_for_media_type(
    media_type: &str,
    rules_by_bidder: &HashMap<String, AdjustmentsByDealId>,
    rules: &mut HashMap<String, Vec<AdjustmentRule>>,
) {
    for (bidder_name, deal_map) in rules_by_bidder {
        for (deal_id, adjustments) in deal_map {
            let key = format!("{}{}{}{}{}", media_type, DELIMITER, bidder_name, DELIMITER, deal_id);
            rules.insert(key, adjustments.clone());
        }
    }
}

// ── Rule Lookup ──────────────────────────────────────────────────────────────

/// Look up the highest-priority adjustment for a bid given its type, bidder, and deal.
///
/// When a deal ID is present, the priority order (highest to lowest) is:
///   1. `bidType|bidder|dealId`
///   2. `bidType|bidder|*`
///   3. `bidType|*|dealId`
///   4. `*|bidder|dealId`
///   5. `bidType|*|*`
///   6. `*|bidder|*`
///   7. `*|*|dealId`
///   8. `*|*|*`
///
/// Without a deal ID, wildcard-deal variants are tried in a shorter priority list.
pub fn get_adjustment(
    rules: &HashMap<String, Vec<AdjustmentRule>>,
    bid_type: &str,
    bidder_name: &str,
    deal_id: &str,
) -> Option<Vec<AdjustmentRule>> {
    let mut priority_rules: Vec<String> = Vec::with_capacity(MAX_NUM_OF_COMBOS);

    if !deal_id.is_empty() {
        priority_rules.push(format!("{}{}{}{}{}", bid_type, DELIMITER, bidder_name, DELIMITER, deal_id));
        priority_rules.push(format!("{}{}{}{}{}", bid_type, DELIMITER, bidder_name, DELIMITER, WILDCARD));
        priority_rules.push(format!("{}{}{}{}{}", bid_type, DELIMITER, WILDCARD, DELIMITER, deal_id));
        priority_rules.push(format!("{}{}{}{}{}", WILDCARD, DELIMITER, bidder_name, DELIMITER, deal_id));
        priority_rules.push(format!("{}{}{}{}{}", bid_type, DELIMITER, WILDCARD, DELIMITER, WILDCARD));
        priority_rules.push(format!("{}{}{}{}{}", WILDCARD, DELIMITER, bidder_name, DELIMITER, WILDCARD));
        priority_rules.push(format!("{}{}{}{}{}", WILDCARD, DELIMITER, WILDCARD, DELIMITER, deal_id));
        priority_rules.push(format!("{}{}{}{}{}", WILDCARD, DELIMITER, WILDCARD, DELIMITER, WILDCARD));
    } else {
        priority_rules.push(format!("{}{}{}{}{}", bid_type, DELIMITER, bidder_name, DELIMITER, WILDCARD));
        priority_rules.push(format!("{}{}{}{}{}", bid_type, DELIMITER, WILDCARD, DELIMITER, WILDCARD));
        priority_rules.push(format!("{}{}{}{}{}", WILDCARD, DELIMITER, bidder_name, DELIMITER, WILDCARD));
        priority_rules.push(format!("{}{}{}{}{}", WILDCARD, DELIMITER, WILDCARD, DELIMITER, WILDCARD));
    }

    for rule_key in &priority_rules {
        if let Some(adjustments) = rules.get(rule_key) {
            return Some(adjustments.clone());
        }
    }
    None
}

// ── Adjustment Application ──────────────────────────────────────────────────

/// Apply a sequence of adjustments to a bid price, returning the adjusted price and currency.
///
/// For CPM adjustments, a `currency_convert` function is required to convert the
/// adjustment's currency to the bid's currency. If conversion fails, the original
/// price is returned unchanged.
pub fn apply_adjustments<F>(
    adjustments: &[AdjustmentRule],
    bid_price: f64,
    currency: &str,
    currency_convert: &F,
) -> (f64, String)
where
    F: Fn(f64, &str, &str) -> Result<f64, String>,
{
    if adjustments.is_empty() {
        return (bid_price, currency.to_string());
    }

    let original_bid_price = bid_price;
    let mut price = bid_price;
    let mut cur = currency.to_string();

    for adjustment in adjustments {
        match adjustment.adjustment_type.as_str() {
            ADJUSTMENT_TYPE_MULTIPLIER => {
                price *= adjustment.value;
            }
            ADJUSTMENT_TYPE_CPM => {
                match currency_convert(adjustment.value, &adjustment.currency, &cur) {
                    Ok(converted) => {
                        price -= converted;
                    }
                    Err(_) => {
                        return (original_bid_price, currency.to_string());
                    }
                }
            }
            ADJUSTMENT_TYPE_STATIC => {
                price = adjustment.value;
                cur = adjustment.currency.clone();
            }
            _ => {}
        }
    }

    let rounded = (price * PRICE_PRECISION).round() / PRICE_PRECISION;
    (rounded, cur)
}

/// Apply bid adjustments to a bid, enforcing minimum price rules.
///
/// - Deal bids: adjusted price is floored at 0.
/// - Non-deal bids: adjusted price is floored at `MIN_BID` (0.1).
pub fn apply<F>(
    rules: &HashMap<String, Vec<AdjustmentRule>>,
    bid_price: f64,
    bid_type: &str,
    bidder_name: &str,
    deal_id: &str,
    currency: &str,
    currency_convert: &F,
) -> (f64, String)
where
    F: Fn(f64, &str, &str) -> Result<f64, String>,
{
    if rules.is_empty() {
        return (bid_price, currency.to_string());
    }

    let adjustments = match get_adjustment(rules, bid_type, bidder_name, deal_id) {
        Some(adj) => adj,
        None => return (bid_price, currency.to_string()),
    };

    let (adjusted_price, adjusted_currency) =
        apply_adjustments(&adjustments, bid_price, currency, currency_convert);

    if !deal_id.is_empty() && adjusted_price < 0.0 {
        return (0.0, currency.to_string());
    }
    if deal_id.is_empty() && adjusted_price <= 0.0 {
        return (MIN_BID, currency.to_string());
    }
    (adjusted_price, adjusted_currency)
}

// ── Validation ───────────────────────────────────────────────────────────────

/// Validate all adjustment rules in a `BidAdjustmentRules` object.
///
/// Returns `true` if all adjustments are valid, `false` otherwise.
pub fn validate(bid_adjustments: &BidAdjustmentRules) -> bool {
    validate_for_media_type(&bid_adjustments.mediatype.banner)
        && validate_for_media_type(&bid_adjustments.mediatype.audio)
        && validate_for_media_type(&bid_adjustments.mediatype.video_instream)
        && validate_for_media_type(&bid_adjustments.mediatype.video_outstream)
        && validate_for_media_type(&bid_adjustments.mediatype.native)
        && validate_for_media_type(&bid_adjustments.mediatype.wildcard)
}

fn validate_for_media_type(bid_adj: &HashMap<String, AdjustmentsByDealId>) -> bool {
    for (_bidder_name, deal_map) in bid_adj {
        for (_deal_id, adjustments) in deal_map {
            for adjustment in adjustments {
                if !validate_adjustment(adjustment) {
                    return false;
                }
            }
        }
    }
    true
}

fn validate_adjustment(adjustment: &AdjustmentRule) -> bool {
    match adjustment.adjustment_type.as_str() {
        ADJUSTMENT_TYPE_CPM => {
            !adjustment.currency.is_empty()
                && adjustment.value >= 0.0
                && adjustment.value < f64::MAX
        }
        ADJUSTMENT_TYPE_MULTIPLIER => adjustment.value >= 0.0 && adjustment.value < 100.0,
        ADJUSTMENT_TYPE_STATIC => {
            !adjustment.currency.is_empty()
                && adjustment.value >= 0.0
                && adjustment.value < f64::MAX
        }
        _ => false,
    }
}

// ── Parsing ──────────────────────────────────────────────────────────────────

/// Parse bid adjustment rules from a request's `ext.prebid.bidadjustments` JSON.
pub fn parse_from_request_ext(ext: &serde_json::Value) -> Option<BidAdjustmentRules> {
    ext.get("prebid")
        .and_then(|p| p.get("bidadjustments"))
        .and_then(|ba| serde_json::from_value::<BidAdjustmentRules>(ba.clone()).ok())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn noop_convert(_val: f64, _from: &str, _to: &str) -> Result<f64, String> {
        Ok(_val)
    }

    #[test]
    fn test_apply_multiplier() {
        let adj = vec![AdjustmentRule {
            adjustment_type: ADJUSTMENT_TYPE_MULTIPLIER.to_string(),
            value: 1.5,
            currency: String::new(),
        }];
        let (price, _) = apply_adjustments(&adj, 2.0, "USD", &noop_convert);
        assert!((price - 3.0).abs() < 0.0001);
    }

    #[test]
    fn test_apply_cpm() {
        let adj = vec![AdjustmentRule {
            adjustment_type: ADJUSTMENT_TYPE_CPM.to_string(),
            value: 0.5,
            currency: "USD".to_string(),
        }];
        let (price, _) = apply_adjustments(&adj, 2.0, "USD", &noop_convert);
        assert!((price - 1.5).abs() < 0.0001);
    }

    #[test]
    fn test_apply_static() {
        let adj = vec![AdjustmentRule {
            adjustment_type: ADJUSTMENT_TYPE_STATIC.to_string(),
            value: 5.0,
            currency: "EUR".to_string(),
        }];
        let (price, cur) = apply_adjustments(&adj, 2.0, "USD", &noop_convert);
        assert!((price - 5.0).abs() < 0.0001);
        assert_eq!(cur, "EUR");
    }

    #[test]
    fn test_get_adjustment_with_deal() {
        let mut rules = HashMap::new();
        let specific = vec![AdjustmentRule {
            adjustment_type: ADJUSTMENT_TYPE_MULTIPLIER.to_string(),
            value: 2.0,
            currency: String::new(),
        }];
        let wildcard = vec![AdjustmentRule {
            adjustment_type: ADJUSTMENT_TYPE_MULTIPLIER.to_string(),
            value: 1.1,
            currency: String::new(),
        }];
        rules.insert("banner|bidderA|deal1".to_string(), specific);
        rules.insert("banner|bidderA|*".to_string(), wildcard);

        // Specific deal should match first.
        let adj = get_adjustment(&rules, "banner", "bidderA", "deal1").unwrap();
        assert!((adj[0].value - 2.0).abs() < 0.0001);

        // Unknown deal falls back to wildcard.
        let adj = get_adjustment(&rules, "banner", "bidderA", "deal2").unwrap();
        assert!((adj[0].value - 1.1).abs() < 0.0001);
    }

    #[test]
    fn test_get_adjustment_no_deal() {
        let mut rules = HashMap::new();
        let adj_rule = vec![AdjustmentRule {
            adjustment_type: ADJUSTMENT_TYPE_MULTIPLIER.to_string(),
            value: 0.9,
            currency: String::new(),
        }];
        rules.insert("banner|bidderA|*".to_string(), adj_rule);

        let adj = get_adjustment(&rules, "banner", "bidderA", "").unwrap();
        assert!((adj[0].value - 0.9).abs() < 0.0001);
    }

    #[test]
    fn test_get_adjustment_no_match() {
        let rules: HashMap<String, Vec<AdjustmentRule>> = HashMap::new();
        assert!(get_adjustment(&rules, "banner", "bidderA", "deal1").is_none());
    }

    #[test]
    fn test_apply_min_bid_no_deal() {
        let mut rules = HashMap::new();
        let adj_rule = vec![AdjustmentRule {
            adjustment_type: ADJUSTMENT_TYPE_MULTIPLIER.to_string(),
            value: 0.0,
            currency: String::new(),
        }];
        rules.insert("banner|bidderA|*".to_string(), adj_rule);

        let (price, _) = apply(&rules, 1.0, "banner", "bidderA", "", "USD", &noop_convert);
        assert!((price - MIN_BID).abs() < 0.0001);
    }

    #[test]
    fn test_apply_negative_deal_bid() {
        let mut rules = HashMap::new();
        let adj_rule = vec![AdjustmentRule {
            adjustment_type: ADJUSTMENT_TYPE_CPM.to_string(),
            value: 10.0,
            currency: "USD".to_string(),
        }];
        rules.insert("banner|bidderA|deal1".to_string(), adj_rule);

        let (price, _) =
            apply(&rules, 1.0, "banner", "bidderA", "deal1", "USD", &noop_convert);
        assert!((price - 0.0).abs() < 0.0001);
    }

    #[test]
    fn test_validate_valid() {
        let rules = BidAdjustmentRules {
            mediatype: MediaType {
                banner: {
                    let mut m = HashMap::new();
                    let mut deals = HashMap::new();
                    deals.insert(
                        "*".to_string(),
                        vec![AdjustmentRule {
                            adjustment_type: ADJUSTMENT_TYPE_MULTIPLIER.to_string(),
                            value: 1.5,
                            currency: String::new(),
                        }],
                    );
                    m.insert("bidderA".to_string(), deals);
                    m
                },
                ..Default::default()
            },
        };
        assert!(validate(&rules));
    }

    #[test]
    fn test_validate_invalid_multiplier() {
        let rules = BidAdjustmentRules {
            mediatype: MediaType {
                banner: {
                    let mut m = HashMap::new();
                    let mut deals = HashMap::new();
                    deals.insert(
                        "*".to_string(),
                        vec![AdjustmentRule {
                            adjustment_type: ADJUSTMENT_TYPE_MULTIPLIER.to_string(),
                            value: 200.0, // > 100, invalid
                            currency: String::new(),
                        }],
                    );
                    m.insert("bidderA".to_string(), deals);
                    m
                },
                ..Default::default()
            },
        };
        assert!(!validate(&rules));
    }

    #[test]
    fn test_validate_cpm_missing_currency() {
        let adj = AdjustmentRule {
            adjustment_type: ADJUSTMENT_TYPE_CPM.to_string(),
            value: 1.0,
            currency: String::new(), // missing
        };
        assert!(!validate_adjustment(&adj));
    }

    #[test]
    fn test_parse_from_request_ext() {
        let ext = serde_json::json!({
            "prebid": {
                "bidadjustments": {
                    "mediatype": {
                        "banner": {
                            "bidderA": {
                                "*": [
                                    { "adjtype": "multiplier", "value": 1.1 }
                                ]
                            }
                        }
                    }
                }
            }
        });
        let rules = parse_from_request_ext(&ext).unwrap();
        assert!(rules.mediatype.banner.contains_key("bidderA"));
    }

    #[test]
    fn test_build_and_get() {
        let ext = serde_json::json!({
            "prebid": {
                "bidadjustments": {
                    "mediatype": {
                        "banner": {
                            "bidderA": {
                                "deal1": [
                                    { "adjtype": "multiplier", "value": 2.0 }
                                ],
                                "*": [
                                    { "adjtype": "multiplier", "value": 1.1 }
                                ]
                            }
                        }
                    }
                }
            }
        });
        let adj_rules = parse_from_request_ext(&ext).unwrap();
        let rules = build_rules(&adj_rules);

        // Specific deal match
        let adj = get_adjustment(&rules, "banner", "bidderA", "deal1").unwrap();
        assert!((adj[0].value - 2.0).abs() < 0.0001);

        // Wildcard deal fallback
        let adj = get_adjustment(&rules, "banner", "bidderA", "deal99").unwrap();
        assert!((adj[0].value - 1.1).abs() < 0.0001);
    }
}
