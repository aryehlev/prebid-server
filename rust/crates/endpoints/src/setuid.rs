//! Skeleton implementation of the `GET /setuid` endpoint.
//!
//! Mirrors the high-level behavior of `endpoints/setuid.go`: read `bidder`
//! and `uid` query params, basic validation, and respond with a `Set-Cookie`
//! header carrying the updated `uids` cookie value.
//!
//! This is a deliberately thin skeleton — the production handler lives in
//! `lib.rs::set_uid_handler`.

use axum::{
    extract::{Query, State},
    http::{header::SET_COOKIE, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
};
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, Default)]
pub struct SetUidConfig {
    /// Set of bidder names considered valid for the `bidder` query param.
    pub known_bidders: HashSet<String>,
    /// Cookie name used for the prebid uids cookie (defaults to "uids").
    pub cookie_name: String,
    /// Optional cookie domain.
    pub cookie_domain: Option<String>,
    /// TTL for the issued cookie, in seconds.
    pub ttl_seconds: i64,
}

pub type SetUidState = Arc<SetUidConfig>;

#[derive(Debug, Clone, Deserialize)]
pub struct SetUidParams {
    /// Bidder identifier. Required.
    pub bidder: String,
    /// User id to assign to the bidder. Empty means "delete the id".
    #[serde(default)]
    pub uid: String,
    /// Optional format query param (e.g. "i" or "b") — skeleton ignores.
    #[serde(default)]
    pub f: Option<String>,
    /// Optional GDPR signal — skeleton ignores.
    #[serde(default)]
    pub gdpr: Option<u8>,
    /// Optional GDPR consent string — skeleton ignores.
    #[serde(default)]
    pub gdpr_consent: Option<String>,
}

#[derive(Debug, Error)]
pub enum SetUidError {
    #[error("missing bidder query parameter")]
    MissingBidder,
    #[error("unknown bidder: {0}")]
    UnknownBidder(String),
    #[error("invalid cookie encoding")]
    InvalidCookie,
}

impl IntoResponse for SetUidError {
    fn into_response(self) -> axum::response::Response {
        let status = match self {
            SetUidError::MissingBidder | SetUidError::UnknownBidder(_) => StatusCode::BAD_REQUEST,
            SetUidError::InvalidCookie => StatusCode::BAD_REQUEST,
        };
        (status, self.to_string()).into_response()
    }
}

/// Axum handler for `GET /setuid` (skeleton).
pub async fn setuid_handler(
    State(state): State<SetUidState>,
    Query(params): Query<SetUidParams>,
) -> Result<impl IntoResponse, SetUidError> {
    if params.bidder.trim().is_empty() {
        return Err(SetUidError::MissingBidder);
    }
    if !state.known_bidders.is_empty() && !state.known_bidders.contains(&params.bidder) {
        return Err(SetUidError::UnknownBidder(params.bidder.clone()));
    }

    let cookie_name = if state.cookie_name.is_empty() {
        "uids"
    } else {
        state.cookie_name.as_str()
    };

    // Build a minimal cookie value. Real implementation would base64-encode
    // the full `UserSyncCookie`.
    let value = format!("{}%3A{}", params.bidder, params.uid);
    let mut cookie = format!(
        "{name}={value}; Max-Age={ttl}; Path=/; HttpOnly",
        name = cookie_name,
        value = value,
        ttl = state.ttl_seconds.max(0),
    );
    if let Some(domain) = state.cookie_domain.as_deref() {
        if !domain.is_empty() {
            cookie.push_str("; Domain=");
            cookie.push_str(domain);
        }
    }

    let mut headers = HeaderMap::new();
    let header_value = HeaderValue::from_str(&cookie).map_err(|_| SetUidError::InvalidCookie)?;
    headers.insert(SET_COOKIE, header_value);

    Ok((StatusCode::OK, headers))
}
