//! Prebid-specific extensions for `Imp.ext`.
//!
//! Mirrors `openrtb_ext/imp.go` (`ExtImp`, `ExtImpPrebid`,
//! `ExtStoredRequest`, `ExtStoredAuctionResponse`, `ExtStoredBidResponse`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `ExtImp` — the `imp[].ext` wrapper.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtImp {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prebid: Option<ExtImpPrebid>,
    /// Arbitrary bidder-specific parameters keyed by bidder code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidder: Option<HashMap<String, Value>>,
}

/// `ExtImpPrebid` — `imp[].ext.prebid`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtImpPrebid {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storedrequest: Option<ExtStoredRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storedauctionresponse: Option<ExtStoredAuctionResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storedbidresponse: Option<Vec<ExtStoredBidResponse>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_rewarded_inventory: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidder: Option<HashMap<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adunitcode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passthrough: Option<Value>,
}

/// `ExtStoredRequest` — `imp[].ext.prebid.storedrequest`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtStoredRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// `ExtStoredAuctionResponse` — `imp[].ext.prebid.storedauctionresponse`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtStoredAuctionResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// `ExtStoredBidResponse` — `imp[].ext.prebid.storedbidresponse[]`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtStoredBidResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaceimpid: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ext_imp() {
        let imp = ExtImp {
            prebid: Some(ExtImpPrebid {
                storedrequest: Some(ExtStoredRequest { id: Some("sr-1".into()) }),
                storedauctionresponse: Some(ExtStoredAuctionResponse {
                    id: Some("sar-1".into()),
                }),
                storedbidresponse: Some(vec![ExtStoredBidResponse {
                    id: Some("sbr-1".into()),
                    bidder: Some("appnexus".into()),
                    replaceimpid: Some(true),
                }]),
                is_rewarded_inventory: Some(1),
                adunitcode: Some("div-1".into()),
                ..Default::default()
            }),
            bidder: None,
        };

        let json = serde_json::to_string(&imp).unwrap();
        let parsed: ExtImp = serde_json::from_str(&json).unwrap();
        assert_eq!(imp, parsed);
    }
}
