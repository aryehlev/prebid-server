//! Event URL generation and tracking injection.
//!
//! Mirrors the Go implementation in `exchange/events.go`.
//! Provides functionality to:
//! - Generate tracking URLs for win/impression/click events
//! - Inject event URLs into bid extensions
//! - Inject tracking pixels into VAST XML for video bids

use std::collections::HashMap;

use openrtb_ext::{BidType, ExtBidPrebidEvents};

use crate::analytics::EventType;

// ---------------------------------------------------------------------------
// Event tracking configuration
// ---------------------------------------------------------------------------

/// Configuration for event tracking, gathered from account and request settings.
#[derive(Debug, Clone)]
pub struct EventTracking {
    /// The account ID for the publisher.
    pub account_id: String,
    /// Whether events are enabled at the account level.
    pub enabled_for_account: bool,
    /// Whether events are enabled at the request level (via `req.ext.prebid.events`).
    pub enabled_for_request: bool,
    /// Auction timestamp in milliseconds since epoch.
    pub auction_timestamp_ms: i64,
    /// Integration type from `req.ext.prebid.integration`.
    pub integration_type: String,
    /// Map of bidder name to bidder-specific info (e.g., whether VAST modification is allowed).
    pub bidder_infos: HashMap<String, BidderEventInfo>,
    /// External URL (scheme + host) for this PBS instance.
    pub external_url: String,
}

/// Per-bidder event configuration.
#[derive(Debug, Clone, Default)]
pub struct BidderEventInfo {
    /// Whether this bidder allows VAST XML modification for event tracking.
    pub modifying_vast_xml_allowed: bool,
}

/// Request parameters used to build an event URL.
#[derive(Debug, Clone)]
pub struct EventRequest {
    pub event_type: EventType,
    pub bid_id: String,
    pub bidder: String,
    pub account_id: String,
    pub timestamp: i64,
    pub integration: String,
}

// ---------------------------------------------------------------------------
// URL generation
// ---------------------------------------------------------------------------

/// Builds a tracking URL from an event request and the external URL base.
///
/// The URL format follows the PBS `/event` endpoint convention:
/// `{external_url}/event?t={type}&b={bid_id}&a={account_id}&bidder={bidder}&ts={timestamp}&int={integration}&f=b`
pub fn create_tracking_url(external_url: &str, req: &EventRequest) -> String {
    let event_type_str = match req.event_type {
        EventType::Win => "win",
        EventType::Imp => "imp",
        EventType::Vast => "vast",
    };

    let mut url = format!(
        "{}/event?t={}&b={}&a={}&bidder={}&ts={}",
        external_url.trim_end_matches('/'),
        event_type_str,
        urlencoding(req.bid_id.as_str()),
        urlencoding(req.account_id.as_str()),
        urlencoding(req.bidder.as_str()),
        req.timestamp,
    );

    if !req.integration.is_empty() {
        url.push_str("&int=");
        url.push_str(&urlencoding(&req.integration));
    }

    // Default to blank response format
    url.push_str("&f=b");
    url
}

/// Minimal URL-encoding for query parameter values.
fn urlencoding(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            ' ' => out.push_str("%20"),
            '&' => out.push_str("%26"),
            '=' => out.push_str("%3D"),
            '+' => out.push_str("%2B"),
            '%' => out.push_str("%25"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            _ => out.push(ch),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// EventTracking methods
// ---------------------------------------------------------------------------

impl EventTracking {
    /// Returns `true` if events are enabled (either for account or request).
    pub fn is_event_allowed(&self) -> bool {
        self.enabled_for_account || self.enabled_for_request
    }

    /// Returns `true` if the given bidder allows VAST XML modifications and events are enabled.
    pub fn is_modifying_vast_xml_allowed(&self, bidder_name: &str) -> bool {
        self.bidder_infos
            .get(bidder_name)
            .map(|info| info.modifying_vast_xml_allowed)
            .unwrap_or(false)
            && self.is_event_allowed()
    }

    /// Builds a tracking URL for the given event type, bid ID, and bidder.
    pub fn make_event_url(
        &self,
        event_type: EventType,
        bid_id: &str,
        bidder: &str,
    ) -> String {
        create_tracking_url(
            &self.external_url,
            &EventRequest {
                event_type,
                bid_id: bid_id.to_string(),
                bidder: bidder.to_string(),
                account_id: self.account_id.clone(),
                timestamp: self.auction_timestamp_ms,
                integration: self.integration_type.clone(),
            },
        )
    }

    /// Creates bid ext events (win + imp URLs) for a non-video bid.
    /// Returns `None` if events are not allowed or the bid is a video type.
    pub fn make_bid_ext_events(
        &self,
        bid_id: &str,
        bid_type: &BidType,
        bidder: &str,
    ) -> Option<ExtBidPrebidEvents> {
        if !self.is_event_allowed() || *bid_type == BidType::Video {
            return None;
        }
        Some(ExtBidPrebidEvents {
            win: Some(self.make_event_url(EventType::Win, bid_id, bidder)),
            imp: Some(self.make_event_url(EventType::Imp, bid_id, bidder)),
        })
    }

    /// Injects a win URL ("wurl") into the bid JSON for non-video bids when
    /// events are enabled.
    ///
    /// Returns the (possibly modified) JSON bytes.
    pub fn inject_tracking_into_bid(
        &self,
        bid_type: &BidType,
        bid_id: &str,
        bidder: &str,
        json_bytes: &[u8],
    ) -> Result<Vec<u8>, serde_json::Error> {
        if !self.is_event_allowed() || *bid_type == BidType::Video {
            return Ok(json_bytes.to_vec());
        }

        let win_url = self.make_event_url(EventType::Win, bid_id, bidder);

        let mut value: serde_json::Value = serde_json::from_slice(json_bytes)?;
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "wurl".to_string(),
                serde_json::Value::String(win_url),
            );
        }
        serde_json::to_vec(&value)
    }
}

// ---------------------------------------------------------------------------
// VAST event tracking
// ---------------------------------------------------------------------------

/// Helper for injecting impression tracking pixels into VAST XML strings.
pub struct VastEventTracker {
    external_url: String,
    account_id: String,
    auction_timestamp_ms: i64,
    integration_type: String,
}

impl VastEventTracker {
    pub fn new(
        external_url: String,
        account_id: String,
        auction_timestamp_ms: i64,
        integration_type: String,
    ) -> Self {
        Self {
            external_url,
            account_id,
            auction_timestamp_ms,
            integration_type,
        }
    }

    /// Injects an impression-tracking URL into the VAST XML.
    ///
    /// The tracker inserts an `<Impression>` element just before `</InLine>` or
    /// `</Wrapper>` in the VAST document (whichever comes first).
    ///
    /// Returns `Some(modified_xml)` on success, `None` if injection point not found.
    pub fn inject_tracking(&self, vast_xml: &str, bid_id: &str, bidder: &str) -> Option<String> {
        let tracking_url = create_tracking_url(
            &self.external_url,
            &EventRequest {
                event_type: EventType::Imp,
                bid_id: bid_id.to_string(),
                bidder: bidder.to_string(),
                account_id: self.account_id.clone(),
                timestamp: self.auction_timestamp_ms,
                integration: self.integration_type.clone(),
            },
        );

        let impression_tag = format!(
            "<Impression><![CDATA[{}]]></Impression>",
            tracking_url
        );

        // Try to insert before </InLine> first, then </Wrapper>
        for close_tag in &["</InLine>", "</Wrapper>"] {
            if let Some(pos) = vast_xml.find(close_tag) {
                let mut modified = String::with_capacity(vast_xml.len() + impression_tag.len());
                modified.push_str(&vast_xml[..pos]);
                modified.push_str(&impression_tag);
                modified.push_str(&vast_xml[pos..]);
                return Some(modified);
            }
        }

        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn default_tracking() -> EventTracking {
        EventTracking {
            account_id: "acct-123".to_string(),
            enabled_for_account: true,
            enabled_for_request: false,
            auction_timestamp_ms: 1700000000000,
            integration_type: "pbjs".to_string(),
            bidder_infos: HashMap::new(),
            external_url: "https://pbs.example.com".to_string(),
        }
    }

    #[test]
    fn test_create_tracking_url() {
        let req = EventRequest {
            event_type: EventType::Win,
            bid_id: "bid-1".to_string(),
            bidder: "appnexus".to_string(),
            account_id: "acct-1".to_string(),
            timestamp: 1700000000000,
            integration: "pbjs".to_string(),
        };
        let url = create_tracking_url("https://pbs.example.com", &req);
        assert!(url.starts_with("https://pbs.example.com/event?"));
        assert!(url.contains("t=win"));
        assert!(url.contains("b=bid-1"));
        assert!(url.contains("a=acct-1"));
        assert!(url.contains("bidder=appnexus"));
        assert!(url.contains("ts=1700000000000"));
        assert!(url.contains("int=pbjs"));
        assert!(url.contains("f=b"));
    }

    #[test]
    fn test_create_tracking_url_no_integration() {
        let req = EventRequest {
            event_type: EventType::Imp,
            bid_id: "bid-2".to_string(),
            bidder: "rubicon".to_string(),
            account_id: "acct-2".to_string(),
            timestamp: 1700000000000,
            integration: "".to_string(),
        };
        let url = create_tracking_url("https://pbs.example.com", &req);
        assert!(url.contains("t=imp"));
        assert!(!url.contains("int="));
    }

    #[test]
    fn test_is_event_allowed() {
        let mut et = default_tracking();
        assert!(et.is_event_allowed());

        et.enabled_for_account = false;
        et.enabled_for_request = false;
        assert!(!et.is_event_allowed());

        et.enabled_for_request = true;
        assert!(et.is_event_allowed());
    }

    #[test]
    fn test_is_modifying_vast_xml_allowed() {
        let mut et = default_tracking();

        // No bidder info -> not allowed
        assert!(!et.is_modifying_vast_xml_allowed("appnexus"));

        // Bidder info present but flag false
        et.bidder_infos.insert(
            "appnexus".to_string(),
            BidderEventInfo {
                modifying_vast_xml_allowed: false,
            },
        );
        assert!(!et.is_modifying_vast_xml_allowed("appnexus"));

        // Flag true
        et.bidder_infos.insert(
            "appnexus".to_string(),
            BidderEventInfo {
                modifying_vast_xml_allowed: true,
            },
        );
        assert!(et.is_modifying_vast_xml_allowed("appnexus"));

        // Events disabled -> not allowed even with flag
        et.enabled_for_account = false;
        et.enabled_for_request = false;
        assert!(!et.is_modifying_vast_xml_allowed("appnexus"));
    }

    #[test]
    fn test_make_bid_ext_events_banner() {
        let et = default_tracking();
        let result = et.make_bid_ext_events("bid-1", &BidType::Banner, "appnexus");
        assert!(result.is_some());
        let events = result.unwrap();
        assert!(events.win.is_some());
        assert!(events.imp.is_some());
        assert!(events.win.unwrap().contains("t=win"));
        assert!(events.imp.unwrap().contains("t=imp"));
    }

    #[test]
    fn test_make_bid_ext_events_video_returns_none() {
        let et = default_tracking();
        let result = et.make_bid_ext_events("bid-1", &BidType::Video, "appnexus");
        assert!(result.is_none());
    }

    #[test]
    fn test_make_bid_ext_events_disabled() {
        let mut et = default_tracking();
        et.enabled_for_account = false;
        et.enabled_for_request = false;
        let result = et.make_bid_ext_events("bid-1", &BidType::Banner, "appnexus");
        assert!(result.is_none());
    }

    #[test]
    fn test_inject_tracking_into_bid() {
        let et = default_tracking();
        let bid_json = br#"{"id":"bid-1","impid":"imp-1","price":1.5}"#;

        let result = et
            .inject_tracking_into_bid(&BidType::Banner, "bid-1", "appnexus", bid_json)
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&result).unwrap();
        assert!(parsed.get("wurl").is_some());
        let wurl = parsed["wurl"].as_str().unwrap();
        assert!(wurl.contains("t=win"));
    }

    #[test]
    fn test_inject_tracking_into_bid_video_unchanged() {
        let et = default_tracking();
        let bid_json = br#"{"id":"bid-1","impid":"imp-1","price":1.5}"#;

        let result = et
            .inject_tracking_into_bid(&BidType::Video, "bid-1", "appnexus", bid_json)
            .unwrap();
        assert_eq!(result, bid_json.to_vec());
    }

    #[test]
    fn test_vast_event_tracker_inject_inline() {
        let tracker = VastEventTracker::new(
            "https://pbs.example.com".to_string(),
            "acct-1".to_string(),
            1700000000000,
            "pbjs".to_string(),
        );

        let vast = r#"<VAST version="3.0"><Ad><InLine><Creatives></Creatives></InLine></Ad></VAST>"#;
        let result = tracker.inject_tracking(vast, "bid-1", "appnexus");
        assert!(result.is_some());
        let modified = result.unwrap();
        assert!(modified.contains("<Impression><![CDATA["));
        assert!(modified.contains("t=imp"));
        assert!(modified.contains("</Impression></InLine>"));
    }

    #[test]
    fn test_vast_event_tracker_inject_wrapper() {
        let tracker = VastEventTracker::new(
            "https://pbs.example.com".to_string(),
            "acct-1".to_string(),
            1700000000000,
            "".to_string(),
        );

        let vast = r#"<VAST version="3.0"><Ad><Wrapper><VASTAdTagURI>https://example.com</VASTAdTagURI></Wrapper></Ad></VAST>"#;
        let result = tracker.inject_tracking(vast, "bid-1", "rubicon");
        assert!(result.is_some());
        let modified = result.unwrap();
        assert!(modified.contains("</Impression></Wrapper>"));
    }

    #[test]
    fn test_vast_event_tracker_no_injection_point() {
        let tracker = VastEventTracker::new(
            "https://pbs.example.com".to_string(),
            "acct-1".to_string(),
            1700000000000,
            "".to_string(),
        );

        let vast = r#"<VAST version="3.0"><Ad></Ad></VAST>"#;
        let result = tracker.inject_tracking(vast, "bid-1", "rubicon");
        assert!(result.is_none());
    }

    #[test]
    fn test_urlencoding_special_chars() {
        assert_eq!(urlencoding("hello world"), "hello%20world");
        assert_eq!(urlencoding("a&b=c"), "a%26b%3Dc");
        assert_eq!(urlencoding("100%"), "100%25");
    }

    #[test]
    fn test_trailing_slash_in_external_url() {
        let req = EventRequest {
            event_type: EventType::Win,
            bid_id: "bid-1".to_string(),
            bidder: "appnexus".to_string(),
            account_id: "acct-1".to_string(),
            timestamp: 1000,
            integration: "".to_string(),
        };
        let url = create_tracking_url("https://pbs.example.com/", &req);
        // Should not have double slash
        assert!(url.starts_with("https://pbs.example.com/event?"));
    }
}
