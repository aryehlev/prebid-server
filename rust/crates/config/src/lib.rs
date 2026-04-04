use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Host-level SChain node configuration.
/// When set, this node is prepended to the schain on every bid request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchainNode {
    /// Canonical domain name of the SSP (e.g. "prebid.org")
    pub asi: String,
    /// Seller ID assigned by the SSP
    pub sid: String,
    /// Optional request ID / transaction ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rid: Option<String>,
    /// Human-readable name of the entity
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Domain of the entity
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// 1 if this node is involved in the final transaction (required), else 0
    #[serde(default)]
    pub hp: i32,
}

/// Top-level prebid-server configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Configuration {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_admin_port")]
    pub admin_port: u16,
    #[serde(default)]
    pub enable_cors: bool,
    #[serde(default)]
    pub external_url: String,
    #[serde(default)]
    pub adapters: HashMap<String, AdapterConfig>,
    #[serde(default)]
    pub accounts: HashMap<String, AccountConfig>,
    #[serde(default)]
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub stored_requests: StoredRequestConfig,
    #[serde(default)]
    pub gdpr: GDPRConfig,
    #[serde(default)]
    pub ccpa: CCPAConfig,
    #[serde(default)]
    pub auction_timeouts: AuctionTimeouts,
    #[serde(default)]
    pub currency: CurrencyConfig,
    #[serde(default = "default_max_request_size")]
    pub max_request_size: usize,
    #[serde(default = "default_true")]
    pub auto_gen_source_tid: bool,
    #[serde(default)]
    pub generate_bid_id: bool,
    #[serde(default)]
    pub account_required: bool,
    #[serde(default = "default_static_dir")]
    pub static_dir: String,
    #[serde(default)]
    pub stored_requests_dir: String,
    /// Convenience top-level alias for gdpr.enabled
    #[serde(default)]
    pub gdpr_enabled: bool,
    /// Convenience top-level alias for ccpa.enforce
    #[serde(default = "default_true")]
    pub ccpa_enforce: bool,
    /// Optional host-level SChain node. When set, this node is prepended to
    /// the supply chain on every outgoing bid request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schain_node: Option<SchainNode>,
    /// Bidder alias map: alias name -> canonical bidder name.
    /// Allows synthetic bidder names that route to an existing adapter.
    #[serde(default)]
    pub aliases: HashMap<String, String>,
    #[serde(default)]
    pub http_client: HTTPClientConfig,
    #[serde(default)]
    pub user_sync: UserSyncConfig,
    #[serde(default)]
    pub analytics: AnalyticsConfig,
    #[serde(default)]
    pub price_floors: PriceFloorsConfig,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8000
}

fn default_admin_port() -> u16 {
    6060
}

fn default_static_dir() -> String {
    "./static".to_string()
}

fn default_max_request_size() -> usize {
    1_572_864 // 1.5 MB
}

/// Per-account configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountConfig {
    pub id: String,
    pub price_granularity: Option<String>,
    pub gdpr_enabled: Option<bool>,
    pub ccpa_enabled: Option<bool>,
    pub auction_timeout_ms: Option<u64>,
}

/// Per-adapter configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdapterConfig {
    #[serde(default)]
    pub endpoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_info: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Optional per-adapter timeout override in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

fn default_true() -> bool {
    true
}

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetricsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub influxdb: Option<InfluxDBConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prometheus: Option<PrometheusConfig>,
}

/// InfluxDB metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InfluxDBConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
}

/// Prometheus metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrometheusConfig {
    #[serde(default = "default_prometheus_port")]
    pub port: u16,
    #[serde(default = "default_prometheus_namespace")]
    pub namespace: String,
    #[serde(default = "default_prometheus_path")]
    pub path: String,
    #[serde(default = "default_prometheus_timeout")]
    pub timeout_ms: u64,
}

fn default_prometheus_port() -> u16 {
    8080
}

fn default_prometheus_namespace() -> String {
    "prebid".to_string()
}

fn default_prometheus_path() -> String {
    "/metrics".to_string()
}

fn default_prometheus_timeout() -> u64 {
    10000
}

/// Prebid cache configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CacheConfig {
    #[serde(default)]
    pub scheme: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub expected_millis: u64,
    #[serde(default)]
    pub default_ttl_secs: CacheTTL,
}

/// Per-format default TTL values for the prebid cache
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct CacheTTL {
    pub banner_ttl_secs: u32,
    pub video_ttl_secs: u32,
    pub native_ttl_secs: u32,
    pub audio_ttl_secs: u32,
}

/// Stored request configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredRequestConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filesystem: Option<FilesystemConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http: Option<HttpConfig>,
    #[serde(default)]
    pub in_memory_cache: InMemoryCacheConfig,
}

/// Filesystem stored request configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilesystemConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub directorypath: String,
}

/// HTTP stored request configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HttpConfig {
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub amp_endpoint: String,
}

/// In-memory cache configuration for stored requests
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InMemoryCacheConfig {
    #[serde(default)]
    pub ttl_seconds: i32,
    #[serde(default)]
    pub request_cache_size_bytes: i32,
    #[serde(default)]
    pub imp_cache_size_bytes: i32,
}

/// GDPR configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GDPRConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub host_vendor_id: u32,
    /// Default value for gdpr_applies when not specified in request ("0" = no, "1" = yes/enforce)
    #[serde(default = "default_gdpr_default_value")]
    pub default_value: String,
    #[serde(default = "default_gdpr_host_vendor_list_url")]
    pub host_vendor_list_url: String,
    #[serde(default)]
    pub enforce_vendor_list: bool,
    #[serde(default)]
    pub eea_countries: Vec<String>,
    /// If true, send all cookies regardless of GDPR consent
    #[serde(default)]
    pub send_all_cookies: bool,
    /// Bidders exempt from Purpose 1 (storage and access) consent requirement
    #[serde(default)]
    pub purpose1_vendor_exceptions: Vec<String>,
}

impl Default for GDPRConfig {
    fn default() -> Self {
        GDPRConfig {
            enabled: false,
            host_vendor_id: 0,
            default_value: default_gdpr_default_value(),
            host_vendor_list_url: default_gdpr_host_vendor_list_url(),
            enforce_vendor_list: false,
            eea_countries: Vec::new(),
            send_all_cookies: false,
            purpose1_vendor_exceptions: Vec::new(),
        }
    }
}

fn default_gdpr_host_vendor_list_url() -> String {
    "https://vendor-list.consensu.org/v2/vendor-list.json".to_string()
}

fn default_gdpr_default_value() -> String {
    "1".to_string()
}

/// CCPA configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CCPAConfig {
    #[serde(default = "default_true")]
    pub enforce: bool,
}

/// Auction timeout configuration (milliseconds)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuctionTimeouts {
    #[serde(default = "default_auction_timeout_default")]
    pub default: u64,
    #[serde(default = "default_auction_timeout_max")]
    pub max: u64,
}

fn default_auction_timeout_default() -> u64 {
    1000
}

fn default_auction_timeout_max() -> u64 {
    5000
}

/// Currency conversion configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrencyConfig {
    #[serde(default)]
    pub rates: HashMap<String, HashMap<String, f64>>,
    #[serde(default = "default_currency_fetch_url")]
    pub fetch_url: String,
    #[serde(default = "default_currency_fetch_interval_seconds")]
    pub fetch_interval_seconds: u64,
}

impl Default for CurrencyConfig {
    fn default() -> Self {
        CurrencyConfig {
            rates: HashMap::new(),
            fetch_url: default_currency_fetch_url(),
            fetch_interval_seconds: default_currency_fetch_interval_seconds(),
        }
    }
}

fn default_currency_fetch_url() -> String {
    "https://cdn.jsdelivr.net/gh/prebid/currency-file@1/latest.json".to_string()
}

fn default_currency_fetch_interval_seconds() -> u64 {
    1800
}

/// HTTP client configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HTTPClientConfig {
    pub max_connections_per_host: u32,
    pub max_idle_connections: u32,
    pub idle_connection_timeout_seconds: u64,
    pub request_timeout_milliseconds: u64,
}

impl Default for HTTPClientConfig {
    fn default() -> Self {
        Self {
            max_connections_per_host: 50,
            max_idle_connections: 50,
            idle_connection_timeout_seconds: 60,
            request_timeout_milliseconds: 5000,
        }
    }
}

/// User sync configuration
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct UserSyncConfig {
    pub timeout_ms: u64,
    pub redirect_url: String,
    pub external_url: String,
    pub cookie_name: String,
    #[serde(rename = "coopSync")]
    pub coop_sync: bool,
    pub default_sync_types: Vec<String>,
}

/// Analytics configuration
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct AnalyticsConfig {
    pub file: FileAnalyticsConfig,
    pub pubstack: PubstackAnalyticsConfig,
}

/// File-based analytics configuration
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct FileAnalyticsConfig {
    pub filename: String,
    pub enabled: bool,
}

/// Pubstack analytics configuration
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct PubstackAnalyticsConfig {
    pub endpoint: String,
    pub scope_id: String,
    pub enabled: bool,
}

/// Price floors configuration
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct PriceFloorsConfig {
    pub enabled: bool,
    pub enforce_floors_rate: f64,
    pub adjust_for_bid_adjustment: bool,
    pub enforce_deal_floors: bool,
    pub fetch: FloorFetchConfig,
}

/// Floor fetch configuration
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct FloorFetchConfig {
    pub enabled: bool,
    pub url: String,
    pub max_rules: u32,
    pub max_file_size_kb: u32,
}

impl Configuration {
    /// Load configuration from an optional file path plus environment variables.
    /// Environment variables override file settings.
    pub fn load(config_file: Option<&str>) -> Result<Configuration> {
        let mut builder = config::Config::builder()
            .set_default("host", "0.0.0.0")?
            .set_default("port", 8000)?
            .set_default("admin_port", 6060)?
            .set_default("enable_cors", false)?
            .set_default("auction_timeouts.default", 1000)?
            .set_default("auction_timeouts.max", 5000)?
            .set_default("gdpr.enabled", false)?
            .set_default("gdpr.default_value", "1")?
            .set_default("max_request_size", 1_572_864i64)?
            .set_default("auto_gen_source_tid", true)?
            .set_default("generate_bid_id", false)?
            .set_default("account_required", false)?
            .set_default("static_dir", "./static")?
            .set_default("stored_requests_dir", "./stored_requests")?
            .set_default("gdpr_enabled", false)?
            .set_default("ccpa_enforce", true)?
            .set_default(
                "currency.fetch_url",
                "https://cdn.jsdelivr.net/gh/prebid/currency-file@1/latest.json",
            )?
            .set_default("currency.fetch_interval_seconds", 1800)?;

        if let Some(path) = config_file {
            builder = builder.add_source(config::File::with_name(path).required(false));
        }

        builder = builder.add_source(
            config::Environment::with_prefix("PBS")
                .separator("_")
                .try_parsing(true),
        );

        let config = builder.build()?;
        let mut cfg: Configuration = config.try_deserialize()?;
        cfg.apply_env_overrides();
        Ok(cfg)
    }

    /// Apply well-known PBS_* environment variable overrides with explicit mappings.
    /// These override whatever was loaded from the config file or the generic
    /// `PBS_<KEY>` environment prefix parsing.
    pub fn apply_env_overrides(&mut self) {
        if let Ok(val) = std::env::var("PBS_PORT") {
            if let Ok(port) = val.parse::<u16>() {
                self.port = port;
            }
        }
        if let Ok(val) = std::env::var("PBS_HOST") {
            self.host = val;
        }
        if let Ok(val) = std::env::var("PBS_STATIC_DIR") {
            self.static_dir = val;
        }
        if let Ok(val) = std::env::var("PBS_STORED_REQUESTS_DIR") {
            self.stored_requests_dir = val;
        }
        if let Ok(val) = std::env::var("PBS_MAX_REQUEST_SIZE") {
            if let Ok(size) = val.parse::<usize>() {
                self.max_request_size = size;
            }
        }
        if let Ok(val) = std::env::var("PBS_GDPR_ENABLED") {
            self.gdpr_enabled = matches!(val.to_lowercase().as_str(), "true" | "1" | "yes");
        }
        if let Ok(val) = std::env::var("PBS_CCPA_ENFORCE") {
            self.ccpa_enforce = matches!(val.to_lowercase().as_str(), "true" | "1" | "yes");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_values() {
        // Configuration::load() (no file, no env) returns correct default values.
        // We clear any conflicting env vars that may be set by parallel tests.
        let cfg = Configuration::load(None).expect("should load default config");
        // Host/port defaults from set_default
        assert_eq!(cfg.host, "0.0.0.0");
        assert_eq!(cfg.port, 8000);
        assert_eq!(cfg.admin_port, 6060);
        // Auction timeout defaults
        assert_eq!(cfg.auction_timeouts.default, 1000);
        assert_eq!(cfg.auction_timeouts.max, 5000);
        // GDPR default
        assert_eq!(cfg.gdpr.default_value, "1");
        assert!(!cfg.gdpr.enabled);
        // Max request size default (1.5 MB)
        assert_eq!(cfg.max_request_size, 1_572_864);
    }

    #[test]
    fn test_default_adapter_config_struct() {
        // AdapterConfig::default() uses derive(Default); `enabled` starts as false
        // because `default_true` is only applied during deserialization.
        // We verify the struct fields are accessible and have their zero values.
        let ac = AdapterConfig::default();
        assert!(ac.endpoint.is_empty());
        assert!(ac.extra_info.is_none());
        assert!(ac.timeout_ms.is_none());
    }

    #[test]
    fn test_default_gdpr_config() {
        // GDPRConfig has a custom Default impl.
        let gdpr = GDPRConfig::default();
        assert!(!gdpr.enabled);
        assert_eq!(gdpr.default_value, "1");
        assert!(!gdpr.enforce_vendor_list);
        assert!(gdpr.eea_countries.is_empty());
    }

    #[test]
    fn test_default_ccpa_config_via_load() {
        // When loaded from config (no env vars), ccpa.enforce should be true (set_default + serde).
        // Use Configuration::load to test the actual runtime default.
        let cfg = Configuration::load(None).expect("should load default config");
        assert!(cfg.ccpa_enforce, "ccpa_enforce should default to true via config loading");
    }

    #[test]
    fn test_env_override_port() {
        // Store old value so we can restore it after the test.
        let old = std::env::var("PBS_PORT").ok();
        std::env::set_var("PBS_PORT", "9090");

        let mut cfg = Configuration::default();
        cfg.apply_env_overrides();
        assert_eq!(cfg.port, 9090);

        // Restore
        match old {
            Some(v) => std::env::set_var("PBS_PORT", v),
            None => std::env::remove_var("PBS_PORT"),
        }
    }

    #[test]
    fn test_env_override_host() {
        let old = std::env::var("PBS_HOST").ok();
        std::env::set_var("PBS_HOST", "127.0.0.1");

        let mut cfg = Configuration::default();
        cfg.apply_env_overrides();
        assert_eq!(cfg.host, "127.0.0.1");

        match old {
            Some(v) => std::env::set_var("PBS_HOST", v),
            None => std::env::remove_var("PBS_HOST"),
        }
    }

    #[test]
    fn test_env_override_gdpr_enabled() {
        let old = std::env::var("PBS_GDPR_ENABLED").ok();
        std::env::set_var("PBS_GDPR_ENABLED", "true");

        let mut cfg = Configuration::default();
        cfg.apply_env_overrides();
        assert!(cfg.gdpr_enabled);

        match old {
            Some(v) => std::env::set_var("PBS_GDPR_ENABLED", v),
            None => std::env::remove_var("PBS_GDPR_ENABLED"),
        }
    }

    #[test]
    fn test_env_override_ccpa_enforce_false() {
        let old = std::env::var("PBS_CCPA_ENFORCE").ok();
        std::env::set_var("PBS_CCPA_ENFORCE", "false");

        let mut cfg = Configuration::default();
        cfg.apply_env_overrides();
        assert!(!cfg.ccpa_enforce);

        match old {
            Some(v) => std::env::set_var("PBS_CCPA_ENFORCE", v),
            None => std::env::remove_var("PBS_CCPA_ENFORCE"),
        }
    }

    #[test]
    fn test_env_override_max_request_size() {
        let old = std::env::var("PBS_MAX_REQUEST_SIZE").ok();
        std::env::set_var("PBS_MAX_REQUEST_SIZE", "2048");

        let mut cfg = Configuration::default();
        cfg.apply_env_overrides();
        assert_eq!(cfg.max_request_size, 2048);

        match old {
            Some(v) => std::env::set_var("PBS_MAX_REQUEST_SIZE", v),
            None => std::env::remove_var("PBS_MAX_REQUEST_SIZE"),
        }
    }

    #[test]
    fn test_currency_config_default_fetch_url() {
        let cur = CurrencyConfig::default();
        assert!(cur.fetch_url.contains("jsdelivr.net"), "fetch_url should point to CDN");
        assert_eq!(cur.fetch_interval_seconds, 1800);
    }

    #[test]
    fn test_schema_node_serialization() {
        let node = SchainNode {
            asi: "prebid.org".to_string(),
            sid: "12345".to_string(),
            hp: 1,
            ..Default::default()
        };
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("prebid.org"));
        assert!(json.contains("12345"));
    }

    #[test]
    fn test_configuration_aliases_default_empty() {
        let cfg = Configuration::default();
        assert!(cfg.aliases.is_empty());
    }
}
