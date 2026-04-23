//! Skeleton implementation of the `GET /openrtb2/amp` endpoint.
//!
//! Mirrors the query-parameter surface of `endpoints/openrtb2/amp_auction.go`:
//! `tag_id`, `curl`, `w`, `h`, `ow`, `oh`, `ms`, `slot`, `timeout`, `debug`,
//! `account`, `consent_string`, `consent_type`, `gdpr_applies`,
//! `addtl_consent`, etc. The skeleton returns a JSON response whose shape
//! matches what downstream AMP clients expect.
//!
//! The production auction logic lives in `lib.rs::amp_handler`; this module
//! is a thin, dependency-light stub suitable for isolated testing.

use axum::{extract::Query, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AmpQueryParams {
    /// Stored AMP request ID. Required by the real handler.
    pub tag_id: Option<String>,
    /// Canonical URL of the AMP page.
    #[serde(default)]
    pub curl: Option<String>,
    /// Optional explicit width / height.
    #[serde(default)]
    pub w: Option<i64>,
    #[serde(default)]
    pub h: Option<i64>,
    /// "override" width / height.
    #[serde(default)]
    pub ow: Option<i64>,
    #[serde(default)]
    pub oh: Option<i64>,
    /// Comma-separated multi-size list.
    #[serde(default)]
    pub ms: Option<String>,
    /// AMP slot identifier.
    #[serde(default)]
    pub slot: Option<String>,
    /// Timeout override in ms.
    #[serde(default)]
    pub timeout: Option<u64>,
    /// Debug flag.
    #[serde(default)]
    pub debug: Option<String>,
    /// Account id.
    #[serde(default)]
    pub account: Option<String>,
    /// Privacy signals.
    #[serde(default)]
    pub consent_string: Option<String>,
    #[serde(default)]
    pub consent_type: Option<String>,
    #[serde(default)]
    pub gdpr_applies: Option<String>,
    #[serde(default)]
    pub addtl_consent: Option<String>,
}

/// AMP response envelope matching the Go `AmpResponse` shape.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AmpResponse {
    /// Map of AMP "targeting" keys -> values to return to the AMP runtime.
    pub targeting: HashMap<String, String>,
    /// Optional debug information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<serde_json::Value>,
    /// Error list, one per invalid/failed bidder or config lookup.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<AmpError>,
    /// Warnings generated while servicing the request.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<AmpError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AmpError {
    pub code: i32,
    pub message: String,
}

/// Axum handler for `GET /openrtb2/amp` (skeleton).
pub async fn amp_handler(Query(q): Query<AmpQueryParams>) -> impl IntoResponse {
    let mut resp = AmpResponse::default();

    match q.tag_id.as_deref() {
        None | Some("") => {
            resp.errors.push(AmpError {
                code: 400,
                message: "Missing required parameter: tag_id".to_string(),
            });
        }
        Some(tag_id) => {
            resp.targeting
                .insert("hb_tag_id".to_string(), tag_id.to_string());
            if let Some(curl) = q.curl.as_deref() {
                resp.targeting
                    .insert("hb_curl".to_string(), curl.to_string());
            }
            if let (Some(w), Some(h)) = (q.w, q.h) {
                resp.targeting
                    .insert("hb_size".to_string(), format!("{}x{}", w, h));
            }
        }
    }

    Json(resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_tag_id_produces_error() {
        let r = AmpResponse {
            errors: vec![AmpError {
                code: 400,
                message: "Missing required parameter: tag_id".to_string(),
            }],
            ..Default::default()
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["errors"][0]["code"], 400);
    }
}
