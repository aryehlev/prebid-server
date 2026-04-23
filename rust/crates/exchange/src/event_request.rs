//! Event request parsing and URL construction.
//!
//! Mirrors Go `endpoints/events/event.go`.
//!
//! Handles parsing of event tracking requests and construction of
//! event notification URLs for win/impression/click tracking.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// URL template for event tracking endpoints.
pub const TEMPLATE_URL: &str = "{host}/event?t={type}&b={bid_id}&a={account_id}";

/// Query parameter names.
pub const TYPE_PARAM: &str = "t";
pub const VTYPE_PARAM: &str = "vtype";
pub const BID_ID_PARAM: &str = "b";
pub const ACCOUNT_ID_PARAM: &str = "a";
pub const BIDDER_PARAM: &str = "bidder";
pub const TIMESTAMP_PARAM: &str = "ts";
pub const FORMAT_PARAM: &str = "f";
pub const ANALYTICS_PARAM: &str = "x";
pub const INTEGRATION_TYPE_PARAM: &str = "int";

// ---------------------------------------------------------------------------
// EventType
// ---------------------------------------------------------------------------

/// Types of tracking events.
///
/// Mirrors Go `analytics.EventType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    Win,
    Imp,
    Click,
}

impl EventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::Win => "win",
            EventType::Imp => "imp",
            EventType::Click => "click",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "win" => Some(EventType::Win),
            "imp" => Some(EventType::Imp),
            "click" => Some(EventType::Click),
            _ => None,
        }
    }
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// EventRequest
// ---------------------------------------------------------------------------

/// Parsed event tracking request.
///
/// Mirrors Go `analytics.EventRequest`.
#[derive(Debug, Clone)]
pub struct EventRequest {
    /// Event type (win, imp, click).
    pub event_type: EventType,
    /// Bid ID.
    pub bid_id: String,
    /// Account ID.
    pub account_id: String,
    /// Bidder name.
    pub bidder: String,
    /// Auction timestamp in milliseconds.
    pub timestamp: i64,
    /// Response format: "b" (blank), "i" (pixel image).
    pub format: String,
    /// Analytics reporting: "1" to enable.
    pub analytics: String,
    /// Integration type (e.g., "pbjs", "amp").
    pub integration_type: String,
    /// Video event type (for VAST tracking).
    pub vtype: String,
}

impl Default for EventRequest {
    fn default() -> Self {
        Self {
            event_type: EventType::Win,
            bid_id: String::new(),
            account_id: String::new(),
            bidder: String::new(),
            timestamp: 0,
            format: String::new(),
            analytics: String::new(),
            integration_type: String::new(),
            vtype: String::new(),
        }
    }
}

/// Parse an event request from query parameters.
///
/// Mirrors Go `ParseEventRequest`.
pub fn parse_event_request(params: &HashMap<String, String>) -> Result<EventRequest, Vec<String>> {
    let mut errors = Vec::new();

    let event_type = match params.get(TYPE_PARAM).and_then(|t| EventType::from_str(t)) {
        Some(t) => t,
        None => {
            errors.push(format!(
                "parameter '{}' is required and must be one of: win, imp, click",
                TYPE_PARAM
            ));
            EventType::Win // default to avoid early return
        }
    };

    let bid_id = params.get(BID_ID_PARAM).cloned().unwrap_or_default();
    if bid_id.is_empty() {
        errors.push(format!("parameter '{}' is required", BID_ID_PARAM));
    }

    let account_id = params.get(ACCOUNT_ID_PARAM).cloned().unwrap_or_default();
    if account_id.is_empty() {
        errors.push(format!("parameter '{}' is required", ACCOUNT_ID_PARAM));
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(EventRequest {
        event_type,
        bid_id,
        account_id,
        bidder: params.get(BIDDER_PARAM).cloned().unwrap_or_default(),
        timestamp: params
            .get(TIMESTAMP_PARAM)
            .and_then(|t| t.parse().ok())
            .unwrap_or(0),
        format: params.get(FORMAT_PARAM).cloned().unwrap_or_default(),
        analytics: params.get(ANALYTICS_PARAM).cloned().unwrap_or_default(),
        integration_type: params.get(INTEGRATION_TYPE_PARAM).cloned().unwrap_or_default(),
        vtype: params.get(VTYPE_PARAM).cloned().unwrap_or_default(),
    })
}

/// Construct an event tracking URL from a request and external URL.
///
/// Mirrors Go `EventRequestToUrl`.
pub fn event_request_to_url(external_url: &str, request: &EventRequest) -> String {
    let mut url = format!(
        "{}/event?t={}&b={}&a={}",
        external_url.trim_end_matches('/'),
        request.event_type.as_str(),
        url_encode(&request.bid_id),
        url_encode(&request.account_id),
    );

    if !request.bidder.is_empty() {
        url.push_str(&format!("&bidder={}", url_encode(&request.bidder)));
    }
    if request.timestamp > 0 {
        url.push_str(&format!("&ts={}", request.timestamp));
    }
    if !request.format.is_empty() {
        url.push_str(&format!("&f={}", url_encode(&request.format)));
    }
    if !request.analytics.is_empty() {
        url.push_str(&format!("&x={}", url_encode(&request.analytics)));
    }
    if !request.integration_type.is_empty() {
        url.push_str(&format!("&int={}", url_encode(&request.integration_type)));
    }

    url
}

/// Simple URL encoding for path/query components.
fn url_encode(s: &str) -> String {
    s.replace('%', "%25")
        .replace(' ', "%20")
        .replace('&', "%26")
        .replace('=', "%3D")
        .replace('+', "%2B")
        .replace('#', "%23")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_from_str() {
        assert_eq!(EventType::from_str("win"), Some(EventType::Win));
        assert_eq!(EventType::from_str("imp"), Some(EventType::Imp));
        assert_eq!(EventType::from_str("click"), Some(EventType::Click));
        assert_eq!(EventType::from_str("unknown"), None);
    }

    #[test]
    fn test_event_type_display() {
        assert_eq!(EventType::Win.to_string(), "win");
        assert_eq!(EventType::Imp.to_string(), "imp");
        assert_eq!(EventType::Click.to_string(), "click");
    }

    #[test]
    fn test_parse_event_request_valid() {
        let mut params = HashMap::new();
        params.insert("t".to_string(), "win".to_string());
        params.insert("b".to_string(), "bid-123".to_string());
        params.insert("a".to_string(), "acct-456".to_string());
        params.insert("bidder".to_string(), "appnexus".to_string());
        params.insert("ts".to_string(), "1234567890".to_string());

        let req = parse_event_request(&params).unwrap();
        assert_eq!(req.event_type, EventType::Win);
        assert_eq!(req.bid_id, "bid-123");
        assert_eq!(req.account_id, "acct-456");
        assert_eq!(req.bidder, "appnexus");
        assert_eq!(req.timestamp, 1234567890);
    }

    #[test]
    fn test_parse_event_request_missing_type() {
        let mut params = HashMap::new();
        params.insert("b".to_string(), "bid-123".to_string());
        params.insert("a".to_string(), "acct-456".to_string());

        let err = parse_event_request(&params).unwrap_err();
        assert!(err[0].contains("parameter 't'"));
    }

    #[test]
    fn test_parse_event_request_missing_bid_id() {
        let mut params = HashMap::new();
        params.insert("t".to_string(), "win".to_string());
        params.insert("a".to_string(), "acct-456".to_string());

        let err = parse_event_request(&params).unwrap_err();
        assert!(err.iter().any(|e| e.contains("parameter 'b'")));
    }

    #[test]
    fn test_event_request_to_url() {
        let req = EventRequest {
            event_type: EventType::Win,
            bid_id: "bid-123".to_string(),
            account_id: "acct-456".to_string(),
            bidder: "appnexus".to_string(),
            timestamp: 1234567890,
            format: "b".to_string(),
            analytics: "1".to_string(),
            integration_type: "pbjs".to_string(),
            vtype: String::new(),
        };

        let url = event_request_to_url("https://prebid.example.com", &req);
        assert!(url.starts_with("https://prebid.example.com/event?"));
        assert!(url.contains("t=win"));
        assert!(url.contains("b=bid-123"));
        assert!(url.contains("a=acct-456"));
        assert!(url.contains("bidder=appnexus"));
        assert!(url.contains("ts=1234567890"));
        assert!(url.contains("f=b"));
        assert!(url.contains("x=1"));
        assert!(url.contains("int=pbjs"));
    }

    #[test]
    fn test_event_request_to_url_minimal() {
        let req = EventRequest {
            event_type: EventType::Imp,
            bid_id: "bid-1".to_string(),
            account_id: "acct-1".to_string(),
            ..Default::default()
        };

        let url = event_request_to_url("https://example.com", &req);
        assert_eq!(url, "https://example.com/event?t=imp&b=bid-1&a=acct-1");
    }

    #[test]
    fn test_event_request_to_url_trailing_slash() {
        let req = EventRequest {
            event_type: EventType::Win,
            bid_id: "b1".to_string(),
            account_id: "a1".to_string(),
            ..Default::default()
        };

        let url = event_request_to_url("https://example.com/", &req);
        assert!(url.starts_with("https://example.com/event?"));
    }

    #[test]
    fn test_url_encode() {
        assert_eq!(url_encode("hello world"), "hello%20world");
        assert_eq!(url_encode("a&b=c"), "a%26b%3Dc");
    }
}
