//! Core types for the floors crate.
//!
//! Mirrors `openrtb_ext/floors.go` from the Go codebase. Only the
//! JSON-compatible data model is defined here; consumers bring their own
//! request/bid types.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Default currency used when a floors payload does not specify one.
pub const DEFAULT_CURRENCY: &str = "USD";
/// Default delimiter used between fields in a rule key.
pub const DEFAULT_DELIMITER: &str = "|";
/// Catch-all wildcard used in rule keys.
pub const CATCH_ALL: &str = "*";

/// Minimum and maximum rates (percentages) accepted for skip/enforce rates.
pub const RATE_MIN: i32 = 0;
/// Maximum rate (percentage) accepted for skip/enforce rates.
pub const RATE_MAX: i32 = 100;
/// Minimum valid model weight.
pub const MODEL_WEIGHT_MIN: i32 = 1;
/// Maximum valid model weight.
pub const MODEL_WEIGHT_MAX: i32 = 100;
/// Precision added when comparing floor prices.
pub const FLOOR_PRECISION: f64 = 0.01;

/// Top-level `PriceFloors` object. Some deployments (notably account
/// config) wrap the rules object inside a further struct; this type is
/// provided for symmetry with the Go `PriceFloors` naming.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PriceFloors {
    /// Whether floors are enabled.
    #[serde(default)]
    pub enabled: bool,
    /// The nested rules object, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<PriceFloorRules>,
}

/// `PriceFloorRules` defines the contract for
/// `bidrequest.ext.prebid.floors`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PriceFloorRules {
    #[serde(default, skip_serializing_if = "is_zero_f64", rename = "floormin")]
    pub floor_min: f64,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "floormincur")]
    pub floor_min_cur: String,
    #[serde(default, skip_serializing_if = "is_zero_i32", rename = "skiprate")]
    pub skip_rate: i32,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "floorendpoint")]
    pub location: Option<PriceFloorEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<PriceFloorData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enforcement: Option<PriceFloorEnforcement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped: Option<bool>,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "floorprovider")]
    pub floor_provider: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "fetchstatus")]
    pub fetch_status: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "location")]
    pub price_floor_location: String,
}

impl PriceFloorRules {
    /// Return whether floors are enabled (defaults to `true` when the
    /// field is not set, matching the Go helper semantics).
    pub fn get_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }

    /// Whether PBS-side floors enforcement is enabled.
    pub fn get_enforce_pbs(&self) -> bool {
        self.enforcement
            .as_ref()
            .and_then(|e| e.enforce_pbs)
            .unwrap_or(true)
    }

    /// Return the configured enforcement rate, defaulting to 0.
    pub fn get_enforce_rate(&self) -> i32 {
        self.enforcement.as_ref().map(|e| e.enforce_rate).unwrap_or(0)
    }

    /// Whether deal bids should have floors enforcement applied.
    pub fn get_enforce_deals_flag(&self) -> bool {
        self.enforcement
            .as_ref()
            .and_then(|e| e.floor_deals)
            .unwrap_or(false)
    }
}

/// Endpoint information for dynamic fetching.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PriceFloorEndpoint {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
}

/// `PriceFloorData` contains the schema and modelgroups used to resolve
/// the actual floor value for an impression.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PriceFloorData {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub currency: String,
    #[serde(default, skip_serializing_if = "is_zero_i32", rename = "skiprate")]
    pub skip_rate: i32,
    #[serde(default, skip_serializing_if = "is_zero_i32", rename = "floorsschemaversion")]
    pub floors_schema_version: i32,
    #[serde(default, skip_serializing_if = "is_zero_i32", rename = "modeltimestamp")]
    pub model_timestamp: i32,
    #[serde(default, rename = "modelgroups")]
    pub model_groups: Vec<PriceFloorModelGroup>,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "floorprovider")]
    pub floor_provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "usefetchdatarate")]
    pub use_fetch_data_rate: Option<i32>,
}

/// One model group inside a floors payload.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PriceFloorModelGroup {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub currency: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "modelweight")]
    pub model_weight: Option<i32>,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "modelversion")]
    pub model_version: String,
    #[serde(default, skip_serializing_if = "is_zero_i32", rename = "skiprate")]
    pub skip_rate: i32,
    #[serde(default)]
    pub schema: PriceFloorSchema,
    /// Rule keys to floor values. BTreeMap keeps deterministic iteration.
    #[serde(default)]
    pub values: BTreeMap<String, f64>,
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub default: f64,
}

/// Schema definition describing which dimensions form a rule key.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PriceFloorSchema {
    #[serde(default)]
    pub fields: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub delimiter: String,
}

/// Enforcement flags controlling how floors are enforced.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PriceFloorEnforcement {
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "enforcejs")]
    pub enforce_js: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "enforcepbs")]
    pub enforce_pbs: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "floordeals")]
    pub floor_deals: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "bidadjustment")]
    pub bid_adjustment: Option<bool>,
    #[serde(default, skip_serializing_if = "is_zero_i32", rename = "enforcerate")]
    pub enforce_rate: i32,
}

/// Known schema dimensions. Mirrors the constants in `floors/rule.go`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SchemaDimension {
    SiteDomain,
    PubDomain,
    Domain,
    Bundle,
    Channel,
    MediaType,
    Size,
    GptSlot,
    AdUnitCode,
    Country,
    DeviceType,
}

impl SchemaDimension {
    /// Attempt to parse a string into a known dimension.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "siteDomain" => Self::SiteDomain,
            "pubDomain" => Self::PubDomain,
            "domain" => Self::Domain,
            "bundle" => Self::Bundle,
            "channel" => Self::Channel,
            "mediaType" => Self::MediaType,
            "size" => Self::Size,
            "gptSlot" => Self::GptSlot,
            "adUnitCode" => Self::AdUnitCode,
            "country" => Self::Country,
            "deviceType" => Self::DeviceType,
            _ => return None,
        })
    }

    /// Name as used in schema JSON.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SiteDomain => "siteDomain",
            Self::PubDomain => "pubDomain",
            Self::Domain => "domain",
            Self::Bundle => "bundle",
            Self::Channel => "channel",
            Self::MediaType => "mediaType",
            Self::Size => "size",
            Self::GptSlot => "gptSlot",
            Self::AdUnitCode => "adUnitCode",
            Self::Country => "country",
            Self::DeviceType => "deviceType",
        }
    }
}

fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

fn is_zero_i32(v: &i32) -> bool {
    *v == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_schema_dimensions() {
        assert_eq!(
            SchemaDimension::parse("siteDomain"),
            Some(SchemaDimension::SiteDomain)
        );
        assert_eq!(SchemaDimension::parse("nope"), None);
    }

    #[test]
    fn price_floor_rules_get_enabled_default_true() {
        let rules = PriceFloorRules::default();
        assert!(rules.get_enabled());
    }

    #[test]
    fn price_floor_rules_get_enforce_pbs_default_true() {
        let rules = PriceFloorRules::default();
        assert!(rules.get_enforce_pbs());
    }

    #[test]
    fn price_floor_rules_parse_json() {
        let json = r#"{
            "floormin": 1.2,
            "floormincur": "USD",
            "enabled": true,
            "data": {
                "currency": "USD",
                "modelgroups": [
                    {
                        "currency": "USD",
                        "schema": {"fields": ["mediaType"], "delimiter": "|"},
                        "values": {"banner": 0.5, "video": 1.0}
                    }
                ]
            }
        }"#;
        let rules: PriceFloorRules = serde_json::from_str(json).expect("parse floors rules");
        assert_eq!(rules.floor_min, 1.2);
        assert_eq!(rules.floor_min_cur, "USD");
        assert_eq!(rules.data.as_ref().unwrap().model_groups.len(), 1);
        let mg = &rules.data.unwrap().model_groups[0];
        assert_eq!(mg.values.len(), 2);
        assert_eq!(mg.schema.fields, vec!["mediaType".to_string()]);
    }
}
