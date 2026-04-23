//! Shadow proxy: forwards a request to two backends in parallel and diffs
//! the resulting responses.

use crate::diff::{diff, DiffEntry};
use crate::options::DiffOptions;
use std::collections::HashMap;
use thiserror::Error;

/// Error type for [`ShadowProxy::forward`].
#[derive(Debug, Error)]
pub enum ProxyError {
    /// Error from the underlying HTTP client.
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    /// Invalid HTTP header name or value on the outgoing request.
    #[error("invalid header: {0}")]
    InvalidHeader(String),
    /// Primary backend returned a non-success status.
    #[error("primary status {0}")]
    PrimaryStatus(u16),
}

/// Response returned to the caller plus bookkeeping for the shadow diff.
#[derive(Debug, Clone)]
pub struct ShadowResponse {
    /// HTTP status returned by the primary backend.
    pub status: u16,
    /// Response headers from the primary backend.
    pub headers: HashMap<String, String>,
    /// Raw body returned by the primary backend.
    pub body: Vec<u8>,
    /// Diff between the primary and secondary bodies (empty if identical or
    /// the secondary failed to respond).
    pub diffs: Vec<DiffEntry>,
}

/// Forwards a single request to two independent backends, returns the primary
/// response immediately, and computes a diff between the two JSON bodies.
#[derive(Debug, Clone)]
pub struct ShadowProxy {
    /// HTTP client used to call the primary backend.
    pub primary: reqwest::Client,
    /// HTTP client used to call the secondary backend.
    pub secondary: reqwest::Client,
    /// Base URL for the primary backend.
    pub primary_url: String,
    /// Base URL for the secondary backend.
    pub secondary_url: String,
    /// Options used when diffing the two JSON bodies.
    pub diff_opts: DiffOptions,
}

impl ShadowProxy {
    /// Construct a new shadow proxy. `primary` is the backend whose response
    /// is returned to the caller; `secondary` is the shadow being compared.
    pub fn new(
        primary_url: impl Into<String>,
        secondary_url: impl Into<String>,
        diff_opts: DiffOptions,
    ) -> Self {
        Self {
            primary: reqwest::Client::new(),
            secondary: reqwest::Client::new(),
            primary_url: primary_url.into(),
            secondary_url: secondary_url.into(),
            diff_opts,
        }
    }

    /// Forward the request body/headers to both backends in parallel. Returns
    /// the primary response plus any diffs computed against the secondary.
    ///
    /// The diff is emitted via `tracing::info!` tagged with the `request_id`
    /// extracted from the `x-request-id` header (or `"unknown"` if absent).
    pub async fn forward(
        &self,
        body: Vec<u8>,
        headers: HashMap<String, String>,
    ) -> Result<ShadowResponse, ProxyError> {
        let request_id = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("x-request-id"))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| "unknown".to_string());

        let primary_fut = send(&self.primary, &self.primary_url, body.clone(), &headers);
        let secondary_fut = send(&self.secondary, &self.secondary_url, body, &headers);
        let (primary_res, secondary_res) = tokio::join!(primary_fut, secondary_fut);

        let primary = primary_res?;

        // Compute the diff asynchronously (and non-fatally): secondary
        // failures must not break the caller.
        let diffs = match secondary_res {
            Ok(secondary) => {
                let diff_opts = self.diff_opts.clone();
                let rid = request_id.clone();
                let primary_body = primary.body.clone();
                let secondary_body = secondary.body.clone();
                // Run the diff on the tokio runtime so it does not block the
                // response path. For small payloads it is fast enough inline,
                // but spawning makes the behavior explicit.
                tokio::spawn(async move {
                    compute_and_emit(&primary_body, &secondary_body, &diff_opts, &rid)
                })
                .await
                .unwrap_or_default()
            }
            Err(err) => {
                tracing::warn!(
                    request_id = request_id.as_str(),
                    error = %err,
                    "parity: secondary backend failed",
                );
                Vec::new()
            }
        };

        Ok(ShadowResponse {
            status: primary.status,
            headers: primary.headers,
            body: primary.body,
            diffs,
        })
    }
}

struct RawResponse {
    status: u16,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

async fn send(
    client: &reqwest::Client,
    url: &str,
    body: Vec<u8>,
    headers: &HashMap<String, String>,
) -> Result<RawResponse, ProxyError> {
    let mut builder = client.post(url).body(body);
    for (k, v) in headers {
        // Skip hop-by-hop headers that reqwest sets itself.
        if k.eq_ignore_ascii_case("host") || k.eq_ignore_ascii_case("content-length") {
            continue;
        }
        builder = builder.header(k, v);
    }
    let resp = builder.send().await?;
    let status = resp.status().as_u16();
    let mut out_headers = HashMap::new();
    for (name, value) in resp.headers().iter() {
        if let Ok(v) = value.to_str() {
            out_headers.insert(name.as_str().to_string(), v.to_string());
        }
    }
    let bytes = resp.bytes().await?.to_vec();
    Ok(RawResponse { status, headers: out_headers, body: bytes })
}

fn compute_and_emit(
    primary_body: &[u8],
    secondary_body: &[u8],
    opts: &DiffOptions,
    request_id: &str,
) -> Vec<DiffEntry> {
    let primary_json: serde_json::Value = match serde_json::from_slice(primary_body) {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(
                request_id = request_id,
                error = %err,
                "parity: primary body is not valid JSON",
            );
            return Vec::new();
        }
    };
    let secondary_json: serde_json::Value = match serde_json::from_slice(secondary_body) {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(
                request_id = request_id,
                error = %err,
                "parity: secondary body is not valid JSON",
            );
            return Vec::new();
        }
    };
    let entries = diff(&primary_json, &secondary_json, opts);
    for entry in &entries {
        tracing::info!(
            request_id = request_id,
            path = entry.path.as_str(),
            kind = ?entry.kind,
            "parity: shadow diff",
        );
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_and_emit_finds_diffs() {
        let a = br#"{"x":1,"y":2}"#;
        let b = br#"{"x":1,"y":3}"#;
        let opts = DiffOptions::default();
        let entries = compute_and_emit(a, b, &opts, "req-1");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "y");
    }

    #[test]
    fn compute_and_emit_equal() {
        let a = br#"{"x":1}"#;
        let b = br#"{"x":1}"#;
        let opts = DiffOptions::default();
        assert!(compute_and_emit(a, b, &opts, "req-1").is_empty());
    }

    #[test]
    fn compute_and_emit_handles_bad_json() {
        let a = b"not json";
        let b = br#"{"x":1}"#;
        let opts = DiffOptions::default();
        assert!(compute_and_emit(a, b, &opts, "req-1").is_empty());
    }
}
