//! Prebid-specific extensions for `BidRequest.ext`.
//!
//! Mirrors the Go types declared in `openrtb_ext/request.go`
//! (`ExtRequest`, `ExtRequestPrebid`, `ExtRequestTargeting`,
//! `ExtRequestPrebidCache`, `PriceGranularity`, ...).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `BidRequest.ext` — the Prebid wrapper for the top-level ext object.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prebid: Option<ExtRequestPrebid>,
    /// Raw schain object (openrtb2.SupplyChain in Go).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schain: Option<Value>,
}

/// `BidRequest.ext.prebid` — the Prebid-specific body.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtRequestPrebid {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidadjustmentfactors: Option<HashMap<String, f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub targeting: Option<Targeting>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache: Option<Cache>,
    /// schains list (each entry is `{bidders, schain}`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schains: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storedrequest: Option<Value>,
}

/// `ExtRequestTargeting` — `bidrequest.ext.prebid.targeting`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Targeting {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pricegranularity: Option<PriceGranularity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub includewinners: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub includebidderkeys: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub includeformat: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub durationrangesec: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferdeals: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub appendbiddernames: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alwaysincludedeals: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
}

/// `PriceGranularity` — `bidrequest.ext.prebid.targeting.pricegranularity`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PriceGranularity {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precision: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ranges: Option<Vec<GranularityRange>>,
}

/// A single entry inside `PriceGranularity.ranges`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct GranularityRange {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub increment: Option<f64>,
}

/// `Cache` — `bidrequest.ext.prebid.cache`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Cache {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bids: Option<CacheBids>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vastxml: Option<CacheVastXml>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CacheBids {
    #[serde(rename = "returnCreative", skip_serializing_if = "Option::is_none")]
    pub return_creative: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CacheVastXml {
    #[serde(rename = "returnCreative", skip_serializing_if = "Option::is_none")]
    pub return_creative: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ext_request() {
        let mut aliases = HashMap::new();
        aliases.insert("alias1".to_string(), "appnexus".to_string());

        let ext = ExtRequest {
            prebid: Some(ExtRequestPrebid {
                debug: Some(true),
                aliases: Some(aliases),
                bidadjustmentfactors: Some(HashMap::from([("rubicon".to_string(), 1.5f64)])),
                targeting: Some(Targeting {
                    pricegranularity: Some(PriceGranularity {
                        precision: Some(2),
                        ranges: Some(vec![GranularityRange {
                            min: Some(0.0),
                            max: Some(20.0),
                            increment: Some(0.1),
                        }]),
                    }),
                    includewinners: Some(true),
                    ..Default::default()
                }),
                cache: Some(Cache {
                    bids: Some(CacheBids { return_creative: Some(true) }),
                    vastxml: None,
                }),
                ..Default::default()
            }),
            schain: None,
        };

        let json = serde_json::to_string(&ext).unwrap();
        let parsed: ExtRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(ext, parsed);
    }
}
