//! Video tracking (VTrack) endpoint logic.
//!
//! Mirrors Go `endpoints/events/vtrack.go`.
//!
//! Handles VAST XML modification for video ad tracking, including
//! impression URL injection and cache integration.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::event_request;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const IMPRESSION_CLOSE_TAG: &str = "</Impression>";
const IMPRESSION_OPEN_TAG: &str = "<Impression>";

// ---------------------------------------------------------------------------
// BidCacheRequest / BidCacheResponse
// ---------------------------------------------------------------------------

/// Request to cache bid objects (VAST XML).
///
/// Mirrors Go `BidCacheRequest`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BidCacheRequest {
    pub puts: Vec<CacheableBid>,
}

/// A single cacheable bid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheableBid {
    /// Bid ID.
    #[serde(rename = "bidid", default)]
    pub bid_id: String,
    /// Bidder name.
    #[serde(default)]
    pub bidder: String,
    /// Cache type: "xml" for VAST, "json" for JSON.
    #[serde(rename = "type", default)]
    pub cache_type: String,
    /// The data to cache (VAST XML or JSON).
    pub value: Value,
    /// Time-to-live in seconds.
    #[serde(default)]
    pub ttlseconds: i32,
    /// Custom cache key.
    #[serde(default)]
    pub key: String,
    /// Auction timestamp.
    #[serde(default)]
    pub timestamp: i64,
    /// Whether VAST modification is allowed for this bid.
    #[serde(rename = "bidder", skip)]
    _bidder_for_check: String,
}

/// Response from the cache after storing bids.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BidCacheResponse {
    pub responses: Vec<CacheObject>,
}

/// A cached object with its UUID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheObject {
    pub uuid: String,
}

// ---------------------------------------------------------------------------
// VAST URL tracking
// ---------------------------------------------------------------------------

/// Create a VAST tracking URL.
///
/// Mirrors Go `GetVastUrlTracking`.
pub fn get_vast_url_tracking(
    external_url: &str,
    bid_id: &str,
    bidder: &str,
    account_id: &str,
    timestamp: i64,
    integration: &str,
) -> String {
    let req = event_request::EventRequest {
        event_type: event_request::EventType::Imp,
        bid_id: bid_id.to_string(),
        account_id: account_id.to_string(),
        bidder: bidder.to_string(),
        timestamp,
        integration_type: integration.to_string(),
        ..Default::default()
    };
    event_request::event_request_to_url(external_url, &req)
}

/// Modify a VAST XML string by injecting an impression tracking URL.
///
/// Returns (modified_vast, was_modified).
///
/// Mirrors Go `ModifyVastXmlString`.
pub fn modify_vast_xml_string(
    external_url: &str,
    vast: &str,
    bid_id: &str,
    bidder: &str,
    account_id: &str,
    timestamp: i64,
    integration_type: &str,
) -> (String, bool) {
    if vast.is_empty() {
        return (vast.to_string(), false);
    }

    let tracking_url = get_vast_url_tracking(
        external_url,
        bid_id,
        bidder,
        account_id,
        timestamp,
        integration_type,
    );

    let impression_tag = format!(
        "{}<![CDATA[{}]]>{}",
        IMPRESSION_OPEN_TAG, tracking_url, IMPRESSION_CLOSE_TAG
    );

    // Find the last </Impression> tag and insert before it
    if let Some(pos) = vast.rfind(IMPRESSION_CLOSE_TAG) {
        let insert_pos = pos + IMPRESSION_CLOSE_TAG.len();
        let mut modified = String::with_capacity(vast.len() + impression_tag.len());
        modified.push_str(&vast[..insert_pos]);
        modified.push_str(&impression_tag);
        modified.push_str(&vast[insert_pos..]);
        return (modified, true);
    }

    // If no Impression tag found, try inserting before </InLine> or </Wrapper>
    for close_tag in &["</InLine>", "</Wrapper>"] {
        if let Some(pos) = vast.rfind(close_tag) {
            let mut modified = String::with_capacity(vast.len() + impression_tag.len());
            modified.push_str(&vast[..pos]);
            modified.push_str(&impression_tag);
            modified.push_str(&vast[pos..]);
            return (modified, true);
        }
    }

    (vast.to_string(), false)
}

/// Modify a VAST XML stored as a JSON value.
///
/// Mirrors Go `ModifyVastXmlJSON`.
pub fn modify_vast_xml_json(
    external_url: &str,
    data: &Value,
    bid_id: &str,
    bidder: &str,
    account_id: &str,
    timestamp: i64,
    integration_type: &str,
) -> Value {
    let xml_str = match data.as_str() {
        Some(s) => s,
        None => return data.clone(),
    };

    let (modified, _) = modify_vast_xml_string(
        external_url,
        xml_str,
        bid_id,
        bidder,
        account_id,
        timestamp,
        integration_type,
    );

    Value::String(modified)
}

/// Get the set of bidders that allow VAST modification.
///
/// Mirrors Go `getBiddersAllowingVastUpdate`.
pub fn get_bidders_allowing_vast_update(
    request: &BidCacheRequest,
    bidder_infos: &std::collections::HashMap<String, BidderVastConfig>,
    allow_unknown_bidder: bool,
) -> HashSet<String> {
    let mut allowed = HashSet::new();

    for put in &request.puts {
        if is_allow_vast_for_bidder(&put.bidder, bidder_infos, allow_unknown_bidder) {
            allowed.insert(put.bidder.clone());
        }
    }

    allowed
}

/// Bidder configuration for VAST modification.
#[derive(Debug, Clone, Default)]
pub struct BidderVastConfig {
    pub modifying_vast_xml_allowed: bool,
}

/// Check if a bidder allows VAST XML modification.
pub fn is_allow_vast_for_bidder(
    bidder: &str,
    bidder_infos: &std::collections::HashMap<String, BidderVastConfig>,
    allow_unknown_bidder: bool,
) -> bool {
    match bidder_infos.get(bidder) {
        Some(info) => info.modifying_vast_xml_allowed,
        None => allow_unknown_bidder,
    }
}

/// Parse a VTrack request from raw JSON bytes.
///
/// Mirrors Go `ParseVTrackRequest`.
pub fn parse_vtrack_request(body: &[u8], max_size: usize) -> Result<BidCacheRequest, String> {
    if body.len() > max_size {
        return Err(format!(
            "request body size {} exceeds max size {}",
            body.len(),
            max_size
        ));
    }

    serde_json::from_slice(body).map_err(|e| format!("failed to parse vtrack request: {}", e))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_vast_url_tracking() {
        let url = get_vast_url_tracking(
            "https://prebid.example.com",
            "bid-123",
            "appnexus",
            "acct-456",
            1234567890,
            "pbjs",
        );
        assert!(url.contains("t=imp"));
        assert!(url.contains("b=bid-123"));
        assert!(url.contains("a=acct-456"));
        assert!(url.contains("bidder=appnexus"));
        assert!(url.contains("ts=1234567890"));
        assert!(url.contains("int=pbjs"));
    }

    #[test]
    fn test_modify_vast_xml_string_with_impression() {
        let vast = r#"<VAST><Ad><InLine><Impression><![CDATA[https://original.com/imp]]></Impression></InLine></Ad></VAST>"#;
        let (modified, was_modified) = modify_vast_xml_string(
            "https://prebid.example.com",
            vast,
            "bid-1",
            "appnexus",
            "acct-1",
            123,
            "pbjs",
        );
        assert!(was_modified);
        assert!(modified.contains("https://prebid.example.com/event?"));
        assert!(modified.contains("https://original.com/imp"));
    }

    #[test]
    fn test_modify_vast_xml_string_no_impression() {
        let vast =
            r#"<VAST><Ad><InLine><Creatives></Creatives></InLine></Ad></VAST>"#;
        let (modified, was_modified) = modify_vast_xml_string(
            "https://prebid.example.com",
            vast,
            "bid-1",
            "appnexus",
            "acct-1",
            123,
            "",
        );
        assert!(was_modified);
        assert!(modified.contains("<Impression>"));
    }

    #[test]
    fn test_modify_vast_xml_string_empty() {
        let (result, was_modified) = modify_vast_xml_string(
            "https://prebid.example.com",
            "",
            "bid-1",
            "appnexus",
            "acct-1",
            0,
            "",
        );
        assert!(!was_modified);
        assert_eq!(result, "");
    }

    #[test]
    fn test_modify_vast_xml_json() {
        let data = Value::String(
            r#"<VAST><Ad><InLine><Impression><![CDATA[https://orig.com]]></Impression></InLine></Ad></VAST>"#.to_string(),
        );
        let result =
            modify_vast_xml_json("https://prebid.example.com", &data, "b1", "an", "a1", 0, "");
        assert!(result.as_str().unwrap().contains("<Impression>"));
    }

    #[test]
    fn test_is_allow_vast_for_bidder() {
        let mut infos = std::collections::HashMap::new();
        infos.insert(
            "appnexus".to_string(),
            BidderVastConfig {
                modifying_vast_xml_allowed: true,
            },
        );
        infos.insert(
            "rubicon".to_string(),
            BidderVastConfig {
                modifying_vast_xml_allowed: false,
            },
        );

        assert!(is_allow_vast_for_bidder("appnexus", &infos, false));
        assert!(!is_allow_vast_for_bidder("rubicon", &infos, false));
        assert!(!is_allow_vast_for_bidder("unknown", &infos, false));
        assert!(is_allow_vast_for_bidder("unknown", &infos, true));
    }

    #[test]
    fn test_parse_vtrack_request() {
        let body = serde_json::json!({
            "puts": [
                {
                    "bidid": "bid-1",
                    "bidder": "appnexus",
                    "type": "xml",
                    "value": "<VAST></VAST>",
                    "ttlseconds": 300
                }
            ]
        });
        let bytes = serde_json::to_vec(&body).unwrap();
        let req = parse_vtrack_request(&bytes, 10000).unwrap();
        assert_eq!(req.puts.len(), 1);
        assert_eq!(req.puts[0].bid_id, "bid-1");
    }

    #[test]
    fn test_parse_vtrack_request_too_large() {
        let body = vec![0u8; 100];
        let result = parse_vtrack_request(&body, 50);
        assert!(result.is_err());
    }
}
