//! Top-level `Configuration` struct for multi-source loading.
//!
//! This module mirrors the Go `config.Configuration` type in
//! `config/config.go`. It is intentionally lenient: every struct uses
//! `#[serde(default)]` and every field is either an `Option<T>` or a
//! primitive that is zero-valued by default. This allows partial YAML /
//! environment overrides to merge cleanly.
//!
//! Note: the existing `crate::Configuration` remains untouched at the crate
//! root. The new type lives under `crate::top::Configuration` and is used by
//! the new `loader`, `defaults`, and `validation` modules.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Top-level prebid-server configuration (new multi-source variant).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Configuration {
    pub host: String,
    pub port: u16,
    pub admin_port: u16,
    pub external_url: String,
    pub enable_gzip: bool,
    pub garbage_collector_threshold: i64,
    pub status_response: String,
    pub datacenter: String,

    pub currency: Currency,
    pub stored_requests: StoredRequests,
    pub metrics: Metrics,
    pub analytics: Analytics,
    pub gdpr: Gdpr,
    pub ccpa: Ccpa,
    pub lmt: Lmt,
    pub privacy: Privacy,
    pub host_cookie: HostCookie,
    pub cookie_sync: CookieSync,
    pub price_floors: PriceFloors,
    pub debug: Debug,
    pub experiment: Experiment,
    pub bidder_infos: BidderInfos,
}

// ---------------------------------------------------------------------------
// Currency
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Currency {
    pub fetch_url: String,
    pub fetch_interval_seconds: i64,
    pub stale_rates_seconds: i64,
    pub default_currency: Option<String>,
}

// ---------------------------------------------------------------------------
// Stored requests
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StoredRequests {
    pub backend: StoredRequestsBackend,
    pub cache_events_api: bool,
    pub http_events: Option<HttpEvents>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StoredRequestsBackend {
    /// One of: "none", "file", "postgres", "http", "memory".
    pub r#type: String,
    pub file: Option<FileBackend>,
    pub http: Option<HttpBackend>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FileBackend {
    pub path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HttpBackend {
    pub endpoint: String,
    pub amp_endpoint: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HttpEvents {
    pub endpoint: String,
    pub amp_endpoint: Option<String>,
    pub refresh_rate_seconds: i64,
    pub timeout_ms: i64,
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Metrics {
    pub influxdb: Option<InfluxMetrics>,
    pub prometheus: Option<PrometheusMetrics>,
    pub disabled_metrics: DisabledMetrics,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct InfluxMetrics {
    pub host: String,
    pub database: String,
    pub measurement: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub align_timestamps: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PrometheusMetrics {
    pub port: u16,
    pub namespace: Option<String>,
    pub subsystem: Option<String>,
    pub timeout_ms: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DisabledMetrics {
    pub account_adapter_details: bool,
    pub account_debug: bool,
    pub account_stored_responses: bool,
    pub adapter_connections_metrics: bool,
    pub adapter_gdpr_request_blocked: bool,
}

// ---------------------------------------------------------------------------
// Analytics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Analytics {
    pub file: Option<FileLogs>,
    pub pubstack: Option<Pubstack>,
    pub agma: Option<AgmaAnalytics>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FileLogs {
    pub filename: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Pubstack {
    pub enabled: bool,
    pub endpoint: String,
    pub scopeid: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AgmaAnalytics {
    pub enabled: bool,
    pub endpoint: String,
}

// ---------------------------------------------------------------------------
// GDPR / CCPA / LMT / Privacy
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Gdpr {
    pub enabled: bool,
    /// Must be "0" or "1" — this is intentionally a string matching Go.
    pub default_value: String,
    pub host_vendor_id: i32,
    pub timeouts_ms: TimeoutsMs,
    pub non_standard_publishers: Vec<String>,
    pub tcf2: Tcf2Config,
    pub amp_exception: bool,
    pub eea_countries: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TimeoutsMs {
    pub init_vendorlist_fetches: i64,
    pub active_vendorlist_fetch: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Tcf2Config {
    pub enabled: bool,
    pub purpose_one_treatment: Option<PurposeOneTreatment>,
    pub special_feature1: Option<SpecialFeature1>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PurposeOneTreatment {
    pub enabled: bool,
    pub access_allowed: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SpecialFeature1 {
    pub enforce: bool,
    pub vendor_exceptions: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Ccpa {
    pub enforce: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Lmt {
    pub enforce: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Privacy {
    pub ipv4: IpMasking,
    pub ipv6: IpMasking,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct IpMasking {
    pub anon_keep_bits: i32,
}

// ---------------------------------------------------------------------------
// Host cookie / cookie sync
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HostCookie {
    pub enabled: bool,
    pub domain: String,
    pub family: String,
    pub cookie_name: String,
    pub opt_out_url: String,
    pub opt_in_url: String,
    pub max_cookie_size_bytes: i64,
    pub ttl_days: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CookieSync {
    pub default_limit: i32,
    pub max_limit: i32,
    pub default_coop_sync: bool,
    pub pri: Vec<String>,
}

// ---------------------------------------------------------------------------
// Price floors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PriceFloors {
    pub enabled: bool,
    pub fetcher: PriceFloorFetcher,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PriceFloorFetcher {
    pub http_client_timeout_seconds: i64,
    pub max_retries: i32,
    pub cache_size_mb: i64,
    pub worker: i32,
}

// ---------------------------------------------------------------------------
// Debug / Experiment
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Debug {
    pub timeout_notification: TimeoutNotification,
    pub override_token: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TimeoutNotification {
    pub log: bool,
    pub sampling_rate: f64,
    pub fail_only: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Experiment {
    pub adscert: AdsCert,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AdsCert {
    pub mode: String,
    pub in_process: Option<AdsCertInProcess>,
    pub remote: Option<AdsCertRemote>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AdsCertInProcess {
    pub origin: String,
    pub key: String,
    pub domain_check_interval_seconds: i64,
    pub domain_renewal_interval_seconds: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AdsCertRemote {
    pub url: String,
    pub signing_timeout_ms: i64,
}

// ---------------------------------------------------------------------------
// Bidder infos
// ---------------------------------------------------------------------------

/// Free-form map of bidder-code -> bidder info. The full `BidderInfo` struct
/// lives in `crate::bidder_info`; for top-level loading we use a loose map so
/// merging can happen generically without double-binding.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BidderInfos {
    pub inner: HashMap<String, serde_json::Value>,
}
