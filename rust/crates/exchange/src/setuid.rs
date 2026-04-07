use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Minimum Chrome version that supports the SameSite cookie attribute.
pub const CHROME_MIN_VER: u32 = 67;

/// Name of the UID cookie.
pub const UID_COOKIE_NAME: &str = "uids";

/// Chrome user-agent prefix string.
const CHROME_STR: &str = "Chrome/";

/// Chrome iOS user-agent prefix string.
const CHROME_IOS_STR: &str = "CriOS/";

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Parsed `/setuid` request, extracted from query parameters.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SetUidRequest {
    /// Bidder name (required).
    pub bidder: String,

    /// UID value to set; empty string means "unsync" (delete).
    #[serde(default)]
    pub uid: String,

    /// GDPR signal: 0 = no, 1 = yes, absent = ambiguous.
    pub gdpr: Option<i32>,

    /// TCF consent string.
    #[serde(default)]
    pub gdpr_consent: String,

    /// Publisher account ID.
    #[serde(default)]
    pub account: String,

    /// Response format: `"i"` for pixel (image), `"b"` for redirect (blank HTML).
    #[serde(default)]
    pub format: String,
}

/// The outcome of processing a `/setuid` request.
#[derive(Debug, Clone, PartialEq)]
pub enum SetUidResponse {
    /// UID was set (or cleared) successfully.
    Success,
    /// User has opted out of syncing.
    OptOut,
    /// The request is malformed.
    BadRequest(String),
    /// Blocked by privacy regulation (GDPR, activity controls, etc.).
    Unauthorized,
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

/// Parse a [`SetUidRequest`] from query parameters.
///
/// Required parameters: `bidder`.
/// Optional parameters: `uid`, `gdpr`, `gdpr_consent`, `account`, `f` (format).
pub fn parse_setuid_request(
    query_params: &HashMap<String, String>,
) -> Result<SetUidRequest, String> {
    let bidder = query_params
        .get("bidder")
        .filter(|v| !v.is_empty())
        .ok_or_else(|| r#""bidder" query param is required"#.to_string())?
        .clone();

    let uid = query_params
        .get("uid")
        .cloned()
        .unwrap_or_default();

    let gdpr = query_params
        .get("gdpr")
        .and_then(|v| {
            if v.is_empty() {
                None
            } else {
                Some(v.parse::<i32>().map_err(|_| {
                    format!(r#"invalid gdpr value "{v}", must be 0 or 1"#)
                }))
            }
        })
        .transpose()?;

    let gdpr_consent = query_params
        .get("gdpr_consent")
        .cloned()
        .unwrap_or_default();

    let account = query_params
        .get("account")
        .cloned()
        .unwrap_or_default();

    let format = query_params
        .get("f")
        .cloned()
        .unwrap_or_default();

    Ok(SetUidRequest {
        bidder,
        uid,
        gdpr,
        gdpr_consent,
        account,
        format,
    })
}

/// Determine the response format to use.
///
/// Priority:
///   1. Explicit `f` query parameter (`"i"` = pixel, `"b"` = blank HTML).
///   2. `default_format` provided by the syncer configuration.
///
/// Returns an error if the value is present but not `"i"` or `"b"`.
pub fn get_response_format(
    query: &HashMap<String, String>,
    default_format: &str,
) -> Result<String, String> {
    let raw = query.get("f").map(|s| s.as_str()).unwrap_or("");

    if raw.is_empty() {
        return Ok(default_format.to_string());
    }

    let lower = raw.to_lowercase();
    if lower == "b" || lower == "i" {
        Ok(lower)
    } else {
        Err(r#""f" query param is invalid. must be "b" or "i""#.to_string())
    }
}

/// Resolve which syncer key to use from the `bidder` query parameter.
///
/// Returns `(syncer_key, bidder_name)` on success, or an error if the bidder
/// is missing / unknown.
pub fn get_syncer(
    query: &HashMap<String, String>,
    syncers: &HashMap<String, String>,
) -> Result<(String, String), String> {
    let bidder = query
        .get("bidder")
        .filter(|v| !v.is_empty())
        .ok_or_else(|| r#""bidder" query param is required"#.to_string())?;

    // Case-insensitive lookup: try the raw value first, then lowercased
    // against lowercased keys.
    if let Some(syncer_key) = syncers.get(bidder.as_str()) {
        return Ok((syncer_key.clone(), bidder.clone()));
    }

    let bidder_lower = bidder.to_lowercase();
    for (key, syncer_key) in syncers {
        if key.to_lowercase() == bidder_lower {
            return Ok((syncer_key.clone(), bidder.clone()));
        }
    }

    Err("The bidder name provided is not supported by Prebid Server".to_string())
}

/// Check whether a Chrome-based browser version is high enough to support the
/// `SameSite` cookie attribute.  Returns `true` if the `SameSite=None;Secure`
/// attribute should be set.
pub fn site_cookie_check(user_agent: &str) -> bool {
    if let Some(idx) = user_agent.find(CHROME_STR) {
        return check_chrome_browser_version(user_agent, idx + CHROME_STR.len());
    }
    if let Some(idx) = user_agent.find(CHROME_IOS_STR) {
        return check_chrome_browser_version(user_agent, idx + CHROME_IOS_STR.len());
    }
    false
}

/// Extract the major Chrome version from the UA string starting at `start` and
/// compare against [`CHROME_MIN_VER`].
fn check_chrome_browser_version(ua: &str, start: usize) -> bool {
    let rest = &ua[start..];
    let end = rest.find('.').unwrap_or(rest.len());
    rest[..end].parse::<u32>().map_or(false, |v| v >= CHROME_MIN_VER)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_query(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // -- parse_setuid_request -----------------------------------------------

    #[test]
    fn test_parse_setuid_request_missing_bidder() {
        let q = make_query(&[]);
        let err = parse_setuid_request(&q).unwrap_err();
        assert!(err.contains("bidder"));
    }

    #[test]
    fn test_parse_setuid_request_empty_bidder() {
        let q = make_query(&[("bidder", "")]);
        assert!(parse_setuid_request(&q).is_err());
    }

    #[test]
    fn test_parse_setuid_request_minimal() {
        let q = make_query(&[("bidder", "appnexus")]);
        let req = parse_setuid_request(&q).unwrap();
        assert_eq!(req.bidder, "appnexus");
        assert!(req.uid.is_empty());
        assert_eq!(req.gdpr, None);
    }

    #[test]
    fn test_parse_setuid_request_full() {
        let q = make_query(&[
            ("bidder", "rubicon"),
            ("uid", "abc123"),
            ("gdpr", "1"),
            ("gdpr_consent", "CONSENT"),
            ("account", "pub456"),
            ("f", "i"),
        ]);
        let req = parse_setuid_request(&q).unwrap();
        assert_eq!(req.bidder, "rubicon");
        assert_eq!(req.uid, "abc123");
        assert_eq!(req.gdpr, Some(1));
        assert_eq!(req.gdpr_consent, "CONSENT");
        assert_eq!(req.account, "pub456");
        assert_eq!(req.format, "i");
    }

    #[test]
    fn test_parse_setuid_request_invalid_gdpr() {
        let q = make_query(&[("bidder", "appnexus"), ("gdpr", "maybe")]);
        let err = parse_setuid_request(&q).unwrap_err();
        assert!(err.contains("invalid gdpr value"));
    }

    // -- get_response_format ------------------------------------------------

    #[test]
    fn test_get_response_format_explicit_b() {
        let q = make_query(&[("f", "b")]);
        assert_eq!(get_response_format(&q, "").unwrap(), "b");
    }

    #[test]
    fn test_get_response_format_explicit_i() {
        let q = make_query(&[("f", "I")]);
        assert_eq!(get_response_format(&q, "").unwrap(), "i");
    }

    #[test]
    fn test_get_response_format_invalid() {
        let q = make_query(&[("f", "x")]);
        assert!(get_response_format(&q, "").is_err());
    }

    #[test]
    fn test_get_response_format_fallback_default() {
        let q = make_query(&[]);
        assert_eq!(get_response_format(&q, "b").unwrap(), "b");
    }

    #[test]
    fn test_get_response_format_empty_uses_default() {
        let q = make_query(&[("f", "")]);
        assert_eq!(get_response_format(&q, "i").unwrap(), "i");
    }

    // -- get_syncer ---------------------------------------------------------

    #[test]
    fn test_get_syncer_found() {
        let syncers: HashMap<String, String> = [("appnexus".to_string(), "appnexus-key".to_string())]
            .into_iter()
            .collect();
        let q = make_query(&[("bidder", "appnexus")]);
        let (key, bidder) = get_syncer(&q, &syncers).unwrap();
        assert_eq!(key, "appnexus-key");
        assert_eq!(bidder, "appnexus");
    }

    #[test]
    fn test_get_syncer_case_insensitive() {
        let syncers: HashMap<String, String> =
            [("AppNexus".to_string(), "appnexus-key".to_string())]
                .into_iter()
                .collect();
        let q = make_query(&[("bidder", "appnexus")]);
        let (key, _) = get_syncer(&q, &syncers).unwrap();
        assert_eq!(key, "appnexus-key");
    }

    #[test]
    fn test_get_syncer_not_found() {
        let syncers: HashMap<String, String> = HashMap::new();
        let q = make_query(&[("bidder", "unknown")]);
        let err = get_syncer(&q, &syncers).unwrap_err();
        assert!(err.contains("not supported"));
    }

    #[test]
    fn test_get_syncer_missing_bidder() {
        let syncers: HashMap<String, String> = HashMap::new();
        let q = make_query(&[]);
        assert!(get_syncer(&q, &syncers).is_err());
    }

    // -- site_cookie_check --------------------------------------------------

    #[test]
    fn test_site_cookie_check_chrome_above_min() {
        assert!(site_cookie_check(
            "Mozilla/5.0 (Windows) Chrome/80.0.3987.132 Safari/537.36"
        ));
    }

    #[test]
    fn test_site_cookie_check_chrome_below_min() {
        assert!(!site_cookie_check(
            "Mozilla/5.0 (Windows) Chrome/50.0.2661.102 Safari/537.36"
        ));
    }

    #[test]
    fn test_site_cookie_check_chrome_exact_min() {
        assert!(site_cookie_check(
            "Mozilla/5.0 (Windows) Chrome/67.0.3396.99 Safari/537.36"
        ));
    }

    #[test]
    fn test_site_cookie_check_crios() {
        assert!(site_cookie_check(
            "Mozilla/5.0 (iPhone) CriOS/80.0.3987.95 Mobile Safari/604.1"
        ));
    }

    #[test]
    fn test_site_cookie_check_no_chrome() {
        assert!(!site_cookie_check(
            "Mozilla/5.0 (Windows) Firefox/74.0"
        ));
    }
}
