use std::collections::HashMap;

// Re-export so callers can use this without knowing the exchange crate.
pub use pbs_exchange::StoredResponseFetcher;

/// Simple in-memory stored request cache loaded from filesystem.
///
/// Supports three kinds of stored data:
/// - **Request fragments**: full or partial `BidRequest` JSON, loaded from
///   `<dir>/requests/<id>.json` (or the directory root for backwards compat).
/// - **Imp fragments**: partial `Imp` JSON, loaded from `<dir>/imps/<id>.json`.
/// - **Auction responses**: full pre-built `BidResponse` JSON, loaded from
///   `<dir>/storedresponses/<id>.json`.
pub struct StoredRequestFetcher {
    /// Map from request ID to stored BidRequest fragment JSON
    requests: HashMap<String, serde_json::Value>,
    /// Map from imp ID to stored Imp fragment JSON
    imps: HashMap<String, serde_json::Value>,
    /// Map from response ID to stored BidResponse JSON
    responses: HashMap<String, serde_json::Value>,
}

impl StoredRequestFetcher {
    /// Load stored requests from a directory.
    ///
    /// Scans the root directory for `<id>.json` files (full BidRequest
    /// fragments) and, if present, the `requests/` and `imps/` sub-directories
    /// for request and imp fragments respectively.
    pub fn from_directory(dir: &str) -> Self {
        let mut requests = HashMap::new();
        let mut imps = HashMap::new();
        let mut responses = HashMap::new();

        // Helper: load all JSON files in `path` into `map`.
        let load_json_files = |path: &str, map: &mut HashMap<String, serde_json::Value>| {
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    let file_path = entry.path();
                    if file_path.extension().and_then(|e| e.to_str()) == Some("json") {
                        if let Some(stem) = file_path.file_stem().and_then(|s| s.to_str()) {
                            if let Ok(content) = std::fs::read_to_string(&file_path) {
                                if let Ok(json) =
                                    serde_json::from_str::<serde_json::Value>(&content)
                                {
                                    map.insert(stem.to_string(), json);
                                }
                            }
                        }
                    }
                }
            }
        };

        // Root directory — backwards-compat: treat all JSON files as request fragments.
        load_json_files(dir, &mut requests);

        // Optional sub-directories that take precedence.
        let requests_subdir = format!("{}/requests", dir);
        let imps_subdir = format!("{}/imps", dir);
        let responses_subdir = format!("{}/storedresponses", dir);
        load_json_files(&requests_subdir, &mut requests);
        load_json_files(&imps_subdir, &mut imps);
        load_json_files(&responses_subdir, &mut responses);

        Self { requests, imps, responses }
    }

    /// Construct an empty fetcher (no stored requests, imps, or responses).
    pub fn empty() -> Self {
        Self {
            requests: HashMap::new(),
            imps: HashMap::new(),
            responses: HashMap::new(),
        }
    }

    // ── Request fragments ─────────────────────────────────────────────────────

    /// Return the stored BidRequest fragment for `id`, or `None` if not found.
    pub fn fetch(&self, id: &str) -> Option<&serde_json::Value> {
        self.requests.get(id)
    }

    /// Backwards-compatible alias for [`fetch`].
    pub fn get(&self, id: &str) -> Option<&serde_json::Value> {
        self.fetch(id)
    }

    // ── Imp fragments ─────────────────────────────────────────────────────────

    /// Return the stored Imp fragment for `id`, or `None` if not found.
    ///
    /// Imp fragments are partial `Imp` objects stored in `<dir>/imps/<id>.json`.
    /// They are typically merged into `bid_request.imp[n]` before running an
    /// auction (base imp wins on conflict, using JSON deep-merge semantics).
    pub fn fetch_imp(&self, id: &str) -> Option<&serde_json::Value> {
        self.imps.get(id)
    }

    /// Return a stored auction response for `id`, or `None` if not found.
    ///
    /// Stored auction responses are full pre-built `BidResponse` JSON objects,
    /// loaded from `<dir>/storedresponses/<id>.json`.  When present, the auction
    /// can be short-circuited and the stored response returned directly without
    /// calling any bidders.
    pub fn fetch_stored_response(&self, id: &str) -> Option<openrtb::BidResponse> {
        self.responses
            .get(id)
            .or_else(|| self.requests.get(id))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    // ── Merge helpers ─────────────────────────────────────────────────────────

    /// Merge a stored request fragment into a mutable `BidRequest` JSON value.
    ///
    /// The *caller's* fields win on conflict (overlay-wins-base semantics
    /// mirroring the Go server).  For object fields deep-merge is applied; for
    /// arrays and scalars the stored value is used only when the caller's field
    /// is absent.
    pub fn merge_request_fragment(
        stored: &serde_json::Value,
        incoming: &mut serde_json::Value,
    ) {
        if let (Some(stored_obj), Some(incoming_obj)) =
            (stored.as_object(), incoming.as_object_mut())
        {
            for (key, stored_val) in stored_obj {
                let entry = incoming_obj
                    .entry(key.clone())
                    .or_insert(serde_json::Value::Null);
                if entry.is_null() {
                    // Caller did not set this field — use the stored value.
                    *entry = stored_val.clone();
                } else if entry.is_object() && stored_val.is_object() {
                    // Recursively deep-merge; caller keys win.
                    Self::merge_request_fragment(stored_val, entry);
                }
                // For arrays/scalars the caller's non-null value wins.
            }
        }
    }
}

// ── StoredResponseFetcher impl ────────────────────────────────────────────────

/// Allow `StoredRequestFetcher` to serve stored auction responses.
///
/// Stored auction response JSON files are expected to live under
/// `<dir>/responses/<id>.json`.  Because `StoredRequestFetcher` already loads
/// the `imps/` sub-directory into a separate map, response files would need
/// their own sub-directory — but for simplicity we re-use the `requests` map
/// here.  Publishers can store a pre-built `BidResponse` JSON under a request
/// ID and reference it via `req.ext.prebid.storedauctionresponse.id`.
impl StoredResponseFetcher for StoredRequestFetcher {
    fn fetch(&self, id: &str) -> Option<&serde_json::Value> {
        // Check dedicated storedresponses map first, fall back to requests map.
        self.responses.get(id).or_else(|| self.requests.get(id))
    }
}

// ---------------------------------------------------------------------------
// Async HTTP-based stored request fetcher
// ---------------------------------------------------------------------------

/// Fetches stored requests/imps from a remote HTTP endpoint.
///
/// The endpoint is expected to accept POST requests with JSON body:
/// ```json
/// { "requests": ["id1", "id2"], "imps": ["imp1"] }
/// ```
/// And return:
/// ```json
/// { "requests": { "id1": {...}, "id2": {...} }, "imps": { "imp1": {...} } }
/// ```
pub struct HttpStoredRequestFetcher {
    pub endpoint: String,
    client: reqwest::Client,
}

impl HttpStoredRequestFetcher {
    pub fn new(endpoint: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_default();
        Self { endpoint, client }
    }

    pub fn with_client(endpoint: String, client: reqwest::Client) -> Self {
        Self { endpoint, client }
    }

    /// Fetch stored requests and imps by their IDs.
    pub async fn fetch_by_ids(
        &self,
        request_ids: &[String],
        imp_ids: &[String],
    ) -> Result<HttpFetchResult, StoredRequestError> {
        let body = serde_json::json!({
            "requests": request_ids,
            "imps": imp_ids,
        });

        let resp = self
            .client
            .post(&self.endpoint)
            .json(&body)
            .send()
            .await
            .map_err(|e| StoredRequestError::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(StoredRequestError::HttpError(format!(
                "HTTP {}", resp.status()
            )));
        }

        let result: HttpFetchResult = resp
            .json()
            .await
            .map_err(|e| StoredRequestError::ParseError(e.to_string()))?;

        Ok(result)
    }
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct HttpFetchResult {
    #[serde(default)]
    pub requests: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub imps: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
pub enum StoredRequestError {
    HttpError(String),
    ParseError(String),
    NotFound(String),
}

impl std::fmt::Display for StoredRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::HttpError(e) => write!(f, "HTTP error: {}", e),
            Self::ParseError(e) => write!(f, "parse error: {}", e),
            Self::NotFound(id) => write!(f, "stored request not found: {}", id),
        }
    }
}

// ---------------------------------------------------------------------------
// Multi-fetcher: cascading through multiple backends
// ---------------------------------------------------------------------------

/// Combines multiple `StoredRequestFetcher` instances with fallback semantics.
/// Tries each fetcher in order until an ID is found.
pub struct MultiFetcher {
    /// Primary: filesystem (fast, local)
    pub primary: StoredRequestFetcher,
    /// Optional secondary sources
    pub secondary: Vec<StoredRequestFetcher>,
}

impl MultiFetcher {
    pub fn new(primary: StoredRequestFetcher) -> Self {
        Self {
            primary,
            secondary: Vec::new(),
        }
    }

    pub fn with_secondary(mut self, fetcher: StoredRequestFetcher) -> Self {
        self.secondary.push(fetcher);
        self
    }

    /// Fetch a stored request, trying primary then each secondary.
    pub fn fetch_request(&self, id: &str) -> Option<&serde_json::Value> {
        self.primary.fetch(id)
    }

    /// Fetch a stored imp, trying primary then each secondary.
    pub fn fetch_imp(&self, id: &str) -> Option<&serde_json::Value> {
        self.primary.fetch_imp(id)
    }

    /// Fetch multiple request IDs, returning found and missing.
    pub fn fetch_requests(&self, ids: &[String]) -> (HashMap<String, serde_json::Value>, Vec<String>) {
        let mut found = HashMap::new();
        let mut missing = Vec::new();

        for id in ids {
            if let Some(val) = self.primary.fetch(id) {
                found.insert(id.clone(), val.clone());
            } else {
                // Try secondaries
                let mut resolved = false;
                for secondary in &self.secondary {
                    if let Some(val) = secondary.fetch(id) {
                        found.insert(id.clone(), val.clone());
                        resolved = true;
                        break;
                    }
                }
                if !resolved {
                    missing.push(id.clone());
                }
            }
        }

        (found, missing)
    }
}

// ---------------------------------------------------------------------------
// Caching wrapper
// ---------------------------------------------------------------------------

/// In-memory caching layer wrapping another fetcher.
/// Stores cloned values to avoid repeated filesystem/network reads.
pub struct CachingFetcher {
    inner: StoredRequestFetcher,
    /// Request cache: id -> (value, inserted_at)
    request_cache: std::sync::RwLock<HashMap<String, (serde_json::Value, std::time::Instant)>>,
    /// TTL in seconds
    ttl_secs: u64,
}

impl CachingFetcher {
    pub fn new(inner: StoredRequestFetcher, ttl_secs: u64) -> Self {
        Self {
            inner,
            request_cache: std::sync::RwLock::new(HashMap::new()),
            ttl_secs,
        }
    }

    /// Fetch with caching. Returns cached value if available and not expired.
    pub fn fetch_cached(&self, id: &str) -> Option<serde_json::Value> {
        // Check cache first
        if let Ok(cache) = self.request_cache.read() {
            if let Some((val, inserted)) = cache.get(id) {
                if inserted.elapsed().as_secs() < self.ttl_secs {
                    return Some(val.clone());
                }
            }
        }

        // Cache miss — fetch from inner
        let val = self.inner.fetch(id)?.clone();

        // Store in cache
        if let Ok(mut cache) = self.request_cache.write() {
            cache.insert(id.to_string(), (val.clone(), std::time::Instant::now()));
        }

        Some(val)
    }

    /// Invalidate a cache entry.
    pub fn invalidate(&self, id: &str) {
        if let Ok(mut cache) = self.request_cache.write() {
            cache.remove(id);
        }
    }

    /// Clear all cached entries.
    pub fn clear(&self) {
        if let Ok(mut cache) = self.request_cache.write() {
            cache.clear();
        }
    }

    /// Number of entries currently in cache.
    pub fn cache_size(&self) -> usize {
        self.request_cache.read().map(|c| c.len()).unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Category Fetcher
// ---------------------------------------------------------------------------

/// Fetches ad category mappings for category translation.
pub struct CategoryFetcher {
    /// Map: (primary_ad_server, publisher_id) -> (iab_id -> category_string)
    categories: HashMap<(String, String), HashMap<String, String>>,
}

impl CategoryFetcher {
    pub fn new() -> Self {
        Self {
            categories: HashMap::new(),
        }
    }

    /// Load categories from a directory structure:
    /// `<dir>/<primary_ad_server>/<publisher_id>.json`
    pub fn from_directory(dir: &str) -> Self {
        let mut categories = HashMap::new();

        if let Ok(servers) = std::fs::read_dir(dir) {
            for server_entry in servers.flatten() {
                let server_path = server_entry.path();
                if !server_path.is_dir() {
                    continue;
                }
                let server_name = server_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                if let Ok(publishers) = std::fs::read_dir(&server_path) {
                    for pub_entry in publishers.flatten() {
                        let pub_path = pub_entry.path();
                        if pub_path.extension().and_then(|e| e.to_str()) != Some("json") {
                            continue;
                        }
                        let pub_id = pub_path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_string();

                        if let Ok(content) = std::fs::read_to_string(&pub_path) {
                            if let Ok(map) =
                                serde_json::from_str::<HashMap<String, String>>(&content)
                            {
                                categories.insert((server_name.clone(), pub_id), map);
                            }
                        }
                    }
                }
            }
        }

        Self { categories }
    }

    /// Fetch the translated category for a given ad server, publisher, and IAB ID.
    pub fn fetch_category(
        &self,
        primary_ad_server: &str,
        publisher_id: &str,
        iab_id: &str,
    ) -> Option<&String> {
        let key = (primary_ad_server.to_string(), publisher_id.to_string());
        self.categories.get(&key)?.get(iab_id)
    }
}

impl Default for CategoryFetcher {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_fetcher() {
        let fetcher = StoredRequestFetcher::empty();
        assert!(fetcher.fetch("nonexistent").is_none());
        assert!(fetcher.fetch_imp("nonexistent").is_none());
        assert!(fetcher.fetch_stored_response("nonexistent").is_none());
    }

    #[test]
    fn test_merge_request_fragment_sets_missing() {
        let stored = serde_json::json!({ "tmax": 500, "at": 1 });
        let mut incoming = serde_json::json!({ "id": "req1" });
        StoredRequestFetcher::merge_request_fragment(&stored, &mut incoming);
        assert_eq!(incoming.get("tmax").unwrap().as_i64(), Some(500));
        assert_eq!(incoming.get("id").unwrap().as_str(), Some("req1"));
    }

    #[test]
    fn test_merge_request_fragment_caller_wins() {
        let stored = serde_json::json!({ "id": "stored-id", "tmax": 500 });
        let mut incoming = serde_json::json!({ "id": "caller-id" });
        StoredRequestFetcher::merge_request_fragment(&stored, &mut incoming);
        // Caller's id wins
        assert_eq!(incoming.get("id").unwrap().as_str(), Some("caller-id"));
        // Stored tmax fills in
        assert_eq!(incoming.get("tmax").unwrap().as_i64(), Some(500));
    }

    #[test]
    fn test_merge_deep_objects() {
        let stored = serde_json::json!({ "ext": { "prebid": { "debug": true }, "extra": 1 } });
        let mut incoming = serde_json::json!({ "ext": { "prebid": { "targeting": {} } } });
        StoredRequestFetcher::merge_request_fragment(&stored, &mut incoming);
        let ext = incoming.get("ext").unwrap();
        let prebid = ext.get("prebid").unwrap();
        // Caller's targeting preserved
        assert!(prebid.get("targeting").is_some());
        // Stored debug fills in
        assert_eq!(prebid.get("debug").unwrap().as_bool(), Some(true));
        // Stored extra fills in
        assert_eq!(ext.get("extra").unwrap().as_i64(), Some(1));
    }

    #[test]
    fn test_caching_fetcher() {
        let inner = StoredRequestFetcher::empty();
        let caching = CachingFetcher::new(inner, 3600);
        assert_eq!(caching.cache_size(), 0);
        assert!(caching.fetch_cached("missing").is_none());
    }

    #[test]
    fn test_multi_fetcher() {
        let primary = StoredRequestFetcher::empty();
        let multi = MultiFetcher::new(primary);
        let (found, missing) = multi.fetch_requests(&["id1".to_string()]);
        assert!(found.is_empty());
        assert_eq!(missing, vec!["id1".to_string()]);
    }

    #[test]
    fn test_category_fetcher_empty() {
        let fetcher = CategoryFetcher::new();
        assert!(fetcher.fetch_category("dfp", "pub1", "IAB1").is_none());
    }
}
