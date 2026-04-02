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

/// GDPR configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GDPRConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub host_vendor_id: u32,
    #[serde(default = "default_gdpr_default_value")]
    pub default_value: String,
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

impl Configuration {
    /// Load configuration from an optional file path plus environment variables.
    /// Environment variables override file settings.
    pub fn load(config_file: Option<&str>) -> Result<Configuration> {
        let mut builder = config::Config::builder()
            .set_default("host", "0.0.0.0")?
            .set_default("port", 8000)?
            .set_default("admin_port", 6060)?
            .set_default("enable_cors", false)?;

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
