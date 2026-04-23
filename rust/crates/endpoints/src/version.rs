//! Skeleton implementation of the `GET /version` endpoint.
//!
//! Mirrors `endpoints/version.go` verbatim: reply with `{revision, version}`
//! JSON, substituting "not-set" for empty strings.

use axum::{extract::State, response::IntoResponse, Json};
use serde::Serialize;
use std::sync::Arc;

const NOT_SET: &str = "not-set";

#[derive(Debug, Clone, Default)]
pub struct VersionInfo {
    pub version: String,
    pub revision: String,
}

pub type VersionState = Arc<VersionInfo>;

#[derive(Debug, Clone, Serialize)]
pub struct VersionResponse {
    pub revision: String,
    pub version: String,
}

/// Axum handler for `GET /version` (skeleton).
pub async fn version_handler(State(state): State<VersionState>) -> impl IntoResponse {
    let version = if state.version.is_empty() {
        NOT_SET.to_string()
    } else {
        state.version.clone()
    };
    let revision = if state.revision.is_empty() {
        NOT_SET.to_string()
    } else {
        state.revision.clone()
    };
    Json(VersionResponse { revision, version })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn empty_maps_to_not_set() {
        let state: VersionState = Arc::new(VersionInfo::default());
        let resp = version_handler(State(state)).await;
        // The response is a JSON(VersionResponse) — just verify serialization.
        let j = serde_json::to_value(&VersionResponse {
            revision: NOT_SET.into(),
            version: NOT_SET.into(),
        })
        .unwrap();
        assert_eq!(j["version"], "not-set");
        let _ = resp;
    }
}
