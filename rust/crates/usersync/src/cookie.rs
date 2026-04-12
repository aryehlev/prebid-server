//! Cookie representation for user sync state.
//!
//! Ported from `usersync/cookie.go`.

use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Default amount of time a UID stored in a cookie is considered valid.
/// This is separate from the cookie TTL itself.
pub const UID_TTL_DAYS: i64 = 14;

/// The name of the cookie used by Prebid Server.
pub const UID_COOKIE_NAME: &str = "uids";

/// A single user ID entry associated with a bidder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UidEntry {
    /// ID assigned to a user by a particular bidder.
    pub uid: String,
    /// Time after which this UID should no longer be used.
    pub expires: DateTime<Utc>,
}

/// The prebid-server `uids` cookie.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cookie {
    uids: HashMap<String, UidEntry>,
    opt_out: bool,
}

/// Storage format for the cookie on the wire. Matches Go's `cookieJson`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CookieJson {
    #[serde(
        rename = "tempUIDs",
        default,
        skip_serializing_if = "HashMap::is_empty"
    )]
    uids: HashMap<String, UidEntry>,
    #[serde(default, skip_serializing_if = "is_false", rename = "optout")]
    opt_out: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Errors produced while manipulating a [`Cookie`].
#[derive(Debug, Error)]
pub enum CookieError {
    #[error("the user has opted out of prebid server cookie syncs")]
    OptedOut,
    #[error("base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

impl Cookie {
    /// Returns a new empty cookie.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if the cookie allows bidders to sync IDs.
    pub fn allow_syncs(&self) -> bool {
        !self.opt_out
    }

    /// Returns the opt-out flag.
    pub fn opt_out(&self) -> bool {
        self.opt_out
    }

    /// Sets the opt-out flag. When opting out, clears all stored UIDs.
    pub fn set_opt_out(&mut self, opt_out: bool) {
        self.opt_out = opt_out;
        if opt_out {
            self.uids.clear();
        }
    }

    /// Returns the full entry for the given syncer key, if any.
    pub fn entry(&self, key: &str) -> Option<&UidEntry> {
        self.uids.get(key)
    }

    /// Look up the UID for a syncer key.
    /// Returns `(uid, is_found, is_active)`.
    pub fn get_uid(&self, key: &str) -> (String, bool, bool) {
        match self.uids.get(key) {
            Some(entry) => (entry.uid.clone(), true, Utc::now() < entry.expires),
            None => (String::new(), false, false),
        }
    }

    /// Returns a map from bidder name to UID (without expiration).
    pub fn get_uids(&self) -> HashMap<String, String> {
        self.uids
            .iter()
            .map(|(k, v)| (k.clone(), v.uid.clone()))
            .collect()
    }

    /// Returns true if there is an active (non-expired) sync for the given key.
    pub fn has_live_sync(&self, key: &str) -> bool {
        let (_, _, active) = self.get_uid(key);
        active
    }

    /// Returns true if there is at least one active sync in the cookie.
    pub fn has_live_uids(&self) -> bool {
        let now = Utc::now();
        self.uids.values().any(|entry| now < entry.expires)
    }

    /// Number of entries stored in the cookie (active or expired).
    pub fn len(&self) -> usize {
        self.uids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.uids.is_empty()
    }

    /// Tries to set the UID for the given syncer key.
    /// Fails with [`CookieError::OptedOut`] if the user has opted out.
    pub fn sync(&mut self, key: &str, uid: &str) -> Result<(), CookieError> {
        if !self.allow_syncs() {
            return Err(CookieError::OptedOut);
        }
        let expires = Utc::now() + Duration::days(UID_TTL_DAYS);
        self.uids.insert(
            key.to_string(),
            UidEntry {
                uid: uid.to_string(),
                expires,
            },
        );
        Ok(())
    }

    /// Remove the UID for the given syncer key.
    pub fn unsync(&mut self, key: &str) {
        self.uids.remove(key);
    }

    /// Encode the cookie as a URL-safe base64 encoded JSON blob.
    pub fn encode(&self) -> Result<String, CookieError> {
        let data = CookieJson {
            uids: self.uids.clone(),
            opt_out: self.opt_out,
        };
        let json = serde_json::to_vec(&data)?;
        Ok(base64::engine::general_purpose::URL_SAFE.encode(json))
    }

    /// Decode a cookie from its URL-safe base64 JSON representation. Returns a new
    /// empty cookie on decode failure (matching the Go behaviour).
    pub fn decode(value: &str) -> Self {
        // Be lenient with padding (Go uses URLEncoding with padding; some clients don't).
        let engine = if value.ends_with('=') {
            base64::engine::general_purpose::URL_SAFE
        } else {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
        };
        let bytes = match engine.decode(value) {
            Ok(b) => b,
            Err(_) => return Self::new(),
        };
        let parsed: CookieJson = match serde_json::from_slice(&bytes) {
            Ok(p) => p,
            Err(_) => return Self::new(),
        };
        let mut cookie = Cookie {
            uids: if parsed.opt_out {
                HashMap::new()
            } else {
                parsed.uids
            },
            opt_out: parsed.opt_out,
        };
        // Audience network: UID "0" means "not yet recognized".
        if let Some(entry) = cookie.uids.get("audienceNetwork") {
            if entry.uid == "0" {
                cookie.uids.remove("audienceNetwork");
            }
        }
        cookie
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_cookie_is_empty_and_allows_syncs() {
        let c = Cookie::new();
        assert!(c.allow_syncs());
        assert!(!c.has_live_uids());
        assert_eq!(c.len(), 0);
    }

    #[test]
    fn sync_and_get_uid() {
        let mut c = Cookie::new();
        c.sync("rubicon", "abc-123").unwrap();
        let (uid, found, active) = c.get_uid("rubicon");
        assert_eq!(uid, "abc-123");
        assert!(found);
        assert!(active);
        assert!(c.has_live_sync("rubicon"));
        assert!(c.has_live_uids());
    }

    #[test]
    fn unsync_removes_entry() {
        let mut c = Cookie::new();
        c.sync("rubicon", "abc").unwrap();
        c.unsync("rubicon");
        let (_, found, _) = c.get_uid("rubicon");
        assert!(!found);
    }

    #[test]
    fn opt_out_blocks_syncs_and_clears_entries() {
        let mut c = Cookie::new();
        c.sync("rubicon", "abc").unwrap();
        c.set_opt_out(true);
        assert!(c.is_empty());
        assert!(c.sync("appnexus", "x").is_err());
    }

    #[test]
    fn encode_decode_roundtrip() {
        let mut c = Cookie::new();
        c.sync("rubicon", "abc").unwrap();
        c.sync("appnexus", "xyz").unwrap();

        let encoded = c.encode().unwrap();
        let decoded = Cookie::decode(&encoded);

        let (uid1, _, _) = decoded.get_uid("rubicon");
        let (uid2, _, _) = decoded.get_uid("appnexus");
        assert_eq!(uid1, "abc");
        assert_eq!(uid2, "xyz");
        assert_eq!(decoded.len(), 2);
    }

    #[test]
    fn decode_invalid_base64_returns_empty() {
        let c = Cookie::decode("not valid base64 !!!");
        assert!(c.is_empty());
        assert!(c.allow_syncs());
    }

    #[test]
    fn decode_opt_out_cookie() {
        let json = r#"{"optout":true}"#;
        let encoded = base64::engine::general_purpose::URL_SAFE.encode(json);
        let c = Cookie::decode(&encoded);
        assert!(c.opt_out());
        assert!(!c.allow_syncs());
    }

    #[test]
    fn audience_network_zero_uid_dropped_on_decode() {
        let mut c = Cookie::new();
        c.uids.insert(
            "audienceNetwork".to_string(),
            UidEntry {
                uid: "0".to_string(),
                expires: Utc::now() + Duration::days(1),
            },
        );
        let encoded = c.encode().unwrap();
        let decoded = Cookie::decode(&encoded);
        let (_, found, _) = decoded.get_uid("audienceNetwork");
        assert!(!found);
    }
}
