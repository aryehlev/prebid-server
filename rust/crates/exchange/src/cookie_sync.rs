use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::usersync::{PrebidCookie, SyncerChoice};

// ---------------------------------------------------------------------------
// Error constants
// ---------------------------------------------------------------------------

pub const ERR_COOKIE_SYNC_OPT_OUT: &str = "User has opted out";
pub const ERR_COOKIE_SYNC_BODY: &str = "Failed to read request body";
pub const ERR_COOKIE_SYNC_GDPR_CONSENT_MISSING: &str = "gdpr_consent is required if gdpr=1";
pub const ERR_COOKIE_SYNC_GDPR_CONSENT_MISSING_SIGNAL_AMBIGUOUS: &str =
    "gdpr_consent is required. gdpr is not specified and is assumed to be 1 by the server. set gdpr=0 to exempt this request";
pub const ERR_COOKIE_SYNC_INVALID_BIDDERS_TYPE: &str =
    "invalid bidders type. must either be a string '*' or a string array of bidders";
pub const ERR_COOKIE_SYNC_ACCOUNT_BLOCKED: &str =
    "account is disabled, please reach out to the prebid server host";
pub const ERR_COOKIE_SYNC_ACCOUNT_CONFIG_MALFORMED: &str =
    "account config is malformed and could not be read";
pub const ERR_COOKIE_SYNC_ACCOUNT_INVALID: &str =
    "account must be valid if provided, please reach out to the prebid server host";

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Parsed cookie sync request body. Mirrors the Go `cookieSyncRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CookieSyncRequest {
    /// List of bidder names to sync, or `["*"]` for all.
    #[serde(default)]
    pub bidders: Vec<String>,

    /// GDPR signal: 0 = no, 1 = yes, absent = ambiguous.
    pub gdpr: Option<i32>,

    /// TCF consent string.
    #[serde(default)]
    pub gdpr_consent: String,

    /// US Privacy / CCPA consent string.
    #[serde(default)]
    pub us_privacy: String,

    /// Maximum number of syncs to return.
    pub limit: Option<i32>,

    /// Whether cooperative (server-initiated) syncing is enabled for this
    /// request.  `None` means "use server default".
    #[serde(rename = "coopSync")]
    pub cooperative_sync: Option<bool>,

    /// Filter settings for allowed sync types.
    pub filter_settings: Option<FilterSettings>,

    /// Publisher account ID.
    #[serde(default)]
    pub account: String,

    /// Debug flag.
    #[serde(default)]
    pub debug: bool,
}

/// Filter settings control which sync types (iframe / redirect) are allowed.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilterSettings {
    pub iframe: Option<FilterConfig>,
    pub image: Option<FilterConfig>,
}

/// A single filter entry: a mode (`include` or `exclude`) and an optional set
/// of bidders.  `bidders: ["*"]` means "all bidders".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterConfig {
    #[serde(default)]
    pub bidders: FilterBidders,
    #[serde(default = "default_filter_mode")]
    pub mode: FilterMode,
}

fn default_filter_mode() -> FilterMode {
    FilterMode::Include
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FilterMode {
    Include,
    Exclude,
}

impl Default for FilterMode {
    fn default() -> Self {
        FilterMode::Include
    }
}

/// Bidders in a filter can be either the wildcard `"*"` (all) or a list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum FilterBidders {
    All(String),       // "*"
    List(Vec<String>), // ["appnexus", "rubicon"]
}

impl Default for FilterBidders {
    fn default() -> Self {
        FilterBidders::All("*".to_string())
    }
}

/// Cookie sync response returned to the caller.
#[derive(Debug, Clone, Serialize)]
pub struct CookieSyncResponse {
    pub status: CookieSyncStatus,
    #[serde(rename = "bidder_status")]
    pub bidder_status: Vec<BidderSyncStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CookieSyncStatus {
    Ok,
    #[serde(rename = "no_cookie")]
    NoCookie,
}

#[derive(Debug, Clone, Serialize)]
pub struct BidderSyncStatus {
    pub bidder: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usersync: Option<UsersyncInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub no_cookie: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsersyncInfo {
    pub url: String,
    #[serde(rename = "type")]
    pub sync_type: String,
    #[serde(rename = "supportCORS")]
    pub support_cors: bool,
}

// ---------------------------------------------------------------------------
// Privacy config (subset relevant for cookie-sync decisions)
// ---------------------------------------------------------------------------

/// Holds the privacy-related configuration used when evaluating a cookie sync
/// request.  This is the Rust equivalent of the Go `usersyncPrivacyConfig`.
#[derive(Debug, Clone)]
pub struct UsersyncPrivacyConfig {
    /// Default GDPR signal when the request does not specify one.
    pub gdpr_default_value: String,
    /// Whether CCPA should be enforced.
    pub ccpa_enforce: bool,
    /// Set of known bidder names (for CCPA scope checks).
    pub bidder_hash_set: HashSet<String>,
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

/// Parse a JSON request body into a [`CookieSyncRequest`].
pub fn parse_cookie_sync_request(body: &[u8]) -> Result<CookieSyncRequest, String> {
    if body.is_empty() {
        return Err(ERR_COOKIE_SYNC_BODY.to_string());
    }
    serde_json::from_slice(body).map_err(|e| format!("JSON parsing failed: {e}"))
}

/// Validate that the GDPR consent string is present when required.
///
/// Rules (matching the Go implementation):
///   - `gdpr == Some(1)` and `gdpr_consent` is empty  -> error
///   - `gdpr == None` (ambiguous) and `gdpr_consent` is empty -> error
///     (with a different message)
///   - otherwise -> Ok
pub fn validate_gdpr_consent(req: &CookieSyncRequest) -> Result<(), String> {
    match req.gdpr {
        Some(1) => {
            if req.gdpr_consent.is_empty() {
                Err(ERR_COOKIE_SYNC_GDPR_CONSENT_MISSING.to_string())
            } else {
                Ok(())
            }
        }
        None => {
            if req.gdpr_consent.is_empty() {
                Err(ERR_COOKIE_SYNC_GDPR_CONSENT_MISSING_SIGNAL_AMBIGUOUS.to_string())
            } else {
                Ok(())
            }
        }
        _ => Ok(()),
    }
}

/// Build a [`CookieSyncResponse`] from a list of chosen syncers and the
/// current prebid cookie.
///
/// The `limit` parameter caps how many bidder-status entries are returned.
pub fn build_cookie_sync_response(
    syncer_results: Vec<SyncerChoice>,
    cookie: &PrebidCookie,
    limit: Option<i32>,
) -> CookieSyncResponse {
    let status = if cookie.uids.is_empty() {
        CookieSyncStatus::NoCookie
    } else {
        CookieSyncStatus::Ok
    };

    let entries: Vec<BidderSyncStatus> = syncer_results
        .into_iter()
        .map(|s| {
            let sync_type_str = match s.sync_type {
                crate::usersync::SyncType::Iframe => "iframe",
                crate::usersync::SyncType::Redirect | crate::usersync::SyncType::Both => {
                    "redirect"
                }
            };
            BidderSyncStatus {
                bidder: s.bidder,
                usersync: Some(UsersyncInfo {
                    url: s.url,
                    sync_type: sync_type_str.to_string(),
                    support_cors: true,
                }),
                error: None,
                no_cookie: true,
            }
        })
        .collect();

    let bidder_status = match limit {
        Some(n) if n >= 0 => entries.into_iter().take(n as usize).collect(),
        _ => entries,
    };

    CookieSyncResponse {
        status,
        bidder_status,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usersync::{PrebidCookie, SyncType, SyncerChoice};

    #[test]
    fn test_parse_cookie_sync_request_empty_body() {
        let result = parse_cookie_sync_request(b"");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ERR_COOKIE_SYNC_BODY);
    }

    #[test]
    fn test_parse_cookie_sync_request_invalid_json() {
        let result = parse_cookie_sync_request(b"{bad json}");
        assert!(result.is_err());
        assert!(result.unwrap_err().starts_with("JSON parsing failed:"));
    }

    #[test]
    fn test_parse_cookie_sync_request_minimal() {
        let body = br#"{}"#;
        let req = parse_cookie_sync_request(body).unwrap();
        assert!(req.bidders.is_empty());
        assert_eq!(req.gdpr, None);
        assert!(req.gdpr_consent.is_empty());
        assert_eq!(req.limit, None);
    }

    #[test]
    fn test_parse_cookie_sync_request_full() {
        let body = br#"{
            "bidders": ["appnexus", "rubicon"],
            "gdpr": 1,
            "gdpr_consent": "CONSENT_STRING",
            "us_privacy": "1YNN",
            "limit": 5,
            "coopSync": true,
            "account": "pub123",
            "debug": true
        }"#;
        let req = parse_cookie_sync_request(body).unwrap();
        assert_eq!(req.bidders, vec!["appnexus", "rubicon"]);
        assert_eq!(req.gdpr, Some(1));
        assert_eq!(req.gdpr_consent, "CONSENT_STRING");
        assert_eq!(req.us_privacy, "1YNN");
        assert_eq!(req.limit, Some(5));
        assert_eq!(req.cooperative_sync, Some(true));
        assert_eq!(req.account, "pub123");
        assert!(req.debug);
    }

    #[test]
    fn test_validate_gdpr_consent_gdpr_1_no_consent() {
        let req = CookieSyncRequest {
            gdpr: Some(1),
            gdpr_consent: String::new(),
            ..Default::default()
        };
        let err = validate_gdpr_consent(&req).unwrap_err();
        assert_eq!(err, ERR_COOKIE_SYNC_GDPR_CONSENT_MISSING);
    }

    #[test]
    fn test_validate_gdpr_consent_gdpr_1_with_consent() {
        let req = CookieSyncRequest {
            gdpr: Some(1),
            gdpr_consent: "CONSENT".to_string(),
            ..Default::default()
        };
        assert!(validate_gdpr_consent(&req).is_ok());
    }

    #[test]
    fn test_validate_gdpr_consent_gdpr_ambiguous_no_consent() {
        let req = CookieSyncRequest {
            gdpr: None,
            gdpr_consent: String::new(),
            ..Default::default()
        };
        let err = validate_gdpr_consent(&req).unwrap_err();
        assert_eq!(err, ERR_COOKIE_SYNC_GDPR_CONSENT_MISSING_SIGNAL_AMBIGUOUS);
    }

    #[test]
    fn test_validate_gdpr_consent_gdpr_0() {
        let req = CookieSyncRequest {
            gdpr: Some(0),
            gdpr_consent: String::new(),
            ..Default::default()
        };
        assert!(validate_gdpr_consent(&req).is_ok());
    }

    #[test]
    fn test_build_cookie_sync_response_no_cookie() {
        let cookie = PrebidCookie::new();
        let syncs = vec![SyncerChoice {
            bidder: "appnexus".to_string(),
            sync_type: SyncType::Iframe,
            url: "http://sync.example.com".to_string(),
        }];
        let resp = build_cookie_sync_response(syncs, &cookie, None);
        assert_eq!(resp.status, CookieSyncStatus::NoCookie);
        assert_eq!(resp.bidder_status.len(), 1);
        assert_eq!(resp.bidder_status[0].bidder, "appnexus");
        let us = resp.bidder_status[0].usersync.as_ref().unwrap();
        assert_eq!(us.sync_type, "iframe");
        assert!(us.support_cors);
    }

    #[test]
    fn test_build_cookie_sync_response_with_limit() {
        let cookie = PrebidCookie::new();
        let syncs = vec![
            SyncerChoice {
                bidder: "a".to_string(),
                sync_type: SyncType::Iframe,
                url: "http://a".to_string(),
            },
            SyncerChoice {
                bidder: "b".to_string(),
                sync_type: SyncType::Redirect,
                url: "http://b".to_string(),
            },
            SyncerChoice {
                bidder: "c".to_string(),
                sync_type: SyncType::Iframe,
                url: "http://c".to_string(),
            },
        ];
        let resp = build_cookie_sync_response(syncs, &cookie, Some(2));
        assert_eq!(resp.bidder_status.len(), 2);
    }

    #[test]
    fn test_build_cookie_sync_response_ok_status() {
        let mut cookie = PrebidCookie::new();
        cookie.set_uid("existing".to_string(), "uid123".to_string());
        let resp = build_cookie_sync_response(vec![], &cookie, None);
        assert_eq!(resp.status, CookieSyncStatus::Ok);
        assert!(resp.bidder_status.is_empty());
    }

    #[test]
    fn test_parse_filter_settings() {
        let body = br#"{
            "filter_settings": {
                "iframe": { "bidders": "*", "mode": "include" },
                "image": { "bidders": ["appnexus"], "mode": "exclude" }
            }
        }"#;
        let req = parse_cookie_sync_request(body).unwrap();
        let fs = req.filter_settings.unwrap();
        let iframe = fs.iframe.unwrap();
        assert_eq!(iframe.bidders, FilterBidders::All("*".to_string()));
        assert_eq!(iframe.mode, FilterMode::Include);
        let image = fs.image.unwrap();
        assert_eq!(
            image.bidders,
            FilterBidders::List(vec!["appnexus".to_string()])
        );
        assert_eq!(image.mode, FilterMode::Exclude);
    }
}
