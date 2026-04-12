//! Skeleton implementation of the `GET /getuids` endpoint.
//!
//! Mirrors `endpoints/getuids.go`: read the prebid `uids` cookie from the
//! request headers, decode it, and return the `{buyeruids: {...}}` JSON
//! payload. This is a dependency-light skeleton that uses only the request
//! `Cookie` header; the production path lives in `lib.rs::get_uids_handler`.

use axum::{
    http::{header::COOKIE, HeaderMap},
    response::IntoResponse,
    Json,
};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize)]
pub struct UserSyncs {
    #[serde(rename = "buyeruids", skip_serializing_if = "HashMap::is_empty")]
    pub buyer_uids: HashMap<String, String>,
}

/// Wire-format of the prebid uids cookie. Matches the shape used by
/// `endpoints/getuids.go` closely enough for skeleton purposes.
#[derive(Debug, Clone, Default, Deserialize)]
struct UidsCookieWire {
    #[serde(default, rename = "tempUIDs")]
    temp_uids: HashMap<String, UidEntryWire>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct UidEntryWire {
    #[serde(default)]
    uid: String,
}

fn read_cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let raw = headers.get(COOKIE)?.to_str().ok()?;
    for piece in raw.split(';') {
        let trimmed = piece.trim();
        if let Some(eq) = trimmed.find('=') {
            let (k, v) = trimmed.split_at(eq);
            if k == name {
                return Some(&v[1..]);
            }
        }
    }
    None
}

fn decode_uids_cookie(value: &str) -> UserSyncs {
    let Ok(bytes) = BASE64.decode(value) else {
        return UserSyncs::default();
    };
    let Ok(wire) = serde_json::from_slice::<UidsCookieWire>(&bytes) else {
        return UserSyncs::default();
    };
    let mut buyer_uids = HashMap::new();
    for (bidder, entry) in wire.temp_uids {
        if !entry.uid.is_empty() {
            buyer_uids.insert(bidder, entry.uid);
        }
    }
    UserSyncs { buyer_uids }
}

/// Axum handler for `GET /getuids` (skeleton).
pub async fn getuids_handler(headers: HeaderMap) -> impl IntoResponse {
    let cookie_value = read_cookie_value(&headers, "uids").unwrap_or("");
    let body = if cookie_value.is_empty() {
        UserSyncs::default()
    } else {
        decode_uids_cookie(cookie_value)
    };
    Json(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_cookie_returns_empty_map() {
        let s = decode_uids_cookie("");
        assert!(s.buyer_uids.is_empty());
    }
}
