use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Represents a syncer configuration for a bidder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncerInfo {
    pub bidder: String,
    pub sync_type: SyncType,
    pub iframe_url: Option<String>,
    pub redirect_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SyncType {
    Iframe,
    Redirect,
    Both,
}

/// User UID stored in prebid cookie
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UidEntry {
    pub uid: String,
    pub expires: Option<String>,
}

/// The prebid cookie stores UIDs per bidder
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrebidCookie {
    pub uids: HashMap<String, UidEntry>,
    pub opt_out: bool,
}

impl PrebidCookie {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_uid(&self, bidder: &str) -> Option<&UidEntry> {
        self.uids.get(bidder)
    }

    pub fn set_uid(&mut self, bidder: String, uid: String) {
        self.uids.insert(bidder, UidEntry { uid, expires: None });
    }

    pub fn opt_out(&self) -> bool {
        self.opt_out
    }

    /// Parse from cookie string (base64-encoded JSON)
    pub fn from_cookie(cookie_val: &str) -> Self {
        // Try base64 decode then JSON parse
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(cookie_val)
            .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(cookie_val));
        if let Ok(bytes) = decoded {
            if let Ok(cookie) = serde_json::from_slice(&bytes) {
                return cookie;
            }
        }
        // Try direct JSON
        serde_json::from_str(cookie_val).unwrap_or_default()
    }

    /// Encode to base64 JSON for cookie
    pub fn to_cookie_string(&self) -> String {
        use base64::Engine;
        let json = serde_json::to_string(self).unwrap_or_default();
        base64::engine::general_purpose::STANDARD.encode(json.as_bytes())
    }
}

/// Syncer determines the sync URL for a bidder
pub struct Syncer {
    pub bidder: String,
    pub iframe_url: Option<String>,
    pub redirect_url: Option<String>,
}

impl Syncer {
    pub fn get_sync_url(&self, sync_type: &SyncType, gdpr: i32, consent: &str) -> Option<String> {
        let base = match sync_type {
            SyncType::Iframe => self.iframe_url.as_deref()?,
            SyncType::Redirect | SyncType::Both => self.redirect_url.as_deref()?,
        };
        let url = base
            .replace("{gdpr}", &gdpr.to_string())
            .replace("{gdpr_consent}", consent)
            .replace("{{gdpr}}", &gdpr.to_string())
            .replace("{{gdpr_consent}}", consent);
        Some(url)
    }
}

// ---------------------------------------------------------------------------
// BidderChooser – selects which bidders need syncing
// ---------------------------------------------------------------------------

/// Configuration for choosing bidders to sync
#[derive(Debug, Clone)]
pub struct ChooserConfig {
    pub max_syncs_per_request: usize,
    pub cooperative_sync_enabled: bool,
    pub cooperative_sync_priority: Vec<String>,
}

impl Default for ChooserConfig {
    fn default() -> Self {
        Self {
            max_syncs_per_request: 8,
            cooperative_sync_enabled: true,
            cooperative_sync_priority: Vec::new(),
        }
    }
}

/// Result of choosing bidders for syncing
#[derive(Debug, Clone, Default)]
pub struct ChooseResult {
    pub syncs: Vec<SyncerChoice>,
    pub rejected: Vec<RejectedSyncer>,
}

#[derive(Debug, Clone)]
pub struct SyncerChoice {
    pub bidder: String,
    pub sync_type: SyncType,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct RejectedSyncer {
    pub bidder: String,
    pub reason: RejectionReason,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RejectionReason {
    AlreadySynced,
    OptedOut,
    GdprBlocked,
    TypeNotSupported,
    MaxSyncsReached,
    BidderDisabled,
}

impl std::fmt::Display for RejectionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::AlreadySynced => write!(f, "already synced"),
            Self::OptedOut => write!(f, "user opted out"),
            Self::GdprBlocked => write!(f, "blocked by GDPR"),
            Self::TypeNotSupported => write!(f, "sync type not supported"),
            Self::MaxSyncsReached => write!(f, "max syncs reached"),
            Self::BidderDisabled => write!(f, "bidder disabled"),
        }
    }
}

/// Choose which bidders should be included in a cookie sync response
pub fn choose_bidders(
    cookie: &PrebidCookie,
    available_syncers: &HashMap<String, Syncer>,
    requested_bidders: &[String],
    gdpr_applies: bool,
    consent_string: &str,
    config: &ChooserConfig,
) -> ChooseResult {
    let mut result = ChooseResult::default();

    if cookie.opt_out() {
        for bidder in requested_bidders {
            result.rejected.push(RejectedSyncer {
                bidder: bidder.clone(),
                reason: RejectionReason::OptedOut,
            });
        }
        return result;
    }

    let bidders_to_check: Vec<&String> = if requested_bidders.is_empty() {
        // Cooperative sync: use all available syncers
        if config.cooperative_sync_enabled {
            available_syncers.keys().collect()
        } else {
            return result;
        }
    } else {
        requested_bidders.iter().collect()
    };

    for bidder in bidders_to_check {
        if result.syncs.len() >= config.max_syncs_per_request {
            result.rejected.push(RejectedSyncer {
                bidder: bidder.clone(),
                reason: RejectionReason::MaxSyncsReached,
            });
            continue;
        }

        // Check if already synced
        if cookie.get_uid(bidder).is_some() {
            result.rejected.push(RejectedSyncer {
                bidder: bidder.clone(),
                reason: RejectionReason::AlreadySynced,
            });
            continue;
        }

        // Check GDPR
        if gdpr_applies && consent_string.is_empty() {
            result.rejected.push(RejectedSyncer {
                bidder: bidder.clone(),
                reason: RejectionReason::GdprBlocked,
            });
            continue;
        }

        // Get syncer
        let syncer = match available_syncers.get(bidder.as_str()) {
            Some(s) => s,
            None => {
                result.rejected.push(RejectedSyncer {
                    bidder: bidder.clone(),
                    reason: RejectionReason::BidderDisabled,
                });
                continue;
            }
        };

        // Determine sync type and URL
        let (sync_type, url) = if let Some(url) = syncer.get_sync_url(
            &SyncType::Iframe,
            if gdpr_applies { 1 } else { 0 },
            consent_string,
        ) {
            (SyncType::Iframe, url)
        } else if let Some(url) = syncer.get_sync_url(
            &SyncType::Redirect,
            if gdpr_applies { 1 } else { 0 },
            consent_string,
        ) {
            (SyncType::Redirect, url)
        } else {
            result.rejected.push(RejectedSyncer {
                bidder: bidder.clone(),
                reason: RejectionReason::TypeNotSupported,
            });
            continue;
        };

        result.syncs.push(SyncerChoice {
            bidder: bidder.clone(),
            sync_type,
            url,
        });
    }

    result
}

// ---------------------------------------------------------------------------
// Cookie sync response types
// ---------------------------------------------------------------------------

/// Cookie sync response returned to the client
#[derive(Debug, Clone, Serialize)]
pub struct CookieSyncResponse {
    pub status: CookieSyncStatus,
    #[serde(rename = "bidder_status")]
    pub bidder_status: Vec<BidderSyncStatus>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CookieSyncStatus {
    Ok,
    NoCookie,
    OptOut,
}

#[derive(Debug, Clone, Serialize)]
pub struct BidderSyncStatus {
    pub bidder: String,
    #[serde(rename = "usersync")]
    pub usersync: Option<UsersyncInfo>,
    pub error: Option<String>,
    #[serde(rename = "no_cookie")]
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

/// Build a CookieSyncResponse from a ChooseResult
pub fn build_sync_response(
    cookie: &PrebidCookie,
    choose_result: &ChooseResult,
) -> CookieSyncResponse {
    let status = if cookie.opt_out() {
        CookieSyncStatus::OptOut
    } else if cookie.uids.is_empty() {
        CookieSyncStatus::NoCookie
    } else {
        CookieSyncStatus::Ok
    };

    let bidder_status = choose_result
        .syncs
        .iter()
        .map(|s| BidderSyncStatus {
            bidder: s.bidder.clone(),
            usersync: Some(UsersyncInfo {
                url: s.url.clone(),
                sync_type: match s.sync_type {
                    SyncType::Iframe => "iframe".to_string(),
                    SyncType::Redirect => "redirect".to_string(),
                    SyncType::Both => "redirect".to_string(),
                },
                support_cors: true,
            }),
            error: None,
            no_cookie: true,
        })
        .collect();

    CookieSyncResponse {
        status,
        bidder_status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_syncer(
        bidder: &str,
        iframe: Option<&str>,
        redirect: Option<&str>,
    ) -> (String, Syncer) {
        (
            bidder.to_string(),
            Syncer {
                bidder: bidder.to_string(),
                iframe_url: iframe.map(String::from),
                redirect_url: redirect.map(String::from),
            },
        )
    }

    #[test]
    fn test_prebid_cookie_round_trip() {
        let mut cookie = PrebidCookie::new();
        cookie.set_uid("appnexus".to_string(), "uid123".to_string());
        let encoded = cookie.to_cookie_string();
        let decoded = PrebidCookie::from_cookie(&encoded);
        assert_eq!(decoded.get_uid("appnexus").unwrap().uid, "uid123");
    }

    #[test]
    fn test_choose_bidders_opts_out() {
        let cookie = PrebidCookie {
            opt_out: true,
            ..Default::default()
        };
        let syncers = HashMap::new();
        let result = choose_bidders(
            &cookie,
            &syncers,
            &["appnexus".to_string()],
            false,
            "",
            &ChooserConfig::default(),
        );
        assert!(result.syncs.is_empty());
        assert_eq!(result.rejected[0].reason, RejectionReason::OptedOut);
    }

    #[test]
    fn test_choose_bidders_already_synced() {
        let mut cookie = PrebidCookie::new();
        cookie.set_uid("appnexus".to_string(), "uid123".to_string());
        let syncers: HashMap<String, Syncer> =
            [make_syncer("appnexus", Some("http://iframe"), None)]
                .into_iter()
                .collect();
        let result = choose_bidders(
            &cookie,
            &syncers,
            &["appnexus".to_string()],
            false,
            "",
            &ChooserConfig::default(),
        );
        assert!(result.syncs.is_empty());
        assert_eq!(result.rejected[0].reason, RejectionReason::AlreadySynced);
    }

    #[test]
    fn test_choose_bidders_success() {
        let cookie = PrebidCookie::new();
        let syncers: HashMap<String, Syncer> = [make_syncer(
            "appnexus",
            Some("http://iframe?gdpr={gdpr}"),
            None,
        )]
        .into_iter()
        .collect();
        let result = choose_bidders(
            &cookie,
            &syncers,
            &["appnexus".to_string()],
            false,
            "",
            &ChooserConfig::default(),
        );
        assert_eq!(result.syncs.len(), 1);
        assert_eq!(result.syncs[0].bidder, "appnexus");
        assert!(result.syncs[0].url.contains("gdpr=0"));
    }

    #[test]
    fn test_choose_bidders_max_limit() {
        let cookie = PrebidCookie::new();
        let syncers: HashMap<String, Syncer> = [
            make_syncer("a", Some("http://a"), None),
            make_syncer("b", Some("http://b"), None),
            make_syncer("c", Some("http://c"), None),
        ]
        .into_iter()
        .collect();
        let config = ChooserConfig {
            max_syncs_per_request: 2,
            ..Default::default()
        };
        let result = choose_bidders(
            &cookie,
            &syncers,
            &["a".to_string(), "b".to_string(), "c".to_string()],
            false,
            "",
            &config,
        );
        assert_eq!(result.syncs.len(), 2);
        assert_eq!(
            result
                .rejected
                .iter()
                .filter(|r| r.reason == RejectionReason::MaxSyncsReached)
                .count(),
            1
        );
    }

    #[test]
    fn test_build_sync_response() {
        let cookie = PrebidCookie::new();
        let choose_result = ChooseResult {
            syncs: vec![SyncerChoice {
                bidder: "appnexus".to_string(),
                sync_type: SyncType::Iframe,
                url: "http://sync".to_string(),
            }],
            rejected: vec![],
        };
        let response = build_sync_response(&cookie, &choose_result);
        assert_eq!(response.status, CookieSyncStatus::NoCookie);
        assert_eq!(response.bidder_status.len(), 1);
    }
}
