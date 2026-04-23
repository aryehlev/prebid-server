//! Skeleton implementation of the `GET /event` endpoint.
//!
//! Mirrors the high-level behavior of `endpoints/events/event.go`: parse
//! `type`, `bidid`, `a` (account id) query params, and respond with either a
//! 1x1 transparent GIF (when `format=i`) or an empty 200 body.

use axum::{
    extract::Query,
    http::{header::CONTENT_TYPE, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
};
use serde::Deserialize;
use thiserror::Error;

/// 43-byte 1x1 transparent GIF used as the pixel response.
const TRANSPARENT_GIF_1X1: &[u8] = &[
    0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xff, 0xff, 0xff,
    0x00, 0x00, 0x00, 0x21, 0xf9, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01, 0x00, 0x3b,
];

#[derive(Debug, Clone, Deserialize)]
pub struct EventParams {
    /// Event type: "win", "imp", "view", "click".
    #[serde(rename = "t")]
    pub event_type: String,
    /// Bid identifier.
    #[serde(default)]
    pub bidid: String,
    /// Account id (Go code uses query key `a`).
    #[serde(default, rename = "a")]
    pub account_id: String,
    /// Response format: "i" for image (1x1 pixel), anything else for no body.
    #[serde(default)]
    pub f: Option<String>,
    /// Bidder name.
    #[serde(default, rename = "b")]
    pub bidder: Option<String>,
    /// Timestamp.
    #[serde(default, rename = "ts")]
    pub timestamp: Option<i64>,
}

#[derive(Debug, Error)]
pub enum EventError {
    #[error("missing required query parameter: {0}")]
    MissingParam(&'static str),
    #[error("invalid event type: {0}")]
    InvalidType(String),
}

impl IntoResponse for EventError {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::BAD_REQUEST, self.to_string()).into_response()
    }
}

fn valid_event_type(t: &str) -> bool {
    matches!(t, "win" | "imp" | "view" | "click")
}

/// Axum handler for `GET /event` (skeleton).
pub async fn event_handler(
    Query(params): Query<EventParams>,
) -> Result<impl IntoResponse, EventError> {
    if params.event_type.is_empty() {
        return Err(EventError::MissingParam("t"));
    }
    if !valid_event_type(&params.event_type) {
        return Err(EventError::InvalidType(params.event_type.clone()));
    }
    if params.bidid.is_empty() {
        return Err(EventError::MissingParam("bidid"));
    }
    if params.account_id.is_empty() {
        return Err(EventError::MissingParam("a"));
    }

    let want_image = params.f.as_deref() == Some("i");
    let mut headers = HeaderMap::new();
    if want_image {
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("image/gif"));
        Ok((StatusCode::OK, headers, TRANSPARENT_GIF_1X1.to_vec()))
    } else {
        Ok((StatusCode::OK, headers, Vec::<u8>::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_is_gif_header() {
        assert_eq!(&TRANSPARENT_GIF_1X1[..3], b"GIF");
    }

    #[test]
    fn event_type_validation() {
        assert!(valid_event_type("win"));
        assert!(!valid_event_type("bogus"));
    }
}
