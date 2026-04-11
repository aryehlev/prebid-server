//! ORTB2 Blocking module.
//!
//! Mirrors Go `modules/prebid/ortb2blocking/` — blocks bids based on
//! advertiser domains (badv), categories (bcat), apps (bapp), banner
//! types (btype), and creative attributes (battr).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use crate::hookstage::*;
use crate::{Module, Stage};

// ---------------------------------------------------------------------------
// Module definition
// ---------------------------------------------------------------------------

/// ORTB2 blocking module.
pub struct Ortb2BlockingModule;

impl Module for Ortb2BlockingModule {
    fn stages(&self) -> Vec<Stage> {
        vec![Stage::BidderRequest, Stage::RawBidderResponse]
    }
}

/// Builder function for the ortb2blocking module.
pub fn builder(_cfg: Value) -> Result<Box<dyn Module>, String> {
    Ok(Box::new(Ortb2BlockingModule))
}

// ---------------------------------------------------------------------------
// Config — mirrors Go ortb2blocking/config.go
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub attributes: Attributes,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Attributes {
    #[serde(default)]
    pub badv: Badv,
    #[serde(default)]
    pub bcat: Bcat,
    #[serde(default)]
    pub bapp: Bapp,
    #[serde(default)]
    pub btype: Btype,
    #[serde(default)]
    pub battr: Battr,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Badv {
    #[serde(default)]
    pub blocked_adomain: Vec<String>,
    #[serde(default)]
    pub allowed_adomain_for_deals: Vec<String>,
    #[serde(default)]
    pub block_unknown_adomain: bool,
    #[serde(default)]
    pub enforce_blocks: bool,
    #[serde(default)]
    pub action_overrides: BadvActionOverride,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BadvActionOverride {
    #[serde(default)]
    pub blocked_adomain: Vec<ActionOverride>,
    #[serde(default)]
    pub allowed_adomain_for_deals: Vec<ActionOverride>,
    #[serde(default)]
    pub block_unknown_adomain: Vec<ActionOverride>,
    #[serde(default)]
    pub enforce_blocks: Vec<ActionOverride>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Bcat {
    #[serde(default)]
    pub blocked_adv_cat: Vec<String>,
    #[serde(default)]
    pub allowed_adv_cat_for_deals: Vec<String>,
    #[serde(default)]
    pub block_unknown_adv_cat: bool,
    #[serde(default)]
    pub category_taxonomy: i32,
    #[serde(default)]
    pub enforce_blocks: bool,
    #[serde(default)]
    pub action_overrides: BcatActionOverride,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BcatActionOverride {
    #[serde(default)]
    pub blocked_adv_cat: Vec<ActionOverride>,
    #[serde(default)]
    pub allowed_adv_cat_for_deals: Vec<ActionOverride>,
    #[serde(default)]
    pub block_unknown_adv_cat: Vec<ActionOverride>,
    #[serde(default)]
    pub enforce_blocks: Vec<ActionOverride>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Bapp {
    #[serde(default)]
    pub blocked_app: Vec<String>,
    #[serde(default)]
    pub allowed_app_for_deals: Vec<String>,
    #[serde(default)]
    pub enforce_blocks: bool,
    #[serde(default)]
    pub action_overrides: BappActionOverride,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BappActionOverride {
    #[serde(default)]
    pub blocked_app: Vec<ActionOverride>,
    #[serde(default)]
    pub allowed_app_for_deals: Vec<ActionOverride>,
    #[serde(default)]
    pub enforce_blocks: Vec<ActionOverride>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Btype {
    #[serde(default)]
    pub blocked_banner_type: Vec<i32>,
    #[serde(default)]
    pub action_overrides: BtypeActionOverride,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BtypeActionOverride {
    #[serde(default)]
    pub blocked_banner_type: Vec<ActionOverride>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Battr {
    #[serde(default)]
    pub blocked_banner_attr: Vec<i32>,
    #[serde(default)]
    pub blocked_video_attr: Vec<i32>,
    #[serde(default)]
    pub blocked_audio_attr: Vec<i32>,
    #[serde(default)]
    pub enforce_blocks: bool,
    #[serde(default)]
    pub action_overrides: BattrActionOverride,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BattrActionOverride {
    #[serde(default)]
    pub blocked_banner_attr: Vec<ActionOverride>,
    #[serde(default)]
    pub blocked_video_attr: Vec<ActionOverride>,
    #[serde(default)]
    pub blocked_audio_attr: Vec<ActionOverride>,
    #[serde(default)]
    pub enforce_blocks: Vec<ActionOverride>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ActionOverride {
    #[serde(default)]
    pub conditions: Conditions,
    #[serde(default)]
    pub r#override: OverrideValue,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Conditions {
    #[serde(default)]
    pub bidders: Vec<String>,
    #[serde(default)]
    pub media_types: Vec<String>,
    #[serde(default)]
    pub deal_ids: Vec<String>,
}

/// Override can be a boolean or a list of strings/ints.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OverrideValue {
    #[default]
    None,
    Bool(bool),
    Strings(Vec<String>),
    Ints(Vec<i32>),
}

// ---------------------------------------------------------------------------
// Blocking logic — mirrors Go ortb2blocking/hook_raw_bidder_response.go
// ---------------------------------------------------------------------------

/// Result of checking whether a bid should be blocked.
#[derive(Debug, Clone)]
pub struct BlockCheckResult {
    pub blocked: bool,
    pub failed_attributes: Vec<String>,
    pub details: HashMap<String, Value>,
}

/// Check if a bid's advertiser domains should be blocked.
pub fn should_block_badv(
    bid_adomains: &[String],
    blocked: &[String],
    allowed_for_deals: &[String],
    block_unknown: bool,
    is_deal: bool,
) -> bool {
    if bid_adomains.is_empty() && block_unknown {
        return true;
    }

    for adomain in bid_adomains {
        // Check deal exceptions
        if is_deal && allowed_for_deals.iter().any(|a| a.eq_ignore_ascii_case(adomain)) {
            continue;
        }
        if blocked.iter().any(|b| b.eq_ignore_ascii_case(adomain)) {
            return true;
        }
    }

    false
}

/// Check if a bid's category should be blocked.
pub fn should_block_bcat(
    bid_cats: &[String],
    blocked: &[String],
    allowed_for_deals: &[String],
    block_unknown: bool,
    is_deal: bool,
) -> bool {
    if bid_cats.is_empty() && block_unknown {
        return true;
    }

    for cat in bid_cats {
        if is_deal && allowed_for_deals.iter().any(|a| a.eq_ignore_ascii_case(cat)) {
            continue;
        }
        if blocked.iter().any(|b| b.eq_ignore_ascii_case(cat)) {
            return true;
        }
    }

    false
}

/// Check if a bid's app bundle should be blocked.
pub fn should_block_bapp(
    bid_bundle: &str,
    blocked: &[String],
    allowed_for_deals: &[String],
    is_deal: bool,
) -> bool {
    if bid_bundle.is_empty() {
        return false;
    }
    if is_deal && allowed_for_deals.iter().any(|a| a.eq_ignore_ascii_case(bid_bundle)) {
        return false;
    }
    blocked.iter().any(|b| b.eq_ignore_ascii_case(bid_bundle))
}

/// Utility: case-insensitive string match.
fn has_matches(list: &[String], s: &str) -> bool {
    list.iter().any(|v| v.eq_ignore_ascii_case(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder() {
        let module = builder(Value::Null).unwrap();
        let stages = module.stages();
        assert!(stages.contains(&Stage::BidderRequest));
        assert!(stages.contains(&Stage::RawBidderResponse));
    }

    #[test]
    fn test_config_parse() {
        let json = r#"{
            "attributes": {
                "badv": {
                    "blocked_adomain": ["blocked.com"],
                    "enforce_blocks": true
                }
            }
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.attributes.badv.blocked_adomain, vec!["blocked.com"]);
        assert!(cfg.attributes.badv.enforce_blocks);
    }

    #[test]
    fn test_should_block_badv_match() {
        let bid_adomains = vec!["blocked.com".to_string()];
        let blocked = vec!["blocked.com".to_string()];
        assert!(should_block_badv(&bid_adomains, &blocked, &[], false, false));
    }

    #[test]
    fn test_should_block_badv_no_match() {
        let bid_adomains = vec!["allowed.com".to_string()];
        let blocked = vec!["blocked.com".to_string()];
        assert!(!should_block_badv(&bid_adomains, &blocked, &[], false, false));
    }

    #[test]
    fn test_should_block_badv_unknown() {
        assert!(should_block_badv(&[], &[], &[], true, false));
        assert!(!should_block_badv(&[], &[], &[], false, false));
    }

    #[test]
    fn test_should_block_badv_deal_exception() {
        let bid_adomains = vec!["blocked.com".to_string()];
        let blocked = vec!["blocked.com".to_string()];
        let allowed_for_deals = vec!["blocked.com".to_string()];
        assert!(!should_block_badv(&bid_adomains, &blocked, &allowed_for_deals, false, true));
    }

    #[test]
    fn test_should_block_bcat() {
        let bid_cats = vec!["IAB1".to_string()];
        let blocked = vec!["IAB1".to_string()];
        assert!(should_block_bcat(&bid_cats, &blocked, &[], false, false));
        assert!(!should_block_bcat(&bid_cats, &["IAB2".to_string()], &[], false, false));
    }

    #[test]
    fn test_should_block_bapp() {
        assert!(should_block_bapp("com.bad.app", &["com.bad.app".to_string()], &[], false));
        assert!(!should_block_bapp("com.good.app", &["com.bad.app".to_string()], &[], false));
        assert!(!should_block_bapp("", &["com.bad.app".to_string()], &[], false));
    }
}
