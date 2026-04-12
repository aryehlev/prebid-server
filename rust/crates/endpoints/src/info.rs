//! Skeleton implementation of the `/info/bidders` endpoints.
//!
//! Mirrors `endpoints/info/bidders.go` and `bidders_detail.go`:
//! - `GET /info/bidders` returns a JSON array of bidder names.
//! - `GET /info/bidders/:name` returns the YAML-sourced details for one
//!   bidder, as JSON.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Per-bidder details returned from `/info/bidders/:name`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BidderDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maintainer: Option<Maintainer>,
    #[serde(default)]
    pub capabilities: Capabilities,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Maintainer {
    #[serde(default)]
    pub email: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<MediaTypes>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<MediaTypes>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MediaTypes {
    #[serde(default, rename = "mediaTypes")]
    pub media_types: Vec<String>,
}

/// Shared state for the info handlers.
#[derive(Debug, Default)]
pub struct InfoState {
    pub bidders: HashMap<String, BidderDetails>,
}

pub type SharedInfoState = Arc<InfoState>;

#[derive(Debug, Deserialize)]
pub struct BiddersQuery {
    /// When set to "1", return disabled bidders too.
    #[serde(default)]
    pub enabledonly: Option<String>,
    /// When set to "1", return full details map instead of just names.
    #[serde(default)]
    pub all: Option<String>,
}

/// Axum handler for `GET /info/bidders` (skeleton).
pub async fn info_bidders_handler(
    State(state): State<SharedInfoState>,
    Query(q): Query<BiddersQuery>,
) -> impl IntoResponse {
    let enabled_only = q.enabledonly.as_deref() == Some("1") || q.enabledonly.as_deref() == Some("true");
    let include_all = q.all.as_deref() == Some("1") || q.all.as_deref() == Some("true");

    if include_all {
        let filtered: HashMap<&String, &BidderDetails> = state
            .bidders
            .iter()
            .filter(|(_, d)| !(enabled_only && d.disabled))
            .collect();
        return Json(serde_json::to_value(filtered).unwrap_or(serde_json::json!({})));
    }

    let mut names: Vec<&String> = state
        .bidders
        .iter()
        .filter(|(_, d)| !(enabled_only && d.disabled))
        .map(|(k, _)| k)
        .collect();
    names.sort();
    Json(serde_json::to_value(names).unwrap_or(serde_json::json!([])))
}

/// Axum handler for `GET /info/bidders/:name` (skeleton).
pub async fn info_bidder_detail_handler(
    State(state): State<SharedInfoState>,
    Path(name): Path<String>,
) -> Result<Json<BidderDetails>, (StatusCode, String)> {
    match state.bidders.get(&name) {
        Some(d) => Ok(Json(d.clone())),
        None => Err((StatusCode::NOT_FOUND, format!("unknown bidder: {}", name))),
    }
}
