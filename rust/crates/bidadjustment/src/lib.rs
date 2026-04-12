//! Bid adjustment rules, ported from the Go `bidadjustment` package.
//!
//! Exposes [`Adjustments`] with a nested `mediatype -> bidder ->
//! dealid -> [adjustment]` shape, [`build_rules`] for flattening those
//! into a priority-ordered lookup, [`apply_adjustments`] for mutating
//! a bid price according to matched rules, and [`validate`] for
//! config-time correctness checks. Self-contained: no dependency on
//! the `openrtb` crates.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Adjustment type constants.
pub const ADJUSTMENT_TYPE_CPM: &str = "cpm";
pub const ADJUSTMENT_TYPE_MULTIPLIER: &str = "multiplier";
pub const ADJUSTMENT_TYPE_STATIC: &str = "static";
pub const WILDCARD: &str = "*";
pub const DELIMITER: &str = "|";

/// Bid type discriminators used when building rule keys.
pub const MEDIA_TYPE_BANNER: &str = "banner";
pub const MEDIA_TYPE_AUDIO: &str = "audio";
pub const MEDIA_TYPE_NATIVE: &str = "native";
pub const MEDIA_TYPE_VIDEO_INSTREAM: &str = "video-instream";
pub const MEDIA_TYPE_VIDEO_OUTSTREAM: &str = "video-outstream";

/// Minimum allowed bid price after an adjustment for non-deal bids.
pub const MIN_BID: f64 = 0.1;

const PRICE_PRECISION: f64 = 10_000.0;
const MAX_NUM_OF_COMBOS: usize = 8;

/// One adjustment entry.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Adjustment {
    #[serde(rename = "adjtype")]
    pub adj_type: String,
    #[serde(default)]
    pub value: f64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub currency: String,
}

/// `dealid -> [adjustment]`
pub type AdjustmentsByDealId = HashMap<String, Vec<Adjustment>>;
/// `bidder -> dealid -> [adjustment]`
pub type AdjustmentsByBidder = HashMap<String, AdjustmentsByDealId>;

/// Top-level mediatype-keyed adjustments map. Mirrors the Go
/// `ExtRequestPrebidBidAdjustments.MediaType` struct.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaTypeAdjustments {
    #[serde(default)]
    pub banner: AdjustmentsByBidder,
    #[serde(default)]
    pub audio: AdjustmentsByBidder,
    #[serde(default)]
    pub native: AdjustmentsByBidder,
    #[serde(default, rename = "video-instream")]
    pub video_instream: AdjustmentsByBidder,
    #[serde(default, rename = "video-outstream")]
    pub video_outstream: AdjustmentsByBidder,
    #[serde(default, rename = "*")]
    pub wildcard: AdjustmentsByBidder,
}

/// `ExtRequestPrebidBidAdjustments` wrapper.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Adjustments {
    #[serde(default, rename = "mediatype")]
    pub media_type: MediaTypeAdjustments,
}

/// Validation errors.
#[derive(Debug, Error, PartialEq)]
pub enum AdjustmentError {
    #[error("invalid adjustment for bidder {0}: nil adjustments map")]
    NilBidderMap(String),
    #[error("invalid adjustment: nil adjustment slice for bidder {0}, deal {1}")]
    NilDealSlice(String, String),
    #[error("invalid adjustment value/type/currency combination")]
    InvalidAdjustment,
}

/// Currency conversion callback used by [`apply_adjustments`].
pub type ConvertFn = dyn Fn(f64, &str, &str) -> Option<f64> + Send + Sync;

/// Validate an [`Adjustments`] struct. Mirrors `Validate` in
/// `bidadjustment/validate.go`.
pub fn validate(adj: Option<&Adjustments>) -> Result<(), AdjustmentError> {
    let Some(adj) = adj else { return Ok(()) };
    let mt = &adj.media_type;
    validate_for_mediatype(&mt.banner)?;
    validate_for_mediatype(&mt.audio)?;
    validate_for_mediatype(&mt.video_instream)?;
    validate_for_mediatype(&mt.video_outstream)?;
    validate_for_mediatype(&mt.native)?;
    validate_for_mediatype(&mt.wildcard)?;
    Ok(())
}

fn validate_for_mediatype(by_bidder: &AdjustmentsByBidder) -> Result<(), AdjustmentError> {
    for (bidder, by_deal) in by_bidder {
        if by_deal.is_empty() {
            // Matches Go behaviour: nil inner map is rejected; empty
            // is tolerated here since Rust cannot distinguish nil vs
            // empty for HashMap — keep as Ok to avoid over-rejecting.
            let _ = bidder;
            continue;
        }
        for (deal, adjustments) in by_deal {
            if adjustments.is_empty() {
                return Err(AdjustmentError::NilDealSlice(
                    bidder.clone(),
                    deal.clone(),
                ));
            }
            for a in adjustments {
                if !validate_adjustment(a) {
                    return Err(AdjustmentError::InvalidAdjustment);
                }
            }
        }
    }
    Ok(())
}

fn validate_adjustment(a: &Adjustment) -> bool {
    match a.adj_type.as_str() {
        ADJUSTMENT_TYPE_CPM | ADJUSTMENT_TYPE_STATIC => {
            !a.currency.is_empty() && a.value >= 0.0 && a.value.is_finite()
        }
        ADJUSTMENT_TYPE_MULTIPLIER => a.value >= 0.0 && a.value < 100.0,
        _ => false,
    }
}

/// Flatten [`Adjustments`] into a `rule-key -> [adjustment]` map.
pub fn build_rules(adj: Option<&Adjustments>) -> HashMap<String, Vec<Adjustment>> {
    let mut rules = HashMap::new();
    let Some(adj) = adj else {
        return rules;
    };
    let mt = &adj.media_type;
    build_rules_for_mediatype(MEDIA_TYPE_BANNER, &mt.banner, &mut rules);
    build_rules_for_mediatype(MEDIA_TYPE_AUDIO, &mt.audio, &mut rules);
    build_rules_for_mediatype(MEDIA_TYPE_NATIVE, &mt.native, &mut rules);
    build_rules_for_mediatype(MEDIA_TYPE_VIDEO_INSTREAM, &mt.video_instream, &mut rules);
    build_rules_for_mediatype(MEDIA_TYPE_VIDEO_OUTSTREAM, &mt.video_outstream, &mut rules);
    build_rules_for_mediatype(WILDCARD, &mt.wildcard, &mut rules);
    rules
}

fn build_rules_for_mediatype(
    media_type: &str,
    rules_by_bidder: &AdjustmentsByBidder,
    out: &mut HashMap<String, Vec<Adjustment>>,
) {
    for (bidder, by_deal) in rules_by_bidder {
        for (deal_id, adjustments) in by_deal {
            let key = format!("{media_type}{DELIMITER}{bidder}{DELIMITER}{deal_id}");
            out.insert(key, adjustments.clone());
        }
    }
}

/// Apply matching adjustments to a bid price. Returns `(new_price,
/// new_currency)`. Mirrors `Apply` in `bidadjustment/apply.go`.
pub fn apply_adjustments(
    rules: &HashMap<String, Vec<Adjustment>>,
    bid_type: &str,
    bidder: &str,
    deal_id: Option<&str>,
    bid_price: f64,
    currency: &str,
    convert: &ConvertFn,
) -> (f64, String) {
    if rules.is_empty() {
        return (bid_price, currency.to_string());
    }

    let adjustments = match get_priority_adjustments(rules, bid_type, bidder, deal_id) {
        Some(v) => v,
        None => return (bid_price, currency.to_string()),
    };

    let (new_price, new_currency) = apply(adjustments, bid_price, currency.to_string(), convert);

    if deal_id.is_some() && new_price < 0.0 {
        return (0.0, currency.to_string());
    }
    if deal_id.is_none() && new_price <= 0.0 {
        return (MIN_BID, currency.to_string());
    }
    (new_price, new_currency)
}

fn apply(
    adjustments: &[Adjustment],
    mut bid_price: f64,
    mut currency: String,
    convert: &ConvertFn,
) -> (f64, String) {
    if adjustments.is_empty() {
        return (bid_price, currency);
    }
    let original_price = bid_price;
    let original_currency = currency.clone();

    for a in adjustments {
        match a.adj_type.as_str() {
            ADJUSTMENT_TYPE_MULTIPLIER => {
                bid_price *= a.value;
            }
            ADJUSTMENT_TYPE_CPM => {
                let Some(converted) = convert(a.value, &a.currency, &currency) else {
                    return (original_price, original_currency);
                };
                bid_price -= converted;
            }
            ADJUSTMENT_TYPE_STATIC => {
                bid_price = a.value;
                currency = a.currency.clone();
            }
            _ => {}
        }
    }

    let rounded = (bid_price * PRICE_PRECISION).round() / PRICE_PRECISION;
    (rounded, currency)
}

fn apply_wrapper(
    adjustments: &[Adjustment],
    bid_price: f64,
    currency: &str,
    convert: &ConvertFn,
) -> (f64, String) {
    apply(adjustments, bid_price, currency.to_string(), convert)
}

fn get_priority_adjustments<'a>(
    rules: &'a HashMap<String, Vec<Adjustment>>,
    bid_type: &str,
    bidder: &str,
    deal_id: Option<&str>,
) -> Option<&'a [Adjustment]> {
    let mut priority: [Option<String>; MAX_NUM_OF_COMBOS] = Default::default();

    if let Some(deal) = deal_id.filter(|d| !d.is_empty()) {
        priority[0] = Some(format!("{bid_type}|{bidder}|{deal}"));
        priority[1] = Some(format!("{bid_type}|{bidder}|*"));
        priority[2] = Some(format!("{bid_type}|*|{deal}"));
        priority[3] = Some(format!("*|{bidder}|{deal}"));
        priority[4] = Some(format!("{bid_type}|*|*"));
        priority[5] = Some(format!("*|{bidder}|*"));
        priority[6] = Some(format!("*|*|{deal}"));
        priority[7] = Some("*|*|*".to_string());
    } else {
        priority[0] = Some(format!("{bid_type}|{bidder}|*"));
        priority[1] = Some(format!("{bid_type}|*|*"));
        priority[2] = Some(format!("*|{bidder}|*"));
        priority[3] = Some("*|*|*".to_string());
    }

    for p in priority.iter().flatten() {
        if let Some(v) = rules.get(p) {
            return Some(v.as_slice());
        }
    }
    None
}

// silence unused warning for helper kept for parity with Go.
#[allow(dead_code)]
fn _keep_apply_wrapper(
    adj: &[Adjustment],
    bid_price: f64,
    currency: &str,
    c: &ConvertFn,
) -> (f64, String) {
    apply_wrapper(adj, bid_price, currency, c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same_currency(value: f64, _from: &str, _to: &str) -> Option<f64> {
        Some(value)
    }

    #[test]
    fn apply_multiplier() {
        let convert: &ConvertFn = &same_currency;
        let mut rules = HashMap::new();
        rules.insert(
            "banner|bidderA|*".to_string(),
            vec![Adjustment {
                adj_type: ADJUSTMENT_TYPE_MULTIPLIER.into(),
                value: 0.5,
                currency: String::new(),
            }],
        );
        let (p, c) = apply_adjustments(&rules, "banner", "bidderA", None, 2.0, "USD", convert);
        assert_eq!(p, 1.0);
        assert_eq!(c, "USD");
    }

    #[test]
    fn apply_cpm_subtract() {
        let convert: &ConvertFn = &same_currency;
        let mut rules = HashMap::new();
        rules.insert(
            "banner|bidderA|*".to_string(),
            vec![Adjustment {
                adj_type: ADJUSTMENT_TYPE_CPM.into(),
                value: 0.5,
                currency: "USD".into(),
            }],
        );
        let (p, _) = apply_adjustments(&rules, "banner", "bidderA", None, 2.0, "USD", convert);
        assert_eq!(p, 1.5);
    }

    #[test]
    fn apply_static_replaces() {
        let convert: &ConvertFn = &same_currency;
        let mut rules = HashMap::new();
        rules.insert(
            "banner|bidderA|*".to_string(),
            vec![Adjustment {
                adj_type: ADJUSTMENT_TYPE_STATIC.into(),
                value: 3.0,
                currency: "EUR".into(),
            }],
        );
        let (p, c) = apply_adjustments(&rules, "banner", "bidderA", None, 2.0, "USD", convert);
        assert_eq!(p, 3.0);
        assert_eq!(c, "EUR");
    }

    #[test]
    fn apply_non_deal_below_zero_goes_to_min_bid() {
        let convert: &ConvertFn = &same_currency;
        let mut rules = HashMap::new();
        rules.insert(
            "banner|bidderA|*".to_string(),
            vec![Adjustment {
                adj_type: ADJUSTMENT_TYPE_MULTIPLIER.into(),
                value: 0.0,
                currency: String::new(),
            }],
        );
        let (p, _) = apply_adjustments(&rules, "banner", "bidderA", None, 2.0, "USD", convert);
        assert_eq!(p, MIN_BID);
    }

    #[test]
    fn build_rules_flattens_structure() {
        let mut adj = Adjustments::default();
        let mut bidder_map = HashMap::new();
        bidder_map.insert(
            "*".to_string(),
            vec![Adjustment {
                adj_type: ADJUSTMENT_TYPE_MULTIPLIER.into(),
                value: 1.0,
                currency: String::new(),
            }],
        );
        adj.media_type
            .banner
            .insert("bidderA".to_string(), bidder_map);

        let rules = build_rules(Some(&adj));
        assert!(rules.contains_key("banner|bidderA|*"));
    }

    #[test]
    fn validate_accepts_good_rules() {
        let mut adj = Adjustments::default();
        let mut bidder_map = HashMap::new();
        bidder_map.insert(
            "*".to_string(),
            vec![Adjustment {
                adj_type: ADJUSTMENT_TYPE_MULTIPLIER.into(),
                value: 0.5,
                currency: String::new(),
            }],
        );
        adj.media_type
            .banner
            .insert("bidderA".to_string(), bidder_map);

        assert!(validate(Some(&adj)).is_ok());
    }

    #[test]
    fn validate_rejects_bad_multiplier() {
        let mut adj = Adjustments::default();
        let mut bidder_map = HashMap::new();
        bidder_map.insert(
            "*".to_string(),
            vec![Adjustment {
                adj_type: ADJUSTMENT_TYPE_MULTIPLIER.into(),
                value: 500.0,
                currency: String::new(),
            }],
        );
        adj.media_type
            .banner
            .insert("bidderA".to_string(), bidder_map);
        assert!(validate(Some(&adj)).is_err());
    }

    #[test]
    fn priority_picks_most_specific() {
        let mut rules = HashMap::new();
        rules.insert(
            "banner|bidderA|deal1".to_string(),
            vec![Adjustment {
                adj_type: ADJUSTMENT_TYPE_STATIC.into(),
                value: 1.0,
                currency: "USD".into(),
            }],
        );
        rules.insert(
            "banner|*|*".to_string(),
            vec![Adjustment {
                adj_type: ADJUSTMENT_TYPE_STATIC.into(),
                value: 9.0,
                currency: "USD".into(),
            }],
        );
        let convert: &ConvertFn = &same_currency;
        let (p, _) =
            apply_adjustments(&rules, "banner", "bidderA", Some("deal1"), 2.0, "USD", convert);
        assert_eq!(p, 1.0);
    }
}
