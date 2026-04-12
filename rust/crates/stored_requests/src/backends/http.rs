//! HTTP-backed fetcher.
//!
//! Issues `GET {endpoint}?request-ids=a,b&imp-ids=c,d` and parses a JSON
//! body of the shape:
//!
//! ```json
//! { "requests": { "...": {} }, "imps": { "...": {} } }
//! ```

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

use crate::fetcher::{FetchError, Fetcher};

/// HTTP-backed stored-request fetcher.
pub struct HttpFetcher {
    /// The `reqwest` client used to issue requests.
    pub client: reqwest::Client,
    /// The base endpoint URL (without query string).
    pub endpoint_url: String,
}

impl HttpFetcher {
    /// Create a new [`HttpFetcher`].
    pub fn new(client: reqwest::Client, endpoint_url: impl Into<String>) -> Self {
        Self {
            client,
            endpoint_url: endpoint_url.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ResponseContract {
    #[serde(default)]
    requests: HashMap<String, Option<Value>>,
    #[serde(default)]
    imps: HashMap<String, Option<Value>>,
}

fn split_nulls(
    raw: HashMap<String, Option<Value>>,
    data_type: &str,
    errs: &mut Vec<FetchError>,
) -> HashMap<String, Value> {
    let mut out = HashMap::with_capacity(raw.len());
    for (k, v) in raw {
        match v {
            Some(val) => {
                out.insert(k, val);
            }
            None => errs.push(FetchError::not_found(k, data_type)),
        }
    }
    out
}

#[async_trait]
impl Fetcher for HttpFetcher {
    async fn fetch_requests(
        &self,
        req_ids: &[String],
        imp_ids: &[String],
    ) -> (
        HashMap<String, Value>,
        HashMap<String, Value>,
        Vec<FetchError>,
    ) {
        if req_ids.is_empty() && imp_ids.is_empty() {
            return (HashMap::new(), HashMap::new(), Vec::new());
        }

        let mut query: Vec<(&str, String)> = Vec::new();
        if !req_ids.is_empty() {
            query.push(("request-ids", req_ids.join(",")));
        }
        if !imp_ids.is_empty() {
            query.push(("imp-ids", imp_ids.join(",")));
        }

        let resp = match self
            .client
            .get(&self.endpoint_url)
            .query(&query)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return (HashMap::new(), HashMap::new(), vec![FetchError::from(e)]),
        };

        if !resp.status().is_success() {
            let status = resp.status();
            return (
                HashMap::new(),
                HashMap::new(),
                vec![FetchError::Other(format!(
                    "Error fetching Stored Requests via HTTP. Response code was {status}"
                ))],
            );
        }

        let bytes = match resp.bytes().await {
            Ok(b) => b,
            Err(e) => return (HashMap::new(), HashMap::new(), vec![FetchError::from(e)]),
        };

        let parsed: ResponseContract = match serde_json::from_slice(&bytes) {
            Ok(p) => p,
            Err(e) => return (HashMap::new(), HashMap::new(), vec![FetchError::from(e)]),
        };

        let mut errs = Vec::new();
        let req = split_nulls(parsed.requests, "Request", &mut errs);
        let imp = split_nulls(parsed.imps, "Imp", &mut errs);
        (req, imp, errs)
    }

    async fn fetch_account(&self, account_id: &str) -> Result<Value, FetchError> {
        Err(FetchError::not_found(account_id, "Account"))
    }

    async fn fetch_categories(
        &self,
        _primary_adserver: &str,
        _publisher_id: &str,
    ) -> Result<String, FetchError> {
        Err(FetchError::Other(
            "HttpFetcher::fetch_categories not implemented".into(),
        ))
    }

    async fn fetch_responses(
        &self,
        _ids: &[String],
    ) -> Result<HashMap<String, Value>, FetchError> {
        Ok(HashMap::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse a canned JSON payload via the shared `ResponseContract` type.
    /// This exercises the null -> NotFound conversion without actually
    /// standing up an HTTP server (which would require a real runtime
    /// listener). A full end-to-end test would be marked `#[ignore]`.
    #[test]
    fn parses_canned_response_contract() {
        let body = br#"{
            "requests": {
                "req1": {"a": 1},
                "req2": null
            },
            "imps": {
                "imp1": {"b": 2}
            }
        }"#;
        let parsed: ResponseContract = serde_json::from_slice(body).unwrap();
        let mut errs = Vec::new();
        let req = split_nulls(parsed.requests, "Request", &mut errs);
        let imp = split_nulls(parsed.imps, "Imp", &mut errs);
        assert_eq!(req.len(), 1);
        assert_eq!(req.get("req1").unwrap()["a"], 1);
        assert_eq!(imp.len(), 1);
        assert_eq!(errs.len(), 1);
        matches!(errs[0], FetchError::NotFound { .. });
    }

    /// Full HTTP round-trip test would go here. Marked `#[ignore]` because
    /// it needs a live `TcpListener`-backed stub server; the non-ignored
    /// test above covers the response parsing path.
    #[tokio::test]
    #[ignore = "requires a live TcpListener stub server"]
    async fn http_fetcher_round_trip() {}
}
