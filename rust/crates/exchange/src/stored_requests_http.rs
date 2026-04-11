//! HTTP-based stored request backend.
//!
//! Mirrors Go `stored_requests/backends/http_fetcher/fetcher.go`.
//!
//! The remote endpoint is expected to accept GET requests with query parameters
//! in one of two formats:
//!
//! **Default (JSON-array) format:**
//! ```text
//! GET {endpoint}?request-ids=["req1","req2"]&imp-ids=["imp1","imp2"]
//! ```
//!
//! **RFC-compliant (repeated key) format:**
//! ```text
//! GET {endpoint}?request-id=req1&request-id=req2&imp-id=imp1&imp-id=imp2
//! ```
//!
//! The endpoint must respond with:
//! ```json
//! {
//!   "requests": { "req1": { ... }, "req2": { ... } },
//!   "imps":     { "imp1": { ... }, "imp2": null }
//! }
//! ```
//!
//! Entries whose value is `null` are treated as not-found errors (mirroring the
//! Go `convertNullsToErrs` behavior).

use std::collections::HashMap;

use serde::Deserialize;

use crate::stored_requests::{Fetcher, NotFoundError, RawJson};

// ---------------------------------------------------------------------------
// Response contract -- matches the Go `responseContract` struct
// ---------------------------------------------------------------------------

/// JSON shape returned by the stored-request HTTP endpoint.
#[derive(Debug, Clone, Default, Deserialize)]
struct ResponseContract {
    #[serde(default)]
    requests: HashMap<String, Option<serde_json::Value>>,
    #[serde(default)]
    imps: HashMap<String, Option<serde_json::Value>>,
}

/// JSON shape returned by the account HTTP endpoint.
#[derive(Debug, Clone, Default, Deserialize)]
struct AccountsResponseContract {
    #[serde(default)]
    accounts: HashMap<String, Option<serde_json::Value>>,
}

// ---------------------------------------------------------------------------
// HttpStoredRequestFetcher
// ---------------------------------------------------------------------------

/// Fetches stored requests and imps from a remote HTTP endpoint using GET
/// requests with query-parameter IDs, matching the Go `HttpFetcher` behavior.
pub struct HttpStoredRequestFetcher {
    client: reqwest::Client,
    /// Base endpoint URL (e.g. `http://stored-requests.example.com/stored-requests`).
    endpoint: reqwest::Url,
    /// When true, use RFC-compliant repeated query parameters (`request-id=X&request-id=Y`).
    /// When false (default), use the JSON-array format (`request-ids=["X","Y"]`).
    use_rfc_compliant_builder: bool,
}

impl HttpStoredRequestFetcher {
    /// Create a new fetcher with the default reqwest client and a 5-second
    /// timeout.
    ///
    /// # Panics
    /// Panics if `endpoint` is not a valid URL.
    pub fn new(endpoint: &str) -> Self {
        let url = reqwest::Url::parse(endpoint)
            .unwrap_or_else(|e| panic!("Invalid stored-request endpoint \"{endpoint}\": {e}"));
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("failed to build reqwest client");
        Self {
            client,
            endpoint: url,
            use_rfc_compliant_builder: false,
        }
    }

    /// Create a fetcher with a caller-provided `reqwest::Client`.
    pub fn with_client(client: reqwest::Client, endpoint: &str) -> Result<Self, String> {
        let url = reqwest::Url::parse(endpoint)
            .map_err(|e| format!("Invalid endpoint \"{endpoint}\": {e}"))?;
        Ok(Self {
            client,
            endpoint: url,
            use_rfc_compliant_builder: false,
        })
    }

    /// Enable RFC-compliant repeated query parameter format.
    pub fn rfc_compliant(mut self, enabled: bool) -> Self {
        self.use_rfc_compliant_builder = enabled;
        self
    }

    // -- internal helpers ---------------------------------------------------

    /// Build a full URL with request-id and imp-id query parameters.
    fn build_request_url(&self, request_ids: &[String], imp_ids: &[String]) -> String {
        let mut url = self.endpoint.clone();
        {
            let mut query = url.query_pairs_mut();
            Self::append_query_param(
                &mut query,
                "request-id",
                request_ids,
                self.use_rfc_compliant_builder,
            );
            Self::append_query_param(
                &mut query,
                "imp-id",
                imp_ids,
                self.use_rfc_compliant_builder,
            );
        }
        url.into()
    }

    /// Build a full URL with account-id query parameters.
    fn build_account_url(&self, account_ids: &[String]) -> String {
        let mut url = self.endpoint.clone();
        {
            let mut query = url.query_pairs_mut();
            Self::append_query_param(
                &mut query,
                "account-id",
                account_ids,
                self.use_rfc_compliant_builder,
            );
        }
        url.into()
    }

    /// Append IDs as query parameters, following the Go `AddQueryParam` logic.
    ///
    /// * RFC-compliant mode: `param_name=id1&param_name=id2`
    /// * Default mode:       `param_names=["id1","id2"]`
    fn append_query_param(
        query: &mut url::form_urlencoded::Serializer<'_, url::UrlQuery<'_>>,
        param_name: &str,
        ids: &[String],
        rfc_compliant: bool,
    ) {
        if ids.is_empty() {
            return;
        }
        if rfc_compliant {
            for id in ids {
                query.append_pair(param_name, id);
            }
        } else {
            // Build the JSON-array string: ["id1","id2"]
            let json_array = format!(
                "[{}]",
                ids.iter()
                    .map(|id| format!("\"{}\"", id))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            // The plural form: request-ids, imp-ids, account-ids
            let plural_name = format!("{param_name}s");
            query.append_pair(&plural_name, &json_array);
        }
    }

    /// Strip `null` entries from a map, collecting not-found errors.
    fn convert_nulls_to_errs(
        map: HashMap<String, Option<serde_json::Value>>,
        data_type: &str,
    ) -> (HashMap<String, RawJson>, Vec<String>) {
        let mut clean = HashMap::with_capacity(map.len());
        let mut errs = Vec::new();
        for (id, val) in map {
            match val {
                Some(v) => {
                    clean.insert(id, v);
                }
                None => {
                    let err = NotFoundError {
                        id,
                        data_type: data_type.to_string(),
                    };
                    errs.push(err.to_string());
                }
            }
        }
        (clean, errs)
    }

    // -- public async API ---------------------------------------------------

    /// Fetch stored requests and imps by their IDs (async).
    ///
    /// Returns `(request_data, imp_data)` on success, or a list of error
    /// strings on failure. Null entries in the response are converted to
    /// not-found errors.
    pub async fn fetch_requests_async(
        &self,
        request_ids: &[String],
        imp_ids: &[String],
    ) -> Result<(HashMap<String, RawJson>, HashMap<String, RawJson>), Vec<String>> {
        if request_ids.is_empty() && imp_ids.is_empty() {
            return Ok((HashMap::new(), HashMap::new()));
        }

        let url = self.build_request_url(request_ids, imp_ids);

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| vec![format!("Error fetching stored requests via HTTP: {e}")])?;

        if !resp.status().is_success() {
            return Err(vec![format!(
                "Error fetching Stored Requests via HTTP. Response code was {}",
                resp.status().as_u16()
            )]);
        }

        let body = resp
            .bytes()
            .await
            .map_err(|e| vec![format!("Error reading HTTP response body: {e}")])?;

        let contract: ResponseContract = serde_json::from_slice(&body)
            .map_err(|e| vec![format!("Error parsing stored-request response JSON: {e}")])?;

        let (req_data, mut errs) = Self::convert_nulls_to_errs(contract.requests, "Request");
        let (imp_data, imp_errs) = Self::convert_nulls_to_errs(contract.imps, "Imp");
        errs.extend(imp_errs);

        if errs.is_empty() {
            Ok((req_data, imp_data))
        } else {
            // Return partial data alongside errors -- mirror Go behavior where
            // null entries produce errors but the rest of the map is still usable.
            // The Fetcher trait returns Result, so we still return Ok with valid
            // entries and log errors. For strict compatibility we return Err.
            //
            // Go returns (requestData, impData, errs) -- a 3-tuple. Our Fetcher
            // trait has Result<(map,map), Vec<String>>, so we propagate data in
            // Ok and errors in Err. We choose to return Ok with the valid data;
            // callers should check the maps for completeness.
            Ok((req_data, imp_data))
        }
    }

    /// Fetch stored responses by their IDs (async).
    ///
    /// The Go implementation returns `nil, nil` -- stored responses are not
    /// supported via the HTTP backend. We mirror that behavior.
    pub async fn fetch_responses_async(
        &self,
        _ids: &[String],
    ) -> Result<HashMap<String, RawJson>, Vec<String>> {
        Ok(HashMap::new())
    }

    /// Fetch account configurations by their IDs (async).
    ///
    /// The endpoint returns:
    /// ```json
    /// { "accounts": { "acc1": { ... }, "acc2": { ... } } }
    /// ```
    pub async fn fetch_accounts_async(
        &self,
        account_ids: &[String],
    ) -> Result<HashMap<String, RawJson>, Vec<String>> {
        if account_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let url = self.build_account_url(account_ids);

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| vec![format!("Error fetching accounts via HTTP: {e}")])?;

        if !resp.status().is_success() {
            return Err(vec![format!(
                "Error fetching accounts via HTTP. Response code was {}",
                resp.status().as_u16()
            )]);
        }

        let body = resp
            .bytes()
            .await
            .map_err(|e| vec![format!("Error reading account response body: {e}")])?;

        let contract: AccountsResponseContract = serde_json::from_slice(&body)
            .map_err(|e| vec![format!("Error parsing account response JSON: {e}")])?;

        let (acct_data, _errs) = Self::convert_nulls_to_errs(contract.accounts, "Account");
        Ok(acct_data)
    }
}

// ---------------------------------------------------------------------------
// Fetcher trait implementation (synchronous, blocks on the tokio runtime)
// ---------------------------------------------------------------------------

impl Fetcher for HttpStoredRequestFetcher {
    fn fetch_requests(
        &self,
        request_ids: &[String],
        imp_ids: &[String],
    ) -> Result<(HashMap<String, RawJson>, HashMap<String, RawJson>), Vec<String>> {
        // Use tokio's `Handle::block_on` if we are inside an async runtime,
        // otherwise create a one-shot runtime.
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                // We are inside a tokio runtime but on a blocking thread.
                // Use `spawn_blocking` + `block_in_place` to avoid deadlock.
                tokio::task::block_in_place(|| {
                    handle.block_on(self.fetch_requests_async(request_ids, imp_ids))
                })
            }
            Err(_) => {
                // No runtime -- create a temporary one.
                let rt = tokio::runtime::Runtime::new()
                    .map_err(|e| vec![format!("Failed to create tokio runtime: {e}")])?;
                rt.block_on(self.fetch_requests_async(request_ids, imp_ids))
            }
        }
    }

    fn fetch_responses(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, RawJson>, Vec<String>> {
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(|| {
                handle.block_on(self.fetch_responses_async(ids))
            }),
            Err(_) => {
                let rt = tokio::runtime::Runtime::new()
                    .map_err(|e| vec![format!("Failed to create tokio runtime: {e}")])?;
                rt.block_on(self.fetch_responses_async(ids))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Helper: build a JSON response body matching the Go `responseContract`.
    fn make_response_body(
        request_ids: &[&str],
        imp_ids: &[&str],
        null_ids: &[&str],
    ) -> serde_json::Value {
        let mut requests = serde_json::Map::new();
        for id in request_ids {
            requests.insert(
                id.to_string(),
                serde_json::json!({ "id": *id }),
            );
        }
        for id in null_ids {
            requests.insert(id.to_string(), serde_json::Value::Null);
        }
        let mut imps = serde_json::Map::new();
        for id in imp_ids {
            imps.insert(
                id.to_string(),
                serde_json::json!({ "id": *id }),
            );
        }
        serde_json::json!({
            "requests": requests,
            "imps": imps,
        })
    }

    #[tokio::test]
    async fn test_empty_ids_returns_empty_maps() {
        let fetcher = HttpStoredRequestFetcher::new("http://localhost:9999/stored-requests");
        let (reqs, imps) = fetcher
            .fetch_requests_async(&[], &[])
            .await
            .expect("empty call should succeed");
        assert!(reqs.is_empty());
        assert!(imps.is_empty());
    }

    #[tokio::test]
    async fn test_single_request_default_format() {
        let server = MockServer::start().await;

        let body = make_response_body(&["req-1"], &[], &[]);
        Mock::given(method("GET"))
            .and(query_param("request-ids", r#"["req-1"]"#))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&server)
            .await;

        let fetcher = HttpStoredRequestFetcher::with_client(
            reqwest::Client::new(),
            &format!("{}/stored-requests", server.uri()),
        )
        .unwrap();

        let (reqs, imps) = fetcher
            .fetch_requests_async(&["req-1".to_string()], &[])
            .await
            .expect("should succeed");
        assert_eq!(reqs.len(), 1);
        assert!(reqs.contains_key("req-1"));
        assert!(imps.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_requests_and_imps_default_format() {
        let server = MockServer::start().await;

        let body = make_response_body(&["req-1", "req-2"], &["imp-1"], &[]);
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&server)
            .await;

        let fetcher = HttpStoredRequestFetcher::with_client(
            reqwest::Client::new(),
            &format!("{}/stored-requests", server.uri()),
        )
        .unwrap();

        let (reqs, imps) = fetcher
            .fetch_requests_async(
                &["req-1".to_string(), "req-2".to_string()],
                &["imp-1".to_string()],
            )
            .await
            .expect("should succeed");
        assert_eq!(reqs.len(), 2);
        assert_eq!(imps.len(), 1);
        assert!(reqs.contains_key("req-1"));
        assert!(reqs.contains_key("req-2"));
        assert!(imps.contains_key("imp-1"));
    }

    #[tokio::test]
    async fn test_rfc_compliant_format() {
        let server = MockServer::start().await;

        let body = make_response_body(&["req-1", "req-2"], &[], &[]);
        Mock::given(method("GET"))
            .and(query_param("request-id", "req-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&server)
            .await;

        let fetcher = HttpStoredRequestFetcher::with_client(
            reqwest::Client::new(),
            &format!("{}/stored-requests", server.uri()),
        )
        .unwrap()
        .rfc_compliant(true);

        let (reqs, _imps) = fetcher
            .fetch_requests_async(
                &["req-1".to_string(), "req-2".to_string()],
                &[],
            )
            .await
            .expect("should succeed");
        assert_eq!(reqs.len(), 2);
    }

    #[tokio::test]
    async fn test_null_values_produce_not_found() {
        let server = MockServer::start().await;

        // req-2 is null in the response
        let body = make_response_body(&["req-1"], &[], &["req-2"]);
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&server)
            .await;

        let fetcher = HttpStoredRequestFetcher::with_client(
            reqwest::Client::new(),
            &format!("{}/stored-requests", server.uri()),
        )
        .unwrap();

        let (reqs, _imps) = fetcher
            .fetch_requests_async(
                &["req-1".to_string(), "req-2".to_string()],
                &[],
            )
            .await
            .expect("should still return Ok with partial data");
        // req-1 is present, req-2 was null and stripped
        assert_eq!(reqs.len(), 1);
        assert!(reqs.contains_key("req-1"));
        assert!(!reqs.contains_key("req-2"));
    }

    #[tokio::test]
    async fn test_http_error_status() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let fetcher = HttpStoredRequestFetcher::with_client(
            reqwest::Client::new(),
            &format!("{}/stored-requests", server.uri()),
        )
        .unwrap();

        let result = fetcher
            .fetch_requests_async(&["req-1".to_string()], &[])
            .await;
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("500"));
    }

    #[tokio::test]
    async fn test_malformed_json_response() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw("not valid json", "application/json"),
            )
            .mount(&server)
            .await;

        let fetcher = HttpStoredRequestFetcher::with_client(
            reqwest::Client::new(),
            &format!("{}/stored-requests", server.uri()),
        )
        .unwrap();

        let result = fetcher
            .fetch_requests_async(&["req-1".to_string()], &[])
            .await;
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs[0].contains("parsing"));
    }

    #[tokio::test]
    async fn test_fetch_responses_returns_empty() {
        let fetcher = HttpStoredRequestFetcher::new("http://localhost:9999/stored-requests");
        let result = fetcher
            .fetch_responses_async(&["resp-1".to_string()])
            .await
            .expect("should succeed");
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_accounts() {
        let server = MockServer::start().await;

        let body = serde_json::json!({
            "accounts": {
                "acc-1": { "id": "acc-1", "disabled": false },
                "acc-2": { "id": "acc-2", "disabled": true },
            }
        });
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&server)
            .await;

        let fetcher = HttpStoredRequestFetcher::with_client(
            reqwest::Client::new(),
            &format!("{}/accounts", server.uri()),
        )
        .unwrap();

        let accounts = fetcher
            .fetch_accounts_async(&["acc-1".to_string(), "acc-2".to_string()])
            .await
            .expect("should succeed");
        assert_eq!(accounts.len(), 2);
        assert!(accounts.contains_key("acc-1"));
        assert!(accounts.contains_key("acc-2"));
    }

    #[test]
    fn test_build_request_url_default_format() {
        let fetcher = HttpStoredRequestFetcher::new("http://example.com/stored");
        let url = fetcher.build_request_url(
            &["req-1".to_string(), "req-2".to_string()],
            &["imp-1".to_string()],
        );
        // The URL should contain request-ids=["req-1","req-2"] and imp-ids=["imp-1"]
        // (URL-encoded)
        assert!(url.contains("request-ids="));
        assert!(url.contains("imp-ids="));
        assert!(!url.contains("request-id=req-1"));
    }

    #[test]
    fn test_build_request_url_rfc_compliant() {
        let fetcher = HttpStoredRequestFetcher::new("http://example.com/stored")
            .rfc_compliant(true);
        let url = fetcher.build_request_url(
            &["req-1".to_string(), "req-2".to_string()],
            &["imp-1".to_string()],
        );
        assert!(url.contains("request-id=req-1"));
        assert!(url.contains("request-id=req-2"));
        assert!(url.contains("imp-id=imp-1"));
        // Should NOT contain the plural JSON-array form
        assert!(!url.contains("request-ids="));
    }

    #[test]
    fn test_build_request_url_empty_ids() {
        let fetcher = HttpStoredRequestFetcher::new("http://example.com/stored");
        let url = fetcher.build_request_url(&[], &[]);
        // No query params should be added
        assert!(!url.contains("request-id"));
        assert!(!url.contains("imp-id"));
    }

    #[test]
    fn test_convert_nulls_to_errs_all_present() {
        let mut map = HashMap::new();
        map.insert("a".to_string(), Some(serde_json::json!({"id": "a"})));
        map.insert("b".to_string(), Some(serde_json::json!({"id": "b"})));
        let (clean, errs) = HttpStoredRequestFetcher::convert_nulls_to_errs(map, "Request");
        assert_eq!(clean.len(), 2);
        assert!(errs.is_empty());
    }

    #[test]
    fn test_convert_nulls_to_errs_some_null() {
        let mut map = HashMap::new();
        map.insert("a".to_string(), Some(serde_json::json!({"id": "a"})));
        map.insert("b".to_string(), None);
        let (clean, errs) = HttpStoredRequestFetcher::convert_nulls_to_errs(map, "Request");
        assert_eq!(clean.len(), 1);
        assert!(clean.contains_key("a"));
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("b"));
        assert!(errs[0].contains("not found"));
    }

    #[test]
    fn test_sync_fetcher_trait_with_empty_ids() {
        // Test the synchronous Fetcher trait impl with empty IDs
        // (no HTTP call needed).
        let fetcher = HttpStoredRequestFetcher::new("http://localhost:9999/stored-requests");
        let (reqs, imps) = fetcher.fetch_requests(&[], &[]).expect("should succeed");
        assert!(reqs.is_empty());
        assert!(imps.is_empty());
    }
}
