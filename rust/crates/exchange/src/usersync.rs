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
