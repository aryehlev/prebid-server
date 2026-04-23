//! Event URL generation and tracking injection.
//!
//! Mirrors the Go implementation in `exchange/events.go`.
//! Provides functionality to:
//! - Generate tracking URLs for win/impression/click events
//! - Inject event URLs into bid extensions
//! - Inject tracking pixels into VAST XML for video bids

use std::collections::HashMap;

use openrtb_ext::{BidType, ExtBidPrebidEvents, ExtRequestPrebid};
use pbs_config::{AccountConfig, BidderInfo};

use crate::analytics::EventType;
use crate::deals::{PbsOrtbBid, PbsOrtbSeatBid};

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
// Factory function
// ---------------------------------------------------------------------------

/// Creates an `EventTracking` from the various configuration sources.
///
/// Mirrors Go `getEventTracking`.
pub fn get_event_tracking(
    request_ext_prebid: Option<&ExtRequestPrebid>,
    timestamp_ms: i64,
    account: &AccountConfig,
    bidder_infos: &HashMap<String, BidderInfo>,
    external_url: &str,
) -> EventTracking {
    let enabled_for_request = request_ext_prebid
        .map(|p| p.events.is_some())
        .unwrap_or(false);

    let integration_type = request_ext_prebid
        .and_then(|p| p.integration.clone())
        .unwrap_or_default();

    let mut event_bidder_infos = HashMap::new();
    for (name, info) in bidder_infos {
        event_bidder_infos.insert(
            name.clone(),
            BidderEventInfo {
                modifying_vast_xml_allowed: info.modifying_vast_xml_allowed,
            },
        );
    }

    EventTracking {
        account_id: account.id.clone(),
        enabled_for_account: account.events.enabled,
        enabled_for_request,
        auction_timestamp_ms: timestamp_ms,
        integration_type,
        bidder_infos: event_bidder_infos,
        external_url: external_url.to_string(),
    }
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

    /// Iterates seat bids, injects event tracking into each bid.
    ///
    /// For video bids where VAST modification is allowed: injects an impression
    /// tracking pixel into the VAST XML.
    /// For all bids: populates `bid_events` with win/imp URLs.
    ///
    /// Mirrors Go `modifyBidsForEvents`.
    pub fn modify_bids_for_events(
        &self,
        seat_bids: &mut HashMap<String, PbsOrtbSeatBid>,
    ) {
        for (bidder_name, seat_bid) in seat_bids.iter_mut() {
            let modifying_vast_allowed = self.is_modifying_vast_xml_allowed(bidder_name);
            for pbs_bid in seat_bid.bids.iter_mut() {
                if modifying_vast_allowed {
                    self.modify_bid_vast(pbs_bid, bidder_name);
                }
                let effective_bid_id = self.effective_bid_id(pbs_bid);
                pbs_bid.bid_events =
                    self.make_bid_ext_events(&effective_bid_id, &pbs_bid.bid_type, bidder_name);
            }
        }
    }

    /// Injects a win URL ("wurl") into the bid JSON for non-video bids.
    ///
    /// If the bid already has pre-computed `bid_events`, uses the win URL from
    /// there; otherwise computes it on the fly.
    ///
    /// Mirrors Go `modifyBidJSON`.
    pub fn modify_bid_json(
        &self,
        pbs_bid: &PbsOrtbBid,
        bidder: &str,
        json_bytes: &[u8],
    ) -> Result<Vec<u8>, serde_json::Error> {
        if !self.is_event_allowed() || pbs_bid.bid_type == BidType::Video {
            return Ok(json_bytes.to_vec());
        }

        let win_url = if let Some(ref events) = pbs_bid.bid_events {
            events.win.clone().unwrap_or_default()
        } else {
            let effective_id = self.effective_bid_id(pbs_bid);
            self.make_event_url(EventType::Win, &effective_id, bidder)
        };

        let mut value: serde_json::Value = serde_json::from_slice(json_bytes)?;
        if let Some(obj) = value.as_object_mut() {
            obj.insert("wurl".to_string(), serde_json::Value::String(win_url));
        }
        serde_json::to_vec(&value)
    }

    /// Injects an event impression URL into the VAST XML of a video bid.
    ///
    /// Mirrors Go `modifyBidVAST`.
    fn modify_bid_vast(&self, pbs_bid: &mut PbsOrtbBid, bidder_name: &str) {
        if pbs_bid.bid_type != BidType::Video {
            return;
        }
        let has_adm = pbs_bid.bid.adm.as_ref().map_or(false, |a| !a.is_empty());
        let has_nurl = pbs_bid.bid.nurl.as_ref().map_or(false, |n| !n.is_empty());
        if !has_adm && !has_nurl {
            return;
        }

        let adm = pbs_bid.bid.adm.as_deref().unwrap_or("");
        let nurl = pbs_bid.bid.nurl.as_deref().unwrap_or("");
        let vast_xml = crate::vast::make_vast(adm, nurl);

        let effective_id = self.effective_bid_id(pbs_bid);

        let tracker = VastEventTracker::new(
            self.external_url.clone(),
            self.account_id.clone(),
            self.auction_timestamp_ms,
            self.integration_type.clone(),
        );

        if let Some(new_vast) = tracker.inject_tracking(&vast_xml, &effective_id, bidder_name) {
            pbs_bid.bid.adm = Some(new_vast);
        }
    }

    /// Returns the effective bid ID: `generated_bid_id` if non-empty, else `bid.id`.
    fn effective_bid_id(&self, pbs_bid: &PbsOrtbBid) -> String {
        if !pbs_bid.generated_bid_id.is_empty() {
            pbs_bid.generated_bid_id.clone()
        } else {
            pbs_bid.bid.id.clone()
        }
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

    // -----------------------------------------------------------------------
    // get_event_tracking tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_event_tracking_basic() {
        let account = AccountConfig {
            id: "pub-123".to_string(),
            events: pbs_config::AccountEventsConfig {
                enabled: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bidder_infos = HashMap::new();
        bidder_infos.insert(
            "appnexus".to_string(),
            BidderInfo {
                modifying_vast_xml_allowed: true,
                ..Default::default()
            },
        );
        let prebid = ExtRequestPrebid {
            events: Some(serde_json::json!({})),
            integration: Some("pbjs".to_string()),
            ..Default::default()
        };

        let et = get_event_tracking(
            Some(&prebid),
            1700000000000,
            &account,
            &bidder_infos,
            "https://pbs.example.com",
        );

        assert_eq!(et.account_id, "pub-123");
        assert!(et.enabled_for_account);
        assert!(et.enabled_for_request);
        assert_eq!(et.auction_timestamp_ms, 1700000000000);
        assert_eq!(et.integration_type, "pbjs");
        assert!(et.is_modifying_vast_xml_allowed("appnexus"));
    }

    #[test]
    fn test_get_event_tracking_no_prebid_ext() {
        let account = AccountConfig {
            id: "pub-456".to_string(),
            ..Default::default()
        };
        let bidder_infos = HashMap::new();

        let et = get_event_tracking(None, 1000, &account, &bidder_infos, "https://pbs.example.com");

        assert!(!et.enabled_for_account);
        assert!(!et.enabled_for_request);
        assert!(et.integration_type.is_empty());
    }

    // -----------------------------------------------------------------------
    // modify_bids_for_events tests
    // -----------------------------------------------------------------------

    fn make_pbs_bid(id: &str, bid_type: BidType, adm: Option<&str>) -> PbsOrtbBid {
        PbsOrtbBid {
            bid: openrtb::Bid {
                id: id.to_string(),
                impid: "imp-1".to_string(),
                adm: adm.map(|s| s.to_string()),
                ..Default::default()
            },
            bid_type,
            ..Default::default()
        }
    }

    #[test]
    fn test_modify_bids_for_events_banner() {
        let et = default_tracking();
        let mut seat_bids = HashMap::new();
        seat_bids.insert(
            "appnexus".to_string(),
            PbsOrtbSeatBid {
                bids: vec![make_pbs_bid("bid-1", BidType::Banner, None)],
                seat: "appnexus".to_string(),
                ..Default::default()
            },
        );

        et.modify_bids_for_events(&mut seat_bids);

        let bid = &seat_bids["appnexus"].bids[0];
        assert!(bid.bid_events.is_some());
        let events = bid.bid_events.as_ref().unwrap();
        assert!(events.win.as_ref().unwrap().contains("t=win"));
        assert!(events.imp.as_ref().unwrap().contains("t=imp"));
    }

    #[test]
    fn test_modify_bids_for_events_video_no_vast_mod() {
        let et = default_tracking();
        // No bidder info -> VAST modification not allowed
        let mut seat_bids = HashMap::new();
        let vast = r#"<VAST version="3.0"><Ad><InLine><Creatives></Creatives></InLine></Ad></VAST>"#;
        seat_bids.insert(
            "appnexus".to_string(),
            PbsOrtbSeatBid {
                bids: vec![make_pbs_bid("bid-1", BidType::Video, Some(vast))],
                seat: "appnexus".to_string(),
                ..Default::default()
            },
        );

        et.modify_bids_for_events(&mut seat_bids);

        let bid = &seat_bids["appnexus"].bids[0];
        // Video bids get no bid_events
        assert!(bid.bid_events.is_none());
        // VAST not modified (no bidder info allowing it)
        assert_eq!(bid.bid.adm.as_deref().unwrap(), vast);
    }

    #[test]
    fn test_modify_bids_for_events_video_with_vast_mod() {
        let mut et = default_tracking();
        et.bidder_infos.insert(
            "appnexus".to_string(),
            BidderEventInfo {
                modifying_vast_xml_allowed: true,
            },
        );
        let vast = r#"<VAST version="3.0"><Ad><InLine><Creatives></Creatives></InLine></Ad></VAST>"#;
        let mut seat_bids = HashMap::new();
        seat_bids.insert(
            "appnexus".to_string(),
            PbsOrtbSeatBid {
                bids: vec![make_pbs_bid("bid-1", BidType::Video, Some(vast))],
                seat: "appnexus".to_string(),
                ..Default::default()
            },
        );

        et.modify_bids_for_events(&mut seat_bids);

        let bid = &seat_bids["appnexus"].bids[0];
        // Video bids get no bid_events (skipped for video)
        assert!(bid.bid_events.is_none());
        // But VAST should be modified with impression tracker
        let modified_adm = bid.bid.adm.as_ref().unwrap();
        assert!(modified_adm.contains("<Impression><![CDATA["));
        assert!(modified_adm.contains("t=imp"));
    }

    #[test]
    fn test_modify_bids_for_events_uses_generated_bid_id() {
        let et = default_tracking();
        let mut seat_bids = HashMap::new();
        let mut bid = make_pbs_bid("original-id", BidType::Banner, None);
        bid.generated_bid_id = "generated-id".to_string();
        seat_bids.insert(
            "appnexus".to_string(),
            PbsOrtbSeatBid {
                bids: vec![bid],
                seat: "appnexus".to_string(),
                ..Default::default()
            },
        );

        et.modify_bids_for_events(&mut seat_bids);

        let events = seat_bids["appnexus"].bids[0].bid_events.as_ref().unwrap();
        // URLs should contain the generated bid ID, not the original
        assert!(events.win.as_ref().unwrap().contains("b=generated-id"));
        assert!(events.imp.as_ref().unwrap().contains("b=generated-id"));
    }

    // -----------------------------------------------------------------------
    // modify_bid_json tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_modify_bid_json_banner() {
        let et = default_tracking();
        let bid = make_pbs_bid("bid-1", BidType::Banner, None);
        let json_bytes = br#"{"id":"bid-1","impid":"imp-1","price":1.5}"#;

        let result = et.modify_bid_json(&bid, "appnexus", json_bytes).unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&result).unwrap();
        assert!(parsed.get("wurl").is_some());
        assert!(parsed["wurl"].as_str().unwrap().contains("t=win"));
    }

    #[test]
    fn test_modify_bid_json_video_unchanged() {
        let et = default_tracking();
        let bid = make_pbs_bid("bid-1", BidType::Video, None);
        let json_bytes = br#"{"id":"bid-1","impid":"imp-1","price":1.5}"#;

        let result = et.modify_bid_json(&bid, "appnexus", json_bytes).unwrap();
        assert_eq!(result, json_bytes.to_vec());
    }

    #[test]
    fn test_modify_bid_json_uses_precomputed_events() {
        let et = default_tracking();
        let mut bid = make_pbs_bid("bid-1", BidType::Banner, None);
        bid.bid_events = Some(ExtBidPrebidEvents {
            win: Some("https://precomputed.win.url".to_string()),
            imp: Some("https://precomputed.imp.url".to_string()),
        });
        let json_bytes = br#"{"id":"bid-1","impid":"imp-1","price":1.5}"#;

        let result = et.modify_bid_json(&bid, "appnexus", json_bytes).unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(parsed["wurl"].as_str().unwrap(), "https://precomputed.win.url");
    }

    #[test]
    fn test_modify_bid_json_events_disabled() {
        let mut et = default_tracking();
        et.enabled_for_account = false;
        et.enabled_for_request = false;
        let bid = make_pbs_bid("bid-1", BidType::Banner, None);
        let json_bytes = br#"{"id":"bid-1","impid":"imp-1","price":1.5}"#;

        let result = et.modify_bid_json(&bid, "appnexus", json_bytes).unwrap();
        assert_eq!(result, json_bytes.to_vec());
    }

    // -----------------------------------------------------------------------
    // effective_bid_id tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_effective_bid_id_uses_generated() {
        let et = default_tracking();
        let mut bid = make_pbs_bid("original", BidType::Banner, None);
        bid.generated_bid_id = "generated".to_string();
        assert_eq!(et.effective_bid_id(&bid), "generated");
    }

    #[test]
    fn test_effective_bid_id_falls_back_to_bid_id() {
        let et = default_tracking();
        let bid = make_pbs_bid("original", BidType::Banner, None);
        assert_eq!(et.effective_bid_id(&bid), "original");
    }
}
