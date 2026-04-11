use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use tracing::{debug, warn};

/// PriceFloors holds floor configuration from req.ext.prebid.floors
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PriceFloors {
    #[serde(rename = "floorMin", default)]
    pub floor_min: f64,
    #[serde(rename = "floorMinCur", default)]
    pub floor_min_cur: String,
    pub data: Option<FloorData>,
    #[serde(rename = "enforcement", default)]
    pub enforcement: FloorEnforcement,
    #[serde(rename = "skipped", default)]
    pub skipped: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FloorData {
    pub currency: Option<String>,
    pub schema: Option<FloorSchema>,
    pub values: Option<HashMap<String, f64>>,
    pub modelgroups: Option<Vec<FloorModelGroup>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FloorSchema {
    pub fields: Vec<String>,
    pub delimiter: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FloorModelGroup {
    pub schema: Option<FloorSchema>,
    pub values: Option<HashMap<String, f64>>,
    #[serde(rename = "modelWeight", default)]
    pub model_weight: i32,
    #[serde(rename = "skipRate", default)]
    pub skip_rate: i32,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FloorEnforcement {
    #[serde(rename = "enforcePbs", default)]
    pub enforce_pbs: bool,
    #[serde(rename = "floorDeals", default)]
    pub floor_deals: bool,
    #[serde(rename = "enforceRate", default)]
    pub enforce_rate: i32,
}

/// Get the effective floor for an impression from req.ext.prebid.floors
/// Returns the floor price in the request currency (USD by default).
pub fn get_floor_for_imp(
    imp: &openrtb::Imp,
    request: &openrtb::BidRequest,
    floors: &PriceFloors,
) -> Option<f64> {
    if floors.skipped {
        return None;
    }

    // Use explicit imp.bidfloor if set
    if let Some(f) = imp.bidfloor.filter(|&f| f > 0.0) {
        // Apply floorMin
        if floors.floor_min > 0.0 {
            return Some(f.max(floors.floor_min));
        }
        return Some(f);
    }

    // Try schema-based lookup from floors.data
    if let Some(data) = &floors.data {
        if let Some(floor) = lookup_schema_floor(imp, request, data) {
            let min = floors.floor_min;
            return Some(if min > 0.0 { floor.max(min) } else { floor });
        }
    }

    // Fallback to floorMin only
    if floors.floor_min > 0.0 {
        return Some(floors.floor_min);
    }

    None
}

fn lookup_schema_floor(
    imp: &openrtb::Imp,
    request: &openrtb::BidRequest,
    data: &FloorData,
) -> Option<f64> {
    // Try each model group
    let groups = data.modelgroups.as_deref().unwrap_or(&[]);
    for group in groups {
        let schema = group.schema.as_ref().or(data.schema.as_ref())?;
        let values = group.values.as_ref().or(data.values.as_ref())?;
        let delimiter = schema.delimiter.as_deref().unwrap_or("|");

        let key = build_floor_key(&schema.fields, delimiter, imp, request);
        if let Some(&floor) = values.get(&key) {
            return Some(floor);
        }
        // Try wildcard
        let wildcard_key = schema
            .fields
            .iter()
            .map(|_| "*")
            .collect::<Vec<_>>()
            .join(delimiter);
        if let Some(&floor) = values.get(&wildcard_key) {
            return Some(floor);
        }
    }

    // Try top-level values
    if let Some(values) = &data.values {
        if let Some(schema) = &data.schema {
            let delimiter = schema.delimiter.as_deref().unwrap_or("|");
            let key = build_floor_key(&schema.fields, delimiter, imp, request);
            if let Some(&floor) = values.get(&key) {
                return Some(floor);
            }
            // Wildcard fallback for top-level values
            let wildcard_key = schema.fields.iter().map(|_| "*").collect::<Vec<_>>().join(delimiter);
            if let Some(&floor) = values.get(&wildcard_key) {
                return Some(floor);
            }
        }
    }

    None
}

fn build_floor_key(
    fields: &[String],
    delimiter: &str,
    imp: &openrtb::Imp,
    request: &openrtb::BidRequest,
) -> String {
    fields
        .iter()
        .map(|field| match field.as_str() {
            "siteDomain" | "pubDomain" | "domain" => request
                .site
                .as_ref()
                .and_then(|s| s.domain.as_deref())
                .unwrap_or("*")
                .to_string(),
            "bundle" => request
                .app
                .as_ref()
                .and_then(|a| a.bundle.as_deref())
                .unwrap_or("*")
                .to_string(),
            "channel" => request
                .site
                .as_ref()
                .and_then(|s| s.name.as_deref())
                .unwrap_or("*")
                .to_string(),
            "mediaType" => {
                if imp.banner.is_some() {
                    "banner".to_string()
                } else if imp.video.is_some() {
                    "video".to_string()
                } else if imp.native.is_some() {
                    "native".to_string()
                } else {
                    "*".to_string()
                }
            }
            "size" => imp
                .banner
                .as_ref()
                .and_then(|b| b.format.as_ref())
                .and_then(|f| f.first())
                .map(|f| format!("{}x{}", f.w.unwrap_or(0), f.h.unwrap_or(0)))
                .unwrap_or_else(|| "*".to_string()),
            "gptSlot" => imp
                .ext
                .as_ref()
                .and_then(|e| e.get("data"))
                .and_then(|d| d.get("adserver"))
                .and_then(|a| a.get("adslot"))
                .and_then(|v| v.as_str())
                .unwrap_or("*")
                .to_string(),
            _ => "*".to_string(),
        })
        .collect::<Vec<_>>()
        .join(delimiter)
}

// ---------------------------------------------------------------------------
// Floor Fetcher — async HTTP-based dynamic floor data retrieval
// ---------------------------------------------------------------------------

/// Configuration for the floor fetcher.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct FloorFetchConfig {
    /// URL to fetch floor data from.
    #[serde(default)]
    pub url: String,
    /// Refresh interval in seconds.
    #[serde(rename = "period", default = "default_period")]
    pub period_secs: u64,
    /// Maximum number of floor rules to accept.
    #[serde(rename = "maxRules", default = "default_max_rules")]
    pub max_rules: usize,
    /// Timeout for HTTP requests in milliseconds.
    #[serde(rename = "timeout", default = "default_fetch_timeout")]
    pub timeout_ms: u64,
    /// Maximum age (seconds) before cached data is considered stale.
    #[serde(rename = "maxAge", default = "default_max_age")]
    pub max_age_secs: u64,
    /// Whether fetching is enabled.
    #[serde(default)]
    pub enabled: bool,
}

fn default_period() -> u64 { 300 }
fn default_max_rules() -> usize { 1000 }
fn default_fetch_timeout() -> u64 { 5000 }
fn default_max_age() -> u64 { 86400 }

/// Holds cached floor data with a fetch timestamp.
#[derive(Debug, Clone)]
pub struct CachedFloorData {
    pub rules: PriceFloors,
    pub fetched_at: std::time::Instant,
}

/// Async floor data fetcher. Caches results in memory.
#[derive(Debug)]
pub struct FloorFetcher {
    pub config: FloorFetchConfig,
    cached: std::sync::RwLock<Option<CachedFloorData>>,
}

impl FloorFetcher {
    pub fn new(config: FloorFetchConfig) -> Self {
        Self {
            config,
            cached: std::sync::RwLock::new(None),
        }
    }

    /// Get cached floor data, or None if expired / not yet fetched.
    pub fn get_cached(&self) -> Option<PriceFloors> {
        let guard = self.cached.read().ok()?;
        let cached = guard.as_ref()?;
        let age = cached.fetched_at.elapsed().as_secs();
        if age > self.config.max_age_secs {
            return None;
        }
        Some(cached.rules.clone())
    }

    /// Store fetched floor data in cache.
    pub fn set_cached(&self, rules: PriceFloors) {
        if let Ok(mut guard) = self.cached.write() {
            *guard = Some(CachedFloorData {
                rules,
                fetched_at: std::time::Instant::now(),
            });
        }
    }

    /// Parse floor data from a JSON response body.
    pub fn parse_response(body: &[u8], max_rules: usize) -> Option<PriceFloors> {
        let mut floors: PriceFloors = serde_json::from_slice(body).ok()?;
        // Enforce max rules limit
        if let Some(data) = &mut floors.data {
            if let Some(groups) = &mut data.modelgroups {
                for group in groups.iter_mut() {
                    if let Some(values) = &mut group.values {
                        if values.len() > max_rules {
                            let keys: Vec<String> = values.keys().take(max_rules).cloned().collect();
                            let trimmed: HashMap<String, f64> = keys.into_iter()
                                .filter_map(|k| values.get(&k).map(|v| (k, *v)))
                                .collect();
                            *values = trimmed;
                        }
                    }
                }
            }
        }
        Some(floors)
    }
}

// ---------------------------------------------------------------------------
// FloorDataFetcher trait & implementations — async HTTP fetch with caching
// ---------------------------------------------------------------------------

/// Errors that can occur during floor data fetching.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("server returned non-success status {0}")]
    Status(u16),
    #[error("response body too large: {size} bytes (max {max})")]
    TooLarge { size: usize, max: usize },
    #[error("failed to parse floor JSON: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("fetched floor data contains no data field")]
    EmptyData,
    #[error("validation failed: {0}")]
    Validation(String),
}

/// Async trait for fetching floor data from a URL.
///
/// Implementations may perform HTTP requests, read from cache, or combine both.
#[async_trait]
pub trait FloorDataFetcher: Send + Sync + 'static {
    /// Fetch floor data from the given URL.
    async fn fetch_floor_data(&self, url: &str) -> Result<FloorData, FetchError>;
}

// ---------------------------------------------------------------------------
// HttpFloorFetcher — plain HTTP fetcher using reqwest
// ---------------------------------------------------------------------------

/// Fetches floor data over HTTP. No caching — every call hits the network.
///
/// Mirrors the core HTTP fetch in the Go `fetchFloorRulesFromURL` but uses
/// `reqwest` instead of the Go `http.Client`.
#[derive(Debug, Clone)]
pub struct HttpFloorFetcher {
    client: reqwest::Client,
    /// Maximum acceptable response body size in bytes.
    max_body_bytes: usize,
    /// Maximum number of floor rules per model group to retain.
    max_rules: usize,
}

impl HttpFloorFetcher {
    /// Create a new HTTP floor fetcher.
    ///
    /// * `timeout` — per-request timeout.
    /// * `max_body_bytes` — reject responses larger than this (0 = no limit).
    /// * `max_rules` — trim model group value maps that exceed this count.
    pub fn new(timeout: Duration, max_body_bytes: usize, max_rules: usize) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .expect("failed to build reqwest client");
        Self {
            client,
            max_body_bytes,
            max_rules,
        }
    }

    /// Build from an existing `reqwest::Client` (useful for testing / shared
    /// connection pools).
    pub fn with_client(
        client: reqwest::Client,
        max_body_bytes: usize,
        max_rules: usize,
    ) -> Self {
        Self {
            client,
            max_body_bytes,
            max_rules,
        }
    }

    /// Build from a [`FloorFetchConfig`].
    pub fn from_config(config: &FloorFetchConfig) -> Self {
        Self::new(
            Duration::from_millis(config.timeout_ms),
            0, // no body-size limit by default — caller can override
            config.max_rules,
        )
    }
}

#[async_trait]
impl FloorDataFetcher for HttpFloorFetcher {
    async fn fetch_floor_data(&self, url: &str) -> Result<FloorData, FetchError> {
        debug!(url, "fetching floor data");

        let resp = self.client.get(url).send().await?;

        let status = resp.status();
        if !status.is_success() {
            return Err(FetchError::Status(status.as_u16()));
        }

        let body = resp.bytes().await?;

        if self.max_body_bytes > 0 && body.len() > self.max_body_bytes {
            return Err(FetchError::TooLarge {
                size: body.len(),
                max: self.max_body_bytes,
            });
        }

        let mut data: FloorData = serde_json::from_slice(&body)?;

        // Enforce max-rules limit on every model group, mirroring Go
        // `validateRules` / `parse_response` logic.
        trim_floor_rules(&mut data, self.max_rules);

        debug!(url, "floor data fetched successfully");
        Ok(data)
    }
}

// ---------------------------------------------------------------------------
// CachedFloorFetcher — wraps any FloorDataFetcher with moka async cache
// ---------------------------------------------------------------------------

/// A floor data fetcher that keeps results in an async in-memory cache
/// ([`moka::future::Cache`]).
///
/// On a cache miss the inner fetcher is invoked and the result is stored.
/// On a cache hit the stored `FloorData` is returned immediately without
/// any network I/O.
///
/// The cache uses the request URL as key and respects `time_to_live` as the
/// maximum age for entries (analogous to Go's `maxAge`).
pub struct CachedFloorFetcher {
    inner: Arc<dyn FloorDataFetcher>,
    cache: moka::future::Cache<String, FloorData>,
}

impl std::fmt::Debug for CachedFloorFetcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedFloorFetcher")
            .field("cache_entry_count", &self.cache.entry_count())
            .finish()
    }
}

impl CachedFloorFetcher {
    /// Create a new cached fetcher.
    ///
    /// * `inner` — the underlying fetcher that performs the real work.
    /// * `max_entries` — maximum number of URLs to cache.
    /// * `time_to_live` — how long a fetched result is considered fresh.
    pub fn new(
        inner: Arc<dyn FloorDataFetcher>,
        max_entries: u64,
        time_to_live: Duration,
    ) -> Self {
        let cache = moka::future::Cache::builder()
            .max_capacity(max_entries)
            .time_to_live(time_to_live)
            .build();
        Self { inner, cache }
    }

    /// Build from a [`FloorFetchConfig`] and an inner fetcher.
    pub fn from_config(inner: Arc<dyn FloorDataFetcher>, config: &FloorFetchConfig) -> Self {
        Self::new(
            inner,
            1000, // sensible default for max cached URLs
            Duration::from_secs(config.max_age_secs),
        )
    }

    /// Invalidate the cached entry for the given URL.
    pub async fn invalidate(&self, url: &str) {
        self.cache.invalidate(url).await;
    }

    /// Return the number of entries currently in the cache.
    pub fn entry_count(&self) -> u64 {
        self.cache.entry_count()
    }
}

#[async_trait]
impl FloorDataFetcher for CachedFloorFetcher {
    async fn fetch_floor_data(&self, url: &str) -> Result<FloorData, FetchError> {
        // Fast path: check if the value is already cached.
        if let Some(data) = self.cache.get(url).await {
            debug!(url, "returning cached floor data");
            return Ok(data);
        }

        // Cache miss — delegate to the inner fetcher.
        debug!(url, "cache miss, fetching floor data from origin");
        match self.inner.fetch_floor_data(url).await {
            Ok(data) => {
                self.cache.insert(url.to_owned(), data.clone()).await;
                Ok(data)
            }
            Err(e) => {
                warn!(url, error = %e, "floor data fetch failed");
                Err(e)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Trim model group value maps so that no group exceeds `max_rules`.
fn trim_floor_rules(data: &mut FloorData, max_rules: usize) {
    if max_rules == 0 {
        return;
    }
    if let Some(groups) = &mut data.modelgroups {
        for group in groups.iter_mut() {
            if let Some(values) = &mut group.values {
                if values.len() > max_rules {
                    let trimmed: HashMap<String, f64> = values
                        .iter()
                        .take(max_rules)
                        .map(|(k, v)| (k.clone(), *v))
                        .collect();
                    *values = trimmed;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Floor enforcement — check bids against floors
// ---------------------------------------------------------------------------

/// Check if a bid meets the floor price. Returns true if the bid is valid.
pub fn bid_meets_floor(bid_price: f64, floor: f64, is_deal: bool, enforce_deals: bool) -> bool {
    if bid_price <= 0.0 {
        return false;
    }
    // Deal bids may be exempt from floor enforcement
    if is_deal && !enforce_deals {
        return true;
    }
    bid_price >= floor
}

/// Determine if floor enforcement should be skipped based on skip rate.
/// Returns true if enforcement should be skipped.
///
/// Mirrors Go `isSatisfiedByEnforceRate`.
pub fn should_skip_enforcement(skip_rate: i32) -> bool {
    if skip_rate <= 0 {
        return false;
    }
    if skip_rate >= 100 {
        return true;
    }
    let roll = rand::random::<i32>().rem_euclid(100);
    roll < skip_rate
}

/// Check if enforcement should proceed based on enforce rate configuration.
///
/// Mirrors Go `isSatisfiedByEnforceRate` which uses both request-level
/// and config-level enforce rates. Enforcement proceeds if a random value
/// is below the minimum of the two rates.
pub fn is_satisfied_by_enforce_rate(request_rate: i32, config_rate: i32) -> bool {
    let effective_rate = if request_rate > 0 && config_rate > 0 {
        request_rate.min(config_rate)
    } else if request_rate > 0 {
        request_rate
    } else if config_rate > 0 {
        config_rate
    } else {
        return true; // No rate configured, always enforce
    };

    if effective_rate >= 100 {
        return true;
    }
    if effective_rate <= 0 {
        return false;
    }

    let roll = rand::random::<i32>().rem_euclid(100);
    roll < effective_rate
}

/// Full floor enforcement on seat bids.
///
/// Returns (accepted_bids, errors, rejected_bids).
/// Mirrors Go `enforceFloorToBids`.
pub fn enforce_floor_to_bids(
    request: &openrtb::BidRequest,
    floors: &PriceFloors,
    bids: &[(String, f64, Option<String>)], // (imp_id, price, deal_id)
    enforce_deals: bool,
) -> (Vec<usize>, Vec<String>, Vec<usize>) {
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    let mut errors = Vec::new();

    for (i, (imp_id, price, deal_id)) in bids.iter().enumerate() {
        // Find the impression's floor
        let imp = request.imp.iter().find(|imp| imp.id == *imp_id);
        let floor = imp
            .and_then(|imp| get_floor_for_imp(imp, request, floors))
            .unwrap_or(0.0);

        if floor <= 0.0 {
            accepted.push(i);
            continue;
        }

        let is_deal = deal_id.as_ref().map_or(false, |d| !d.is_empty());
        if bid_meets_floor(*price, floor, is_deal, enforce_deals) {
            accepted.push(i);
        } else {
            rejected.push(i);
            errors.push(format!(
                "bid rejected: price {:.4} below floor {:.4} for imp {}",
                price, floor, imp_id
            ));
        }
    }

    (accepted, errors, rejected)
}

/// Apply floor rules to a bid request — set imp.bidfloor for each impression.
pub fn apply_floors_to_request(
    request: &mut openrtb::BidRequest,
    floors: &PriceFloors,
) {
    if floors.skipped {
        return;
    }

    let req_snapshot = request.clone();
    for imp in &mut request.imp {
        if let Some(floor) = get_floor_for_imp(imp, &req_snapshot, floors) {
            imp.bidfloor = Some(floor);
            if !floors.floor_min_cur.is_empty() {
                imp.bidfloorcur = Some(floors.floor_min_cur.clone());
            } else if let Some(data) = &floors.data {
                if let Some(cur) = &data.currency {
                    imp.bidfloorcur = Some(cur.clone());
                }
            }
        }
    }
}

/// Extract PriceFloors from req.ext.prebid.floors if present.
pub fn extract_floors_from_request(req: &openrtb::BidRequest) -> Option<PriceFloors> {
    let ext = req.ext.as_ref()?;
    let prebid = ext.get("prebid")?;
    let floors_val = prebid.get("floors")?;
    serde_json::from_value(floors_val.clone()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_banner_imp(id: &str, bidfloor: Option<f64>) -> openrtb::Imp {
        openrtb::Imp {
            id: id.to_string(),
            bidfloor,
            banner: Some(openrtb::Banner::default()),
            ..Default::default()
        }
    }

    fn make_request() -> openrtb::BidRequest {
        openrtb::BidRequest {
            id: "test".to_string(),
            imp: vec![],
            site: Some(openrtb::Site {
                domain: Some("example.com".to_string()),
                name: Some("mychannel".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn test_skipped_floors_returns_none() {
        let imp = make_banner_imp("imp1", Some(1.5));
        let req = make_request();
        let floors = PriceFloors {
            skipped: true,
            floor_min: 2.0,
            ..Default::default()
        };
        assert_eq!(get_floor_for_imp(&imp, &req, &floors), None);
    }

    #[test]
    fn test_bidfloor_used_when_set() {
        let imp = make_banner_imp("imp1", Some(1.5));
        let req = make_request();
        let floors = PriceFloors::default();
        assert_eq!(get_floor_for_imp(&imp, &req, &floors), Some(1.5));
    }

    #[test]
    fn test_floor_min_applied_over_bidfloor() {
        let imp = make_banner_imp("imp1", Some(0.5));
        let req = make_request();
        let floors = PriceFloors {
            floor_min: 1.0,
            ..Default::default()
        };
        assert_eq!(get_floor_for_imp(&imp, &req, &floors), Some(1.0));
    }

    #[test]
    fn test_schema_floor_lookup_by_media_type() {
        let imp = make_banner_imp("imp1", None);
        let req = make_request();

        let mut values = HashMap::new();
        values.insert("banner".to_string(), 2.5_f64);

        let floors = PriceFloors {
            data: Some(FloorData {
                schema: Some(FloorSchema {
                    fields: vec!["mediaType".to_string()],
                    delimiter: None,
                }),
                values: Some(values),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(get_floor_for_imp(&imp, &req, &floors), Some(2.5));
    }

    #[test]
    fn test_schema_wildcard_fallback() {
        let imp = make_banner_imp("imp1", None);
        let req = make_request();

        let mut values = HashMap::new();
        values.insert("*".to_string(), 1.0_f64);

        let floors = PriceFloors {
            data: Some(FloorData {
                schema: Some(FloorSchema {
                    fields: vec!["mediaType".to_string()],
                    delimiter: None,
                }),
                values: Some(values),
                ..Default::default()
            }),
            ..Default::default()
        };
        // "banner" not found, falls through wildcard
        assert_eq!(get_floor_for_imp(&imp, &req, &floors), Some(1.0));
    }

    #[test]
    fn test_floor_min_fallback_when_no_imp_floor() {
        let imp = make_banner_imp("imp1", None);
        let req = make_request();
        let floors = PriceFloors {
            floor_min: 0.75,
            ..Default::default()
        };
        assert_eq!(get_floor_for_imp(&imp, &req, &floors), Some(0.75));
    }

    #[test]
    fn test_no_floor_returns_none() {
        let imp = make_banner_imp("imp1", None);
        let req = make_request();
        let floors = PriceFloors::default();
        assert_eq!(get_floor_for_imp(&imp, &req, &floors), None);
    }

    #[test]
    fn test_build_floor_key_multi_field() {
        let imp = make_banner_imp("imp1", None);
        let req = make_request();
        let fields = vec!["siteDomain".to_string(), "mediaType".to_string()];
        let key = build_floor_key(&fields, "|", &imp, &req);
        assert_eq!(key, "example.com|banner");
    }

    #[test]
    fn test_bid_meets_floor() {
        assert!(bid_meets_floor(2.0, 1.5, false, false));
        assert!(!bid_meets_floor(1.0, 1.5, false, false));
        assert!(bid_meets_floor(1.0, 1.5, true, false)); // deal exempt
        assert!(!bid_meets_floor(1.0, 1.5, true, true)); // deal enforced
        assert!(!bid_meets_floor(0.0, 1.0, false, false));
    }

    #[test]
    fn test_should_skip_enforcement() {
        assert!(!should_skip_enforcement(0));
        assert!(should_skip_enforcement(100));
    }

    #[test]
    fn test_apply_floors_to_request() {
        let mut req = make_request();
        req.imp = vec![make_banner_imp("imp1", None)];
        let floors = PriceFloors {
            floor_min: 1.0,
            floor_min_cur: "USD".to_string(),
            ..Default::default()
        };
        apply_floors_to_request(&mut req, &floors);
        assert_eq!(req.imp[0].bidfloor, Some(1.0));
        assert_eq!(req.imp[0].bidfloorcur.as_deref(), Some("USD"));
    }

    #[test]
    fn test_extract_floors_from_request() {
        let req = openrtb::BidRequest {
            ext: Some(serde_json::json!({
                "prebid": {
                    "floors": {
                        "floorMin": 0.5,
                        "enforcement": { "enforcePbs": true }
                    }
                }
            })),
            ..Default::default()
        };
        let floors = extract_floors_from_request(&req).unwrap();
        assert_eq!(floors.floor_min, 0.5);
        assert!(floors.enforcement.enforce_pbs);
    }

    #[test]
    fn test_floor_fetcher_cache() {
        let config = FloorFetchConfig {
            url: "http://example.com/floors".to_string(),
            max_age_secs: 300,
            enabled: true,
            ..Default::default()
        };
        let fetcher = FloorFetcher::new(config);
        assert!(fetcher.get_cached().is_none());

        let floors = PriceFloors { floor_min: 2.0, ..Default::default() };
        fetcher.set_cached(floors);
        let cached = fetcher.get_cached().unwrap();
        assert_eq!(cached.floor_min, 2.0);
    }

    #[test]
    fn test_parse_floor_response() {
        let json = serde_json::json!({
            "floorMin": 1.0,
            "data": {
                "modelgroups": [{
                    "values": { "banner": 2.0, "video": 3.0 },
                    "schema": { "fields": ["mediaType"] }
                }]
            }
        });
        let body = serde_json::to_vec(&json).unwrap();
        let floors = FloorFetcher::parse_response(&body, 100).unwrap();
        assert_eq!(floors.floor_min, 1.0);
        assert!(floors.data.is_some());
    }

    // -----------------------------------------------------------------------
    // FloorDataFetcher trait / HttpFloorFetcher / CachedFloorFetcher tests
    // -----------------------------------------------------------------------

    /// A mock fetcher that returns a pre-defined FloorData or an error.
    #[derive(Clone)]
    struct MockFloorFetcher {
        data: Result<FloorData, String>,
        call_count: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl MockFloorFetcher {
        fn ok(data: FloorData) -> Self {
            Self {
                data: Ok(data),
                call_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            }
        }

        fn err(msg: &str) -> Self {
            Self {
                data: Err(msg.to_string()),
                call_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            }
        }

        fn calls(&self) -> usize {
            self.call_count.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl FloorDataFetcher for MockFloorFetcher {
        async fn fetch_floor_data(&self, _url: &str) -> Result<FloorData, FetchError> {
            self.call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            match &self.data {
                Ok(d) => Ok(d.clone()),
                Err(msg) => Err(FetchError::Validation(msg.clone())),
            }
        }
    }

    fn sample_floor_data() -> FloorData {
        let mut values = HashMap::new();
        values.insert("banner".to_string(), 1.5);
        values.insert("video".to_string(), 3.0);
        FloorData {
            currency: Some("USD".to_string()),
            schema: Some(FloorSchema {
                fields: vec!["mediaType".to_string()],
                delimiter: None,
            }),
            values: Some(values),
            modelgroups: None,
        }
    }

    #[tokio::test]
    async fn test_mock_fetcher_returns_data() {
        let data = sample_floor_data();
        let fetcher = MockFloorFetcher::ok(data.clone());
        let result = fetcher
            .fetch_floor_data("http://example.com/floors")
            .await;
        assert!(result.is_ok());
        let fetched = result.unwrap();
        assert_eq!(fetched.currency.as_deref(), Some("USD"));
        assert_eq!(fetcher.calls(), 1);
    }

    #[tokio::test]
    async fn test_mock_fetcher_returns_error() {
        let fetcher = MockFloorFetcher::err("boom");
        let result = fetcher
            .fetch_floor_data("http://example.com/floors")
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FetchError::Validation(_)));
    }

    #[tokio::test]
    async fn test_cached_fetcher_caches_result() {
        let data = sample_floor_data();
        let mock = Arc::new(MockFloorFetcher::ok(data));
        let mock_ref = mock.clone();

        let cached = CachedFloorFetcher::new(
            mock.clone() as Arc<dyn FloorDataFetcher>,
            10,
            Duration::from_secs(300),
        );

        // First call -> cache miss -> delegates to inner
        let r1 = cached
            .fetch_floor_data("http://example.com/floors")
            .await
            .unwrap();
        assert_eq!(r1.currency.as_deref(), Some("USD"));
        assert_eq!(mock_ref.calls(), 1);

        // Second call -> cache hit -> no additional inner call
        let r2 = cached
            .fetch_floor_data("http://example.com/floors")
            .await
            .unwrap();
        assert_eq!(r2.currency.as_deref(), Some("USD"));
        assert_eq!(mock_ref.calls(), 1); // still 1
    }

    #[tokio::test]
    async fn test_cached_fetcher_does_not_cache_errors() {
        let mock = Arc::new(MockFloorFetcher::err("transient"));
        let mock_ref = mock.clone();

        let cached = CachedFloorFetcher::new(
            mock.clone() as Arc<dyn FloorDataFetcher>,
            10,
            Duration::from_secs(300),
        );

        let r1 = cached
            .fetch_floor_data("http://example.com/floors")
            .await;
        assert!(r1.is_err());

        let r2 = cached
            .fetch_floor_data("http://example.com/floors")
            .await;
        assert!(r2.is_err());

        // Inner was called twice because errors are not cached.
        assert_eq!(mock_ref.calls(), 2);
    }

    #[tokio::test]
    async fn test_cached_fetcher_invalidate() {
        let data = sample_floor_data();
        let mock = Arc::new(MockFloorFetcher::ok(data));
        let mock_ref = mock.clone();

        let cached = CachedFloorFetcher::new(
            mock.clone() as Arc<dyn FloorDataFetcher>,
            10,
            Duration::from_secs(300),
        );

        // Populate cache
        let _ = cached
            .fetch_floor_data("http://example.com/floors")
            .await
            .unwrap();
        assert_eq!(mock_ref.calls(), 1);

        // Invalidate
        cached.invalidate("http://example.com/floors").await;

        // Next call should miss the cache
        let _ = cached
            .fetch_floor_data("http://example.com/floors")
            .await
            .unwrap();
        assert_eq!(mock_ref.calls(), 2);
    }

    #[test]
    fn test_trim_floor_rules() {
        let mut values = HashMap::new();
        for i in 0..10 {
            values.insert(format!("key{}", i), i as f64);
        }
        let mut data = FloorData {
            modelgroups: Some(vec![FloorModelGroup {
                values: Some(values),
                ..Default::default()
            }]),
            ..Default::default()
        };

        trim_floor_rules(&mut data, 3);
        let group = &data.modelgroups.as_ref().unwrap()[0];
        assert_eq!(group.values.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn test_trim_floor_rules_zero_max_is_noop() {
        let mut values = HashMap::new();
        values.insert("a".to_string(), 1.0);
        values.insert("b".to_string(), 2.0);
        let mut data = FloorData {
            modelgroups: Some(vec![FloorModelGroup {
                values: Some(values),
                ..Default::default()
            }]),
            ..Default::default()
        };

        trim_floor_rules(&mut data, 0);
        let group = &data.modelgroups.as_ref().unwrap()[0];
        assert_eq!(group.values.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_http_floor_fetcher_from_config() {
        let config = FloorFetchConfig {
            url: "http://example.com/floors".to_string(),
            timeout_ms: 3000,
            max_rules: 500,
            enabled: true,
            ..Default::default()
        };
        let fetcher = HttpFloorFetcher::from_config(&config);
        assert_eq!(fetcher.max_rules, 500);
        assert_eq!(fetcher.max_body_bytes, 0);
    }

    #[tokio::test]
    async fn test_http_floor_fetcher_with_wiremock() {
        use wiremock::{Mock, MockServer, ResponseTemplate};
        use wiremock::matchers::method;

        let server = MockServer::start().await;

        let floor_json = serde_json::json!({
            "currency": "USD",
            "modelgroups": [{
                "values": { "banner": 2.0, "video": 4.0 },
                "schema": { "fields": ["mediaType"] },
                "modelWeight": 50
            }]
        });

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&floor_json))
            .mount(&server)
            .await;

        let fetcher = HttpFloorFetcher::new(Duration::from_secs(5), 0, 1000);
        let result = fetcher.fetch_floor_data(&server.uri()).await;
        assert!(result.is_ok(), "fetch failed: {:?}", result.err());

        let data = result.unwrap();
        assert_eq!(data.currency.as_deref(), Some("USD"));
        let groups = data.modelgroups.as_ref().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].values.as_ref().unwrap().get("banner"), Some(&2.0));
    }

    #[tokio::test]
    async fn test_http_floor_fetcher_non_200_returns_error() {
        use wiremock::{Mock, MockServer, ResponseTemplate};
        use wiremock::matchers::method;

        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let fetcher = HttpFloorFetcher::new(Duration::from_secs(5), 0, 1000);
        let result = fetcher.fetch_floor_data(&server.uri()).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FetchError::Status(500)));
    }

    #[tokio::test]
    async fn test_http_floor_fetcher_body_too_large() {
        use wiremock::{Mock, MockServer, ResponseTemplate};
        use wiremock::matchers::method;

        let server = MockServer::start().await;

        // Return a valid JSON body that exceeds the 10-byte limit.
        let body = serde_json::json!({ "currency": "USD" });
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&server)
            .await;

        let fetcher = HttpFloorFetcher::new(Duration::from_secs(5), 10, 1000);
        let result = fetcher.fetch_floor_data(&server.uri()).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FetchError::TooLarge { .. }));
    }

    #[tokio::test]
    async fn test_http_floor_fetcher_trims_rules() {
        use wiremock::{Mock, MockServer, ResponseTemplate};
        use wiremock::matchers::method;

        let server = MockServer::start().await;

        // 5 rules, but max_rules = 2
        let floor_json = serde_json::json!({
            "modelgroups": [{
                "values": {
                    "banner": 1.0,
                    "video": 2.0,
                    "native": 3.0,
                    "audio": 4.0,
                    "other": 5.0
                },
                "schema": { "fields": ["mediaType"] }
            }]
        });

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&floor_json))
            .mount(&server)
            .await;

        let fetcher = HttpFloorFetcher::new(Duration::from_secs(5), 0, 2);
        let data = fetcher.fetch_floor_data(&server.uri()).await.unwrap();
        let groups = data.modelgroups.unwrap();
        assert_eq!(groups[0].values.as_ref().unwrap().len(), 2);
    }
}
