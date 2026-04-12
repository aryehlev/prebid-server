//! Skeleton implementation of the `POST /cookie_sync` endpoint.
//!
//! Mirrors the shape of `endpoints/cookie_sync.go` at a high level: accept a
//! JSON body listing bidders to sync plus an optional limit, iterate over the
//! configured bidder sync URLs, and return a JSON response containing a
//! `BidderStatus` entry per bidder that still needs a user sync.
//!
//! This is intentionally a skeleton — the production handler lives in
//! `lib.rs::cookie_sync_handler` and covers privacy, analytics, metrics, etc.
//! This module exposes a narrower, dependency-light surface suitable for
//! isolated testing and router wiring.

use axum::{extract::State, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Per-bidder sync configuration used by the skeleton.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BidderSyncUrls {
    /// Preferred sync type: "iframe" or "redirect".
    #[serde(default)]
    pub preferred: String,
    /// Optional iframe URL template.
    #[serde(default)]
    pub iframe: Option<String>,
    /// Optional redirect (image) URL template.
    #[serde(default)]
    pub redirect: Option<String>,
}

/// Shared config for the cookie_sync skeleton handler.
#[derive(Debug, Clone, Default)]
pub struct CookieSyncConfig {
    pub bidders: HashMap<String, BidderSyncUrls>,
    /// Optional default limit applied when the request does not specify one.
    pub default_limit: Option<u32>,
}

pub type CookieSyncState = Arc<CookieSyncConfig>;

/// Incoming JSON payload for POST /cookie_sync.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CookieSyncRequest {
    /// Explicit list of bidders to sync. `None` means "all configured".
    #[serde(default)]
    pub bidders: Option<Vec<String>>,
    /// Maximum number of bidder statuses to return.
    #[serde(default)]
    pub limit: Option<u32>,
    /// Opaque GDPR consent signal — unused by the skeleton.
    #[serde(default)]
    pub gdpr: Option<u8>,
    /// Opaque GDPR consent string — unused by the skeleton.
    #[serde(default)]
    pub gdpr_consent: Option<String>,
    /// US privacy (CCPA) string — unused by the skeleton.
    #[serde(default)]
    pub us_privacy: Option<String>,
}

/// Per-bidder status returned by the handler.
#[derive(Debug, Clone, Serialize)]
pub struct BidderStatus {
    pub bidder: String,
    pub no_cookie: bool,
    pub usersync: UserSync,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserSync {
    pub url: String,
    #[serde(rename = "type")]
    pub sync_type: String,
    #[serde(rename = "supportCORS")]
    pub support_cors: bool,
}

/// Outgoing JSON payload from POST /cookie_sync.
#[derive(Debug, Clone, Serialize)]
pub struct CookieSyncResponse {
    pub status: String,
    pub bidder_status: Vec<BidderStatus>,
}

/// Axum handler for `POST /cookie_sync` (skeleton).
pub async fn cookie_sync_handler(
    State(state): State<CookieSyncState>,
    Json(body): Json<CookieSyncRequest>,
) -> impl IntoResponse {
    let limit = body
        .limit
        .or(state.default_limit)
        .unwrap_or(u32::MAX) as usize;

    let requested: Vec<String> = body
        .bidders
        .unwrap_or_else(|| state.bidders.keys().cloned().collect());

    let mut statuses: Vec<BidderStatus> = Vec::new();
    for bidder in requested.into_iter() {
        if statuses.len() >= limit {
            break;
        }
        if let Some(urls) = state.bidders.get(&bidder) {
            let (sync_type, url) = match urls.preferred.as_str() {
                "iframe" => ("iframe", urls.iframe.clone()),
                _ => ("redirect", urls.redirect.clone()),
            };
            let Some(url) = url else { continue };
            statuses.push(BidderStatus {
                bidder,
                no_cookie: true,
                usersync: UserSync {
                    url,
                    sync_type: sync_type.to_string(),
                    support_cors: false,
                },
            });
        } else {
            tracing::debug!(bidder = %bidder, "cookie_sync: unknown bidder");
        }
    }

    Json(CookieSyncResponse {
        status: if statuses.is_empty() {
            "ok".to_string()
        } else {
            "no_cookie".to_string()
        },
        bidder_status: statuses,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_deserializes_with_defaults() {
        let r: CookieSyncRequest = serde_json::from_str("{}").unwrap();
        assert!(r.bidders.is_none());
        assert!(r.limit.is_none());
    }
}
