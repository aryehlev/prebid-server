//! Prebid-specific extensions for `BidResponse.ext`.
//!
//! Mirrors `openrtb_ext/response.go` (`ExtBidResponse`, `ExtResponseDebug`,
//! `ExtResponsePrebid`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `ExtBidResponse` — `bidresponse.ext`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtBidResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<ExtResponseDebug>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<HashMap<String, Vec<ExtBidderMessage>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<HashMap<String, Vec<ExtBidderMessage>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub responsetimemillis: Option<HashMap<String, i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tmaxrequest: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usersync: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prebid: Option<ExtResponsePrebid>,
}

/// `ExtResponseDebug` — `bidresponse.ext.debug`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtResponseDebug {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub httpcalls: Option<HashMap<String, Vec<Value>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolvedrequest: Option<Value>,
}

/// `ExtResponsePrebid` — `bidresponse.ext.prebid`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtResponsePrebid {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auctiontimestamp: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passthrough: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modules: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fledge: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub targeting: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seatnonbid: Option<Vec<Value>>,
}

/// `ExtBidderMessage` — one entry in `errors`/`warnings`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtBidderMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ext_bid_response() {
        let resp = ExtBidResponse {
            debug: Some(ExtResponseDebug {
                httpcalls: Some(HashMap::from([(
                    "appnexus".to_string(),
                    vec![serde_json::json!({"uri": "https://example.com"})],
                )])),
                resolvedrequest: None,
            }),
            errors: Some(HashMap::from([(
                "appnexus".to_string(),
                vec![ExtBidderMessage {
                    code: Some(1),
                    message: Some("boom".into()),
                }],
            )])),
            warnings: None,
            responsetimemillis: Some(HashMap::from([("appnexus".to_string(), 42i64)])),
            tmaxrequest: Some(1000),
            usersync: None,
            prebid: Some(ExtResponsePrebid {
                auctiontimestamp: Some(1_700_000_000_000),
                targeting: Some(HashMap::from([(
                    "hb_pb".to_string(),
                    "1.50".to_string(),
                )])),
                ..Default::default()
            }),
        };

        let s = serde_json::to_string(&resp).unwrap();
        let parsed: ExtBidResponse = serde_json::from_str(&s).unwrap();
        assert_eq!(resp, parsed);
    }
}
