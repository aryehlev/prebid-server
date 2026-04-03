use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

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
    #[serde(default)]
    pub max_request_size: i64,
    #[serde(default = "default_true")]
    pub auto_gen_source_tid: bool,
    #[serde(default)]
    pub generate_bid_id: bool,
    #[serde(default)]
    pub account_required: bool,
    #[serde(default = "default_static_dir")]
    pub static_dir: String,
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

/// Per-adapter configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdapterConfig {
    #[serde(default)]
    pub endpoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_info: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
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
    #[serde(default = "default_gdpr_default_value")]
    pub default_value: String,
    #[serde(default)]
    pub enforce_vendor_list: bool,
    #[serde(default)]
    pub eea_countries: Vec<String>,
}

impl Default for GDPRConfig {
    fn default() -> Self {
        GDPRConfig {
            enabled: false,
            host_vendor_id: 0,
            default_value: default_gdpr_default_value(),
            enforce_vendor_list: false,
            eea_countries: Vec::new(),
        }
    }
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
            .set_default("max_request_size", 0)?
            .set_default("auto_gen_source_tid", true)?
            .set_default("generate_bid_id", false)?
            .set_default("account_required", false)?
            .set_default("static_dir", "./static")?
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
        let cfg: Configuration = config.try_deserialize()?;
        Ok(cfg)
    }
}
