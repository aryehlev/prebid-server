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
