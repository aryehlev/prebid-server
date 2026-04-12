//! Skeleton implementation of the `GET /status` endpoint.
//!
//! Mirrors `endpoints/status.go`: if a configured status response string is
//! supplied, return it as the body; otherwise respond with 204 No Content.

use axum::{
    extract::State,
    http::{header::CONTENT_TYPE, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use std::sync::Arc;

/// Shared state for the status endpoint. A `None` response means: reply 204.
#[derive(Debug, Default, Clone)]
pub struct StatusConfig {
    pub response: Option<String>,
    /// Optional content type for the status response; defaults to
    /// `application/json` when the body parses as JSON, else text/plain.
    pub content_type: Option<String>,
}

pub type StatusState = Arc<StatusConfig>;

/// Axum handler for `GET /status` (skeleton).
pub async fn status_handler(State(state): State<StatusState>) -> Response {
    match state.response.as_deref() {
        None | Some("") => StatusCode::NO_CONTENT.into_response(),
        Some(body) => {
            let ct = state
                .content_type
                .clone()
                .unwrap_or_else(|| detect_content_type(body).to_string());
            let mut resp = (StatusCode::OK, body.to_string()).into_response();
            if let Ok(hv) = HeaderValue::from_str(&ct) {
                resp.headers_mut().insert(CONTENT_TYPE, hv);
            }
            resp
        }
    }
}

fn detect_content_type(body: &str) -> &'static str {
    if serde_json::from_str::<serde_json::Value>(body).is_ok() {
        "application/json"
    } else {
        "text/plain; charset=utf-8"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_json_vs_text() {
        assert_eq!(detect_content_type("{}"), "application/json");
        assert_eq!(detect_content_type("ready"), "text/plain; charset=utf-8");
    }
}
