//! Prebid-specific extensions for `Source.ext`.
//!
//! Mirrors `openrtb_ext/source.go` (`ExtSource`) and defines a
//! self-contained `SupplyChain` / `SupplyChainNode` pair so this crate
//! does not depend on the upstream `openrtb` crate.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `ExtSource` — `source.ext`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schain: Option<SupplyChain>,
}

/// Convenience alias for the `{bidders, schain}` entry used in
/// `ext.prebid.schains[]`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtSourceSchain {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidders: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schain: Option<SupplyChain>,
}

/// Self-contained supply chain object matching `openrtb2.SupplyChain`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SupplyChain {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub complete: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nodes: Option<Vec<SupplyChainNode>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ver: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Value>,
}

/// Self-contained supply chain node matching `openrtb2.SupplyChainNode`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SupplyChainNode {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asi: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hp: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<HashMap<String, Value>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ext_source() {
        let es = ExtSource {
            schain: Some(SupplyChain {
                complete: Some(1),
                ver: Some("1.0".into()),
                nodes: Some(vec![SupplyChainNode {
                    asi: Some("exchange1.com".into()),
                    sid: Some("1234".into()),
                    hp: Some(1),
                    ..Default::default()
                }]),
                ext: None,
            }),
        };
        let s = serde_json::to_string(&es).unwrap();
        let parsed: ExtSource = serde_json::from_str(&s).unwrap();
        assert_eq!(es, parsed);
    }
}
