pub mod account;
pub mod account_fetcher;
pub mod bidder_info;
pub mod bidder_info_loader;
pub mod bidder_params_loader;
pub mod env_config;
pub mod static_embed;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

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
    pub http_client_cache: HTTPClientConfig,
    #[serde(default)]
    pub user_sync: UserSyncConfig,
    #[serde(default)]
    pub analytics: AnalyticsConfig,
    #[serde(default)]
    pub price_floors: PriceFloorsConfig,
    #[serde(default)]
    pub lmt: LmtConfig,
    /// Tmax adjustment settings for estimating bidder tmax.
    #[serde(default)]
    pub tmax_adjustments: TmaxAdjustments,
    /// Default tmax value when not specified in the request.
    #[serde(default)]
    pub tmax_default: u64,
    /// Video endpoint configuration.
    #[serde(default)]
    pub video: VideoConfig,
    /// Hooks configuration for hook execution plans.
    #[serde(default)]
    pub hooks: HooksConfig,
    /// Bid response validation settings.
    #[serde(default)]
    pub validations: ValidationsConfig,
    /// Experimental feature flags.
    #[serde(default)]
    pub experiment: ExperimentConfig,
    /// Default request configuration.
    #[serde(default)]
    pub default_request: DefReqConfig,
    /// Host cookie configuration.
    #[serde(default)]
    pub host_cookie: HostCookieConfig,
    /// External cache URL configuration.
    #[serde(default)]
    pub external_cache: ExternalCacheConfig,
    /// Debug / logging configuration.
    #[serde(default)]
    pub debug: DebugConfig,
    /// Request validation (private network CIDRs).
    #[serde(default)]
    pub request_validation: RequestValidationConfig,
    /// Request timeout headers configuration.
    #[serde(default)]
    pub request_timeout_headers: RequestTimeoutHeaders,
    /// Compression configuration.
    #[serde(default)]
    pub compression: CompressionConfig,
    /// Admin listener configuration.
    #[serde(default)]
    pub admin: AdminConfig,
    /// VTrack configuration.
    #[serde(default)]
    pub vtrack: VTrackConfig,
    /// Event configuration.
    #[serde(default)]
    pub event: EventConfig,
    /// Unix socket enable flag.
    #[serde(default)]
    pub unix_socket_enable: bool,
    /// Unix socket file name.
    #[serde(default)]
    pub unix_socket_name: String,
    /// Status response string for /status endpoint.
    #[serde(default)]
    pub status_response: String,
    /// Recaptcha secret.
    #[serde(default)]
    pub recaptcha_secret: String,
    /// AMP timeout adjustment in milliseconds.
    #[serde(default)]
    pub amp_timeout_adjustment_ms: i64,
    /// Whether video stored request is required.
    #[serde(default)]
    pub video_stored_request_required: bool,
    /// Blocked app bundle IDs.
    #[serde(default)]
    pub blocked_apps: Vec<String>,
    /// Account defaults applied to all accounts.
    #[serde(default)]
    pub account_defaults: AccountConfig,
    /// Path to PEM certificates file.
    #[serde(default)]
    pub certificates_file: String,
    /// When true, a new bid ID is generated in seatbid[].bid[].ext.prebid.bidid.
    #[serde(default)]
    pub generate_request_id: bool,
    /// Data center identifier.
    #[serde(default)]
    pub datacenter: String,
    /// Stored AMP request configuration.
    #[serde(default)]
    pub stored_amp_req: StoredRequestConfig,
    /// Category mapping configuration.
    #[serde(default)]
    pub category_mapping: StoredRequestConfig,
    /// Stored video request configuration.
    #[serde(default)]
    pub stored_video_req: StoredRequestConfig,
    /// Stored responses configuration.
    #[serde(default)]
    pub stored_responses: StoredRequestConfig,
    /// Stored requests timeout in milliseconds.
    #[serde(default = "default_stored_requests_timeout_ms")]
    pub stored_requests_timeout_ms: u64,
    /// Garbage collector threshold in bytes (Go-specific, kept for config compat).
    #[serde(default)]
    pub garbage_collector_threshold: u64,
    /// Debug override token. When set, requests that send this value in the
    /// `x-pbs-debug-override` header can enable debug output regardless of
    /// the account `debug_allow` flag.
    #[serde(default)]
    pub debug_override_token: Option<String>,
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

fn default_stored_requests_timeout_ms() -> u64 {
    50
}

// ---------------------------------------------------------------------------
// TmaxAdjustments
// ---------------------------------------------------------------------------

/// Tmax adjustment settings for estimating bidder tmax.
///
/// Maps to the Go `TmaxAdjustments` struct in config/config.go.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TmaxAdjustments {
    /// Whether bidder tmax should be calculated and passed to bid adapters.
    #[serde(default)]
    pub enabled: bool,
    /// Minimum expected response duration from a bidder in milliseconds.
    #[serde(default)]
    pub bidder_response_duration_min_ms: u64,
    /// Network latency buffer between PBS and bidder servers in milliseconds.
    #[serde(default)]
    pub bidder_network_latency_buffer_ms: u64,
    /// Time required for PBS to process all bidder responses in milliseconds.
    #[serde(default)]
    pub pbs_response_preparation_duration_ms: u64,
}

// ---------------------------------------------------------------------------
// VideoConfig
// ---------------------------------------------------------------------------

/// Video endpoint configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VideoConfig {
    /// When true, the deprecated /video endpoint is enabled.
    #[serde(default)]
    pub enable_deprecated_endpoint: bool,
}

// ---------------------------------------------------------------------------
// HooksConfig
// ---------------------------------------------------------------------------

/// Hook execution plan configuration.
///
/// Maps to the Go `Hooks` struct in config/hooks.go.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HooksConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Module configuration: map[vendor_name] -> map[module_name] -> config.
    #[serde(default)]
    pub modules: HashMap<String, HashMap<String, JsonValue>>,
    /// Host execution plan (always executed).
    #[serde(default)]
    pub host_execution_plan: HookExecutionPlan,
    /// Default account execution plan (can be overridden per-account).
    #[serde(default)]
    pub default_account_execution_plan: HookExecutionPlan,
}

/// Hook execution plan: endpoints -> stages -> groups.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HookExecutionPlan {
    #[serde(default)]
    pub endpoints: HashMap<String, HookEndpointPlan>,
}

/// Per-endpoint hook stages.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HookEndpointPlan {
    #[serde(default)]
    pub stages: HashMap<String, HookStagePlan>,
}

/// Per-stage hook execution groups.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HookStagePlan {
    #[serde(default)]
    pub groups: Vec<HookExecutionGroup>,
}

/// A group of hooks to execute with a shared timeout.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HookExecutionGroup {
    /// Timeout in milliseconds. Zero means immediate timeout status.
    #[serde(default)]
    pub timeout: u64,
    /// Ordered sequence of hooks to execute.
    #[serde(default)]
    pub hook_sequence: Vec<HookSequenceEntry>,
}

/// A single hook in a hook execution sequence.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HookSequenceEntry {
    /// Composite value: "{vendor_name}.{module_name}".
    #[serde(default)]
    pub module_code: String,
    /// Arbitrary identifier used for metrics and debug info.
    #[serde(default)]
    pub hook_impl_code: String,
}

// ---------------------------------------------------------------------------
// ValidationsConfig
// ---------------------------------------------------------------------------

/// Bid response validation settings.
///
/// Maps to the Go `Validations` struct in config/config.go.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidationsConfig {
    /// Banner creative max size enforcement: "skip", "warn", or "enforce".
    #[serde(default)]
    pub banner_creative_max_size: String,
    /// Secure markup enforcement: "skip", "warn", or "enforce".
    #[serde(default)]
    pub secure_markup: String,
    /// Maximum allowed creative width (0 = no limit).
    #[serde(default)]
    pub max_creative_width: i64,
    /// Maximum allowed creative height (0 = no limit).
    #[serde(default)]
    pub max_creative_height: i64,
}

// ---------------------------------------------------------------------------
// ExperimentConfig
// ---------------------------------------------------------------------------

/// Experimental feature flags.
///
/// Maps to the Go `Experiment` struct in config/experiment.go.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExperimentConfig {
    /// Ads Cert configuration.
    #[serde(default)]
    pub adscert: AdsCertConfig,
}

/// Ads Cert configuration for signing bid requests.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdsCertConfig {
    /// Mode: "off", "inprocess", or "remote".
    #[serde(default)]
    pub mode: String,
    /// In-process signing configuration.
    #[serde(default)]
    pub inprocess: AdsCertInProcessConfig,
    /// Remote signing service configuration.
    #[serde(default)]
    pub remote: AdsCertRemoteConfig,
}

/// In-process ads cert signing configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdsCertInProcessConfig {
    /// Ads.cert hostname for the originating party.
    #[serde(default)]
    pub origin: String,
    /// Base64-encoded private key.
    #[serde(default)]
    pub key: String,
    /// Frequency in seconds to check DNS _delivery._adscert and _adscert subdomains.
    #[serde(default = "default_ads_cert_dns_check_interval")]
    pub domain_check_interval_seconds: u64,
    /// Frequency in seconds to renew DNS _delivery._adscert and _adscert subdomains.
    #[serde(default = "default_ads_cert_dns_renewal_interval")]
    pub domain_renewal_interval_seconds: u64,
}

fn default_ads_cert_dns_check_interval() -> u64 {
    30
}

fn default_ads_cert_dns_renewal_interval() -> u64 {
    30
}

/// Remote ads cert signing service configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdsCertRemoteConfig {
    /// Address of gRPC server that creates call signatures.
    #[serde(default)]
    pub url: String,
    /// Timeout in milliseconds for the signing operation.
    #[serde(default = "default_ads_cert_signing_timeout")]
    pub signing_timeout_ms: u64,
}

fn default_ads_cert_signing_timeout() -> u64 {
    5
}

// ---------------------------------------------------------------------------
// DefReqConfig
// ---------------------------------------------------------------------------

/// Default request configuration.
///
/// Maps to the Go `DefReqConfig` struct in config/config.go.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DefReqConfig {
    /// Type of default request source (e.g. "file").
    #[serde(default, rename = "type")]
    pub req_type: String,
    /// Filesystem-based default request settings.
    #[serde(default)]
    pub file: DefReqFiles,
    /// Whether alias info should be included.
    #[serde(default)]
    pub alias_info: bool,
}

/// Filesystem default request file settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DefReqFiles {
    /// Path to the default request JSON file.
    #[serde(default)]
    pub name: String,
}

// ---------------------------------------------------------------------------
// HostCookieConfig
// ---------------------------------------------------------------------------

/// Host cookie configuration.
///
/// Maps to the Go `HostCookie` struct in config/config.go.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HostCookieConfig {
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub cookie_name: String,
    #[serde(default)]
    pub opt_out_url: String,
    #[serde(default)]
    pub opt_in_url: String,
    /// Maximum cookie size in bytes. 0 = unlimited.
    #[serde(default)]
    pub max_cookie_size_bytes: u64,
    /// Opt-out cookie settings.
    #[serde(default)]
    pub optout_cookie: CookieConfig,
    /// Cookie TTL in days.
    #[serde(default = "default_host_cookie_ttl_days")]
    pub ttl_days: i64,
}

fn default_host_cookie_ttl_days() -> i64 {
    90
}

/// Simple cookie name/value pair.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CookieConfig {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub value: String,
}

// ---------------------------------------------------------------------------
// ExternalCacheConfig
// ---------------------------------------------------------------------------

/// External cache URL configuration.
///
/// Maps to the Go `ExternalCache` struct in config/config.go.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExternalCacheConfig {
    #[serde(default)]
    pub scheme: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub path: String,
}

// ---------------------------------------------------------------------------
// DebugConfig
// ---------------------------------------------------------------------------

/// Debug / logging configuration.
///
/// Maps to the Go `Debug` struct in config/config.go.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DebugConfig {
    #[serde(default)]
    pub timeout_notification: TimeoutNotificationConfig,
    #[serde(default)]
    pub override_token: String,
}

/// Timeout notification logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimeoutNotificationConfig {
    /// Log timeout notifications in the application log.
    #[serde(default)]
    pub log: bool,
    /// Fraction of notifications to log (0.0 .. 1.0).
    #[serde(default)]
    pub sampling_rate: f32,
    /// Only log failures.
    #[serde(default)]
    pub fail_only: bool,
}

// ---------------------------------------------------------------------------
// RequestValidationConfig
// ---------------------------------------------------------------------------

/// Request validation configuration (private network CIDRs).
///
/// Maps to the Go `RequestValidation` struct in config/requestvalidation.go.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequestValidationConfig {
    /// IPv4 private network CIDRs.
    #[serde(default)]
    pub ipv4_private_networks: Vec<String>,
    /// IPv6 private network CIDRs.
    #[serde(default)]
    pub ipv6_private_networks: Vec<String>,
}

// ---------------------------------------------------------------------------
// RequestTimeoutHeaders
// ---------------------------------------------------------------------------

/// Custom headers to handle request timeouts from queueing infrastructure.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequestTimeoutHeaders {
    #[serde(default)]
    pub request_time_in_queue: String,
    #[serde(default)]
    pub request_timeout_in_queue: String,
}

// ---------------------------------------------------------------------------
// CompressionConfig
// ---------------------------------------------------------------------------

/// Compression configuration.
///
/// Maps to the Go `Compression` struct in config/compression.go.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompressionConfig {
    #[serde(default)]
    pub request: CompressionInfoConfig,
    #[serde(default)]
    pub response: CompressionInfoConfig,
}

/// Per-direction compression settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompressionInfoConfig {
    #[serde(default)]
    pub enable_gzip: bool,
}

// ---------------------------------------------------------------------------
// AdminConfig
// ---------------------------------------------------------------------------

/// Admin listener configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdminConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// VTrackConfig
// ---------------------------------------------------------------------------

/// Video tracking configuration.
///
/// Maps to the Go `VTrack` struct in config/config.go.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VTrackConfig {
    #[serde(default = "default_vtrack_timeout")]
    pub timeout_ms: i64,
    #[serde(default = "default_true")]
    pub allow_unknown_bidder: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_vtrack_timeout() -> i64 {
    2000
}

// ---------------------------------------------------------------------------
// EventConfig
// ---------------------------------------------------------------------------

/// Event timeout configuration.
///
/// Maps to the Go `Event` struct in config/config.go.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EventConfig {
    #[serde(default = "default_event_timeout")]
    pub timeout_ms: i64,
}

fn default_event_timeout() -> i64 {
    1000
}

/// Per-account configuration with full publisher-level settings.
///
/// Maps to the Go `Account` struct in config/account.go. Each publisher can
/// override host-level defaults for privacy, price floors, bid adjustments, etc.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountConfig {
    /// Publisher account ID.
    #[serde(default)]
    pub id: String,
    /// When true, requests for this account are rejected.
    #[serde(default)]
    pub disabled: bool,
    /// Legacy field kept for backward compat; prefer the richer sub-configs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_granularity: Option<String>,
    /// Default integration channel (e.g. "web", "app", "amp").
    #[serde(default)]
    pub default_integration: String,
    /// Account-level event tracking configuration.
    #[serde(default)]
    pub events: AccountEventsConfig,
    /// Whether events are enabled (optional override).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events_enabled: Option<bool>,
    /// Account-level privacy overrides.
    #[serde(default)]
    pub privacy: AccountPrivacyConfig,
    /// Account-level price floors configuration.
    #[serde(default)]
    pub price_floors: AccountPriceFloorsConfig,
    /// Account-level bid adjustments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bid_adjustments: Option<BidAdjustmentsConfig>,
    /// Account-level auction timeout override in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auction_timeout_ms: Option<u64>,
    /// Allow debug/test requests for this account.
    #[serde(default)]
    pub debug_allow: bool,
    /// Cache TTL defaults inherited from host cache config.
    #[serde(default)]
    pub cache_ttl: CacheTTL,
    /// Cookie sync configuration.
    #[serde(default)]
    pub cookie_sync: AccountCookieSyncConfig,
    /// Truncate targeting attribute to this length.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncate_target_attr: Option<i32>,
    /// Account-level hooks configuration.
    #[serde(default)]
    pub hooks: AccountHooksConfig,
    /// Account-level validations.
    #[serde(default)]
    pub validations: ValidationsConfig,
    /// Default bid limit per imp.
    #[serde(default)]
    pub default_bid_limit: u32,
    /// Bid rounding mode: "down", "true", "timesplit", "up".
    #[serde(default)]
    pub bid_rounding: String,
    /// Targeting key prefix override for this account.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub targeting_prefix: Option<String>,
    /// Account-level module configuration: map[vendor_name] -> map[module_name] -> raw JSON.
    #[serde(default)]
    pub modules: HashMap<String, HashMap<String, JsonValue>>,
}

// ---------------------------------------------------------------------------
// Account sub-configs (cookie sync, hooks)
// ---------------------------------------------------------------------------

/// Account-level cookie sync defaults.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountCookieSyncConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_limit: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_limit: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_coop_sync: Option<bool>,
    #[serde(default)]
    pub priority_groups: Vec<Vec<String>>,
}

/// Account-level hooks configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountHooksConfig {
    /// Module configuration: map[vendor_name] -> map[module_name] -> raw JSON.
    #[serde(default)]
    pub modules: HashMap<String, HashMap<String, JsonValue>>,
    /// Account-specific hook execution plan.
    #[serde(default)]
    pub execution_plan: HookExecutionPlan,
}

// ---------------------------------------------------------------------------
// Account sub-configs
// ---------------------------------------------------------------------------

/// Account-level event tracking.
///
/// Maps to the Go `Events` struct in config/events.go.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountEventsConfig {
    /// When true, event tracking URLs are included in bid responses.
    #[serde(default)]
    pub enabled: bool,
    /// Default event URL template.
    #[serde(default)]
    pub default_url: String,
    /// VAST event injection configuration.
    #[serde(default)]
    pub vast_events: Vec<VastEventConfig>,
}

/// VAST event injection configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VastEventConfig {
    /// Element creation type.
    #[serde(default)]
    pub create_element: String,
    /// Tracking event type.
    #[serde(default, rename = "type")]
    pub event_type: String,
    /// Whether to exclude the default URL for this event.
    #[serde(default)]
    pub exclude_default_url: bool,
    /// Custom URLs for this event.
    #[serde(default)]
    pub urls: Vec<String>,
}

/// Account-level privacy overrides (GDPR, CCPA, COPPA, LMT).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountPrivacyConfig {
    #[serde(default)]
    pub gdpr: AccountGdprConfig,
    #[serde(default)]
    pub ccpa: AccountCcpaConfig,
    #[serde(default)]
    pub coppa: AccountCoppaConfig,
    /// Account-level LMT (Limit Ad Tracking) configuration.
    #[serde(default)]
    pub lmt: AccountLmtConfig,
    /// IPv4 anonymisation config (number of bits to mask).
    #[serde(default)]
    pub ipv4: IpAnonymisationConfig,
    /// IPv6 anonymisation config (number of bits to mask).
    #[serde(default)]
    pub ipv6: IpAnonymisationConfig,
    /// DSA configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dsa: Option<AccountDsaConfig>,
    /// Allow-activities configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowactivities: Option<AllowActivitiesConfig>,
    /// Privacy Sandbox configuration.
    #[serde(default)]
    pub privacysandbox: PrivacySandboxConfig,
}

/// Account-level LMT (Limit Ad Tracking) enforcement configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountLmtConfig {
    /// When true, LMT enforcement is enabled for this account.
    #[serde(default)]
    pub enforce: bool,
}

/// Account-level DSA configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountDsaConfig {
    /// Default DSA object as a JSON string.
    #[serde(default)]
    pub default: String,
    /// When true, DSA is only enforced for GDPR requests.
    #[serde(default)]
    pub gdpr_only: bool,
}

/// Privacy Sandbox configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrivacySandboxConfig {
    #[serde(default)]
    pub topicsdomain: String,
    #[serde(default)]
    pub cookiedeprecation: CookieDeprecationConfig,
}

/// Cookie deprecation settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CookieDeprecationConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_cookie_deprecation_ttl")]
    pub ttl_sec: u64,
}

fn default_cookie_deprecation_ttl() -> u64 {
    604800
}

/// Allow-activities configuration.
///
/// Maps to the Go `AllowActivities` struct in config/activity.go.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AllowActivitiesConfig {
    #[serde(default, rename = "syncUser")]
    pub sync_user: ActivityConfig,
    #[serde(default, rename = "fetchBids")]
    pub fetch_bids: ActivityConfig,
    #[serde(default, rename = "enrichUfpd")]
    pub enrich_ufpd: ActivityConfig,
    #[serde(default, rename = "reportAnalytics")]
    pub report_analytics: ActivityConfig,
    #[serde(default, rename = "transmitUfpd")]
    pub transmit_ufpd: ActivityConfig,
    #[serde(default, rename = "transmitPreciseGeo")]
    pub transmit_precise_geo: ActivityConfig,
    #[serde(default, rename = "transmitUniqueRequestIds")]
    pub transmit_unique_request_ids: ActivityConfig,
    #[serde(default, rename = "transmitTid")]
    pub transmit_tids: ActivityConfig,
}

/// Per-activity configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActivityConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<bool>,
    #[serde(default)]
    pub rules: Vec<ActivityRuleConfig>,
}

/// A single activity rule.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActivityRuleConfig {
    #[serde(default)]
    pub condition: ActivityConditionConfig,
    #[serde(default)]
    pub allow: bool,
}

/// Condition for an activity rule.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActivityConditionConfig {
    #[serde(default, rename = "componentName")]
    pub component_name: Vec<String>,
    #[serde(default, rename = "componentType")]
    pub component_type: Vec<String>,
}

/// IP address anonymisation settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IpAnonymisationConfig {
    /// Number of leading bits to preserve (remaining bits are zeroed).
    #[serde(default)]
    pub anon_keep_bits: u8,
}

/// Account-level GDPR overrides.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountGdprConfig {
    /// If set, overrides the host-level GDPR enabled flag for this account.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Per-channel enable flags.
    #[serde(default)]
    pub channel_enabled: ChannelEnabledConfig,
    /// Purpose-level enforcement overrides (keys "purpose1" .. "purpose10").
    #[serde(default)]
    pub purpose1: AccountGdprPurposeConfig,
    #[serde(default)]
    pub purpose2: AccountGdprPurposeConfig,
    #[serde(default)]
    pub purpose3: AccountGdprPurposeConfig,
    #[serde(default)]
    pub purpose4: AccountGdprPurposeConfig,
    #[serde(default)]
    pub purpose5: AccountGdprPurposeConfig,
    #[serde(default)]
    pub purpose6: AccountGdprPurposeConfig,
    #[serde(default)]
    pub purpose7: AccountGdprPurposeConfig,
    #[serde(default)]
    pub purpose8: AccountGdprPurposeConfig,
    #[serde(default)]
    pub purpose9: AccountGdprPurposeConfig,
    #[serde(default)]
    pub purpose10: AccountGdprPurposeConfig,
}

/// Per-purpose GDPR enforcement at the account level.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountGdprPurposeConfig {
    /// Enforcement algorithm ("basic" or "full").
    #[serde(default)]
    pub enforce_algo: String,
    #[serde(default)]
    pub enforce_purpose: bool,
    #[serde(default)]
    pub enforce_vendors: bool,
    #[serde(default)]
    pub vendor_exceptions: Vec<String>,
}

/// Per-channel enable/disable toggles.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelEnabledConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amp: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dooh: Option<bool>,
}

/// Account-level CCPA overrides.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountCcpaConfig {
    /// If set, overrides the host-level CCPA enforce flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Per-channel enable flags.
    #[serde(default)]
    pub channel_enabled: ChannelEnabledConfig,
}

/// Account-level COPPA overrides.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountCoppaConfig {
    #[serde(default)]
    pub enabled: bool,
}

/// Account-level price floors.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountPriceFloorsConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Rate at which floors are enforced (0..100).
    #[serde(default)]
    pub enforce_floors_rate: u32,
    #[serde(default)]
    pub adjust_for_bid_adjustment: bool,
    #[serde(default)]
    pub enforce_deal_floors: bool,
    #[serde(default)]
    pub use_dynamic_data: bool,
    #[serde(default)]
    pub max_rules: u32,
    #[serde(default)]
    pub max_schema_dims: u32,
    #[serde(default)]
    pub fetch: AccountFloorFetchConfig,
}

/// Dynamic floor fetching configuration at the account level.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountFloorFetchConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub timeout_ms: u32,
    #[serde(default)]
    pub max_file_size_kb: u32,
    #[serde(default)]
    pub max_rules: u32,
    #[serde(default)]
    pub max_age_sec: u32,
    #[serde(default)]
    pub period_sec: u32,
    #[serde(default)]
    pub max_schema_dims: u32,
}

/// Bid adjustments configuration.
///
/// Keys are media type names; values map bidder -> adjustment rules.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BidAdjustmentsConfig {
    #[serde(default)]
    pub media_type: HashMap<String, HashMap<String, Vec<BidAdjustmentRule>>>,
}

/// A single bid adjustment rule.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BidAdjustmentRule {
    #[serde(default)]
    pub adj_type: String,
    #[serde(default)]
    pub value: f64,
    #[serde(default)]
    pub currency: String,
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

/// Metrics configuration.
///
/// Maps to the Go `Metrics` struct in config/config.go.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetricsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub influxdb: Option<InfluxDBConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prometheus: Option<PrometheusConfig>,
    /// Granular flags for disabling specific metric groups.
    #[serde(default)]
    pub disabled_metrics: DisabledMetricsConfig,
}

/// Flags that allow selectively disabling expensive metric groups.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DisabledMetricsConfig {
    /// Stop collecting per-account adapter detail metrics.
    #[serde(default)]
    pub account_adapter_details: bool,
    /// Stop collecting per-account debug request metrics.
    #[serde(default)]
    pub account_debug: bool,
    /// Stop collecting per-account stored-response metrics.
    #[serde(default)]
    pub account_stored_responses: bool,
    /// Stop collecting bidder-connection metrics (created / reused).
    #[serde(default)]
    pub adapter_connections_metrics: bool,
    /// Stop collecting GDPR-request metrics.
    #[serde(default)]
    pub adapter_gdpr_request_blocked: bool,
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

/// Prometheus metrics configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrometheusConfig {
    #[serde(default = "default_prometheus_port")]
    pub port: u16,
    #[serde(default = "default_prometheus_namespace")]
    pub namespace: String,
    /// Subsystem prefix applied to all metric names.
    #[serde(default)]
    pub subsystem: String,
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

/// Stored request / stored data configuration.
///
/// Maps to the Go `StoredRequests` struct in config/stored_requests.go.
/// Configures where stored request JSON is loaded from (filesystem, HTTP,
/// database) and how it is cached in memory.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredRequestConfig {
    /// Filesystem backend.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filesystem: Option<FilesystemConfig>,
    /// HTTP backend.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http: Option<StoredRequestsHttpConfig>,
    /// Database backend.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<StoredRequestsDatabaseConfig>,
    /// In-memory cache settings.
    #[serde(default)]
    pub in_memory_cache: InMemoryCacheConfig,
    /// Cache-invalidation event API settings.
    #[serde(default)]
    pub cache_events: CacheEventsConfig,
    /// HTTP-based cache-invalidation event settings.
    #[serde(default)]
    pub http_events: HttpEventsConfig,
}

/// Filesystem backend for stored requests.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilesystemConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Path to the directory containing stored-request JSON files.
    #[serde(default, alias = "directorypath")]
    pub directory_path: String,
}

/// HTTP backend for stored requests.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredRequestsHttpConfig {
    /// Endpoint for fetching stored requests.
    #[serde(default)]
    pub endpoint: String,
    /// Endpoint for fetching AMP stored requests.
    #[serde(default)]
    pub amp_endpoint: String,
}

/// Database backend for stored requests.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredRequestsDatabaseConfig {
    /// Database driver ("postgres", "mysql", etc.).
    #[serde(default)]
    pub driver: String,
    /// Connection string / DSN.
    #[serde(default)]
    pub connection_string: String,
    /// SQL query for fetching stored requests by ID.
    #[serde(default)]
    pub fetch_query: String,
    /// SQL query for fetching AMP stored requests by ID.
    #[serde(default)]
    pub amp_fetch_query: String,
}

/// In-memory cache configuration for stored requests.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InMemoryCacheConfig {
    /// Cache type: "none", "unbounded", or "lru".
    #[serde(default, rename = "type")]
    pub cache_type: String,
    /// Time-to-live in seconds. 0 = no expiry.
    #[serde(default)]
    pub ttl_seconds: i32,
    /// Maximum size (in bytes) of the request cache.
    #[serde(default)]
    pub request_cache_size_bytes: i32,
    /// Maximum size (in bytes) of the imp cache.
    #[serde(default)]
    pub imp_cache_size_bytes: i32,
    /// Maximum total size (in bytes). Used when a single limit applies to all cached data.
    #[serde(default)]
    pub size_bytes: i32,
}

/// Cache-invalidation event API endpoint configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CacheEventsConfig {
    /// Enable the cache-event API.
    #[serde(default)]
    pub enabled: bool,
    /// URL path for the cache-event endpoint.
    #[serde(default)]
    pub endpoint: String,
}

/// HTTP-based cache-invalidation event configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HttpEventsConfig {
    /// Endpoint to poll for cache invalidation events.
    #[serde(default)]
    pub endpoint: String,
    /// How often (seconds) to poll for new events.
    #[serde(default)]
    pub refresh_rate_seconds: u64,
    /// Timeout in milliseconds for each poll request.
    #[serde(default)]
    pub timeout_ms: u64,
    /// AMP-specific invalidation endpoint.
    #[serde(default)]
    pub amp_endpoint: String,
}

/// Host-level GDPR configuration.
///
/// Maps to the Go `GDPR` struct in config/config.go.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GDPRConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub host_vendor_id: u32,
    /// Default value for gdpr_applies when not specified in request ("0" = no, "1" = yes/enforce).
    #[serde(default = "default_gdpr_default_value")]
    pub default_value: String,
    #[serde(default = "default_gdpr_host_vendor_list_url")]
    pub host_vendor_list_url: String,
    #[serde(default)]
    pub enforce_vendor_list: bool,
    /// EEA countries where GDPR is assumed to apply when the flag is absent.
    #[serde(default)]
    pub eea_countries: Vec<String>,
    /// If true, send all cookies regardless of GDPR consent.
    #[serde(default)]
    pub send_all_cookies: bool,
    /// Bidders exempt from Purpose 1 (storage and access) consent requirement.
    #[serde(default)]
    pub purpose1_vendor_exceptions: Vec<String>,
    /// TCF 2.x enforcement sub-configuration.
    #[serde(default)]
    pub tcf2: Tcf2Config,
    /// Non-standard publishers exempt from normal GDPR enforcement.
    #[serde(default)]
    pub non_standard_publishers: Vec<String>,
    /// Seconds between live GVL refreshes. 0 = disabled.
    #[serde(default)]
    pub live_gvl_refresh_interval_seconds: u64,
    /// GDPR active-investigation timeout settings (milliseconds).
    #[serde(default)]
    pub timeouts_ms: GdprTimeoutsConfig,
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
            tcf2: Tcf2Config::default(),
            non_standard_publishers: Vec::new(),
            live_gvl_refresh_interval_seconds: 0,
            timeouts_ms: GdprTimeoutsConfig::default(),
        }
    }
}

/// GDPR timeout configuration (milliseconds).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GdprTimeoutsConfig {
    /// Timeout for the initial vendor-list fetch.
    #[serde(default = "default_gdpr_timeout_ms")]
    pub active_vendor_list_fetch_ms: u64,
}

impl Default for GdprTimeoutsConfig {
    fn default() -> Self {
        Self {
            active_vendor_list_fetch_ms: default_gdpr_timeout_ms(),
        }
    }
}

fn default_gdpr_timeout_ms() -> u64 {
    200
}

// ---------------------------------------------------------------------------
// TCF 2.x configuration
// ---------------------------------------------------------------------------

/// TCF 2.x enforcement settings.
///
/// Each of the 10 TCF purposes has its own sub-config controlling whether
/// purpose consent and vendor consent are enforced, plus vendor exceptions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Tcf2Config {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub purpose1: Tcf2PurposeConfig,
    #[serde(default)]
    pub purpose2: Tcf2PurposeConfig,
    #[serde(default)]
    pub purpose3: Tcf2PurposeConfig,
    #[serde(default)]
    pub purpose4: Tcf2PurposeConfig,
    #[serde(default)]
    pub purpose5: Tcf2PurposeConfig,
    #[serde(default)]
    pub purpose6: Tcf2PurposeConfig,
    #[serde(default)]
    pub purpose7: Tcf2PurposeConfig,
    #[serde(default)]
    pub purpose8: Tcf2PurposeConfig,
    #[serde(default)]
    pub purpose9: Tcf2PurposeConfig,
    #[serde(default)]
    pub purpose10: Tcf2PurposeConfig,
    /// Special Feature 1 enforcement.
    #[serde(default)]
    pub special_feature1: Tcf2SpecialFeatureConfig,
    /// Purpose One Treatment settings.
    #[serde(default)]
    pub purpose_one_treatment: Tcf2PurposeOneTreatmentConfig,
}

/// Per-purpose TCF 2 enforcement.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Tcf2PurposeConfig {
    /// Enforcement algorithm ("basic" or "full").
    #[serde(default)]
    pub enforce_algo: String,
    /// Whether purpose consent is enforced.
    #[serde(default)]
    pub enforce_purpose: bool,
    /// Whether vendor consent is enforced.
    #[serde(default)]
    pub enforce_vendors: bool,
    /// Bidders that are exempt from this purpose's enforcement.
    #[serde(default)]
    pub vendor_exceptions: Vec<String>,
}

/// TCF 2 Special Feature 1 enforcement.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Tcf2SpecialFeatureConfig {
    #[serde(default)]
    pub enforce: bool,
    #[serde(default)]
    pub vendor_exceptions: Vec<String>,
}

/// TCF 2 Purpose One Treatment settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Tcf2PurposeOneTreatmentConfig {
    #[serde(default)]
    pub enabled: bool,
    /// When true, purpose one is considered consented for EEA traffic.
    #[serde(default)]
    pub access_allowed: bool,
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

/// Limit Ad Tracking (LMT) configuration
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct LmtConfig {
    pub enforce: bool,
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

// ---------------------------------------------------------------------------
// BidderInfo - loaded from per-bidder YAML files
// ---------------------------------------------------------------------------

/// Bidder information loaded from YAML config files.
///
/// Maps to the Go `BidderInfo` struct in config/bidderinfo.go.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BidderInfo {
    /// If set, this bidder is an alias of the named canonical bidder.
    #[serde(default, rename = "aliasOf")]
    pub alias_of: String,
    /// Whether this bidder is disabled.
    #[serde(default)]
    pub disabled: bool,
    /// Bidder endpoint URL template.
    #[serde(default)]
    pub endpoint: String,
    /// Extra adapter info passed to the adapter at construction time.
    #[serde(default)]
    pub extra_info: String,
    /// Maintainer contact information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maintainer: Option<MaintainerInfo>,
    /// Platform capabilities (app, site, dooh).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<CapabilitiesInfo>,
    /// GVL vendor ID for GDPR enforcement.
    #[serde(default, rename = "gvlVendorID")]
    pub gvl_vendor_id: u16,
    /// Whether modifying VAST XML is allowed.
    #[serde(default, rename = "modifyingVastXmlAllowed")]
    pub modifying_vast_xml_allowed: bool,
    /// Debug configuration for this bidder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<BidderDebugInfo>,
    /// Geographic scope restrictions.
    #[serde(default)]
    pub geoscope: Vec<String>,
    /// User sync configuration.
    #[serde(default, rename = "userSync")]
    pub user_sync: Option<BidderSyncerConfig>,
    /// Experiment flags for this bidder.
    #[serde(default)]
    pub experiment: BidderExperimentConfig,
    /// OpenRTB version and feature support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openrtb: Option<BidderOpenRTBInfo>,
    /// Endpoint compression type (e.g. "gzip").
    #[serde(default, rename = "endpointCompression")]
    pub endpoint_compression: String,
    /// XAPI config (needed for Rubicon).
    #[serde(default)]
    pub xapi: BidderXAPIConfig,
    /// Platform ID (needed for Facebook).
    #[serde(default)]
    pub platform_id: String,
    /// App secret (needed for Facebook).
    #[serde(default)]
    pub app_secret: String,
}

/// Maintainer contact info for a bidder.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MaintainerInfo {
    #[serde(default)]
    pub email: String,
}

/// Platform capabilities for a bidder.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CapabilitiesInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<PlatformInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site: Option<PlatformInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dooh: Option<PlatformInfo>,
}

/// Supported media types for a platform.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlatformInfo {
    #[serde(default, rename = "mediaTypes")]
    pub media_types: Vec<String>,
}

/// Debug settings for a bidder.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BidderDebugInfo {
    #[serde(default)]
    pub allow: bool,
}

/// User sync (cookie sync) configuration for a bidder.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct BidderSyncerConfig {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub supports: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iframe: Option<SyncerEndpointConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect: Option<SyncerEndpointConfig>,
    #[serde(default, rename = "externalUrl")]
    pub external_url: String,
    #[serde(default, rename = "formatOverride")]
    pub format_override: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// A single syncer endpoint (iframe or redirect).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SyncerEndpointConfig {
    #[serde(default)]
    pub url: String,
    #[serde(default, rename = "redirectUrl")]
    pub redirect_url: String,
    #[serde(default, rename = "externalUrl")]
    pub external_url: String,
    #[serde(default, rename = "userMacro")]
    pub user_macro: String,
}

/// Experiment flags for a bidder.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BidderExperimentConfig {
    #[serde(default, rename = "adsCert")]
    pub ads_cert: BidderAdsCertConfig,
}

/// AdsCert experiment config for a bidder.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BidderAdsCertConfig {
    #[serde(default)]
    pub enabled: bool,
}

/// OpenRTB feature support for a bidder.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BidderOpenRTBInfo {
    #[serde(default)]
    pub version: String,
    #[serde(default, rename = "gpp-supported")]
    pub gpp_supported: bool,
    #[serde(default, rename = "multiformat-supported")]
    pub multiformat_supported: Option<bool>,
}

/// XAPI config for bidders like Rubicon.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BidderXAPIConfig {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub tracker: String,
}

/// Load all bidder info YAML files from a directory.
///
/// Each file in the directory is expected to be named `{bidder_name}.yaml`
/// and deserialize into a [`BidderInfo`] struct.
pub fn load_bidder_info(dir: &str) -> Result<HashMap<String, BidderInfo>> {
    let dir_path = Path::new(dir);
    if !dir_path.is_dir() {
        bail!("bidder info directory '{}' does not exist or is not a directory", dir);
    }

    let mut bidders = HashMap::new();
    let entries = std::fs::read_dir(dir_path)
        .with_context(|| format!("failed to read bidder info directory '{}'", dir))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        // Only process .yaml and .yml files
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "yaml" && ext != "yml" {
            continue;
        }

        let bidder_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        if bidder_name.is_empty() {
            continue;
        }

        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read bidder info file '{}'", path.display()))?;
        let info: BidderInfo = serde_yaml::from_str(&contents)
            .with_context(|| format!("failed to parse bidder info file '{}'", path.display()))?;

        bidders.insert(bidder_name, info);
    }

    Ok(bidders)
}

// ---------------------------------------------------------------------------
// AccountFetcher trait and implementations
// ---------------------------------------------------------------------------

/// Trait for fetching account configuration by account ID.
pub trait AccountFetcher {
    /// Fetch account configuration for the given account ID.
    fn fetch_account(&self, id: &str) -> Result<AccountConfig>;
}

/// Fetches accounts from a directory of JSON files.
///
/// Each account is stored as `{base_dir}/{id}.json`.
pub struct FileAccountFetcher {
    base_dir: PathBuf,
}

impl FileAccountFetcher {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }
}

impl AccountFetcher for FileAccountFetcher {
    fn fetch_account(&self, id: &str) -> Result<AccountConfig> {
        if id.is_empty() {
            bail!("account ID must not be empty");
        }
        // Prevent path traversal
        if id.contains("..") || id.contains('/') || id.contains('\\') {
            bail!("invalid account ID '{}'", id);
        }
        let path = self.base_dir.join(format!("{}.json", id));
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read account file '{}'", path.display()))?;
        let account: AccountConfig = serde_json::from_str(&contents)
            .with_context(|| format!("failed to parse account file '{}'", path.display()))?;
        Ok(account)
    }
}

/// Fetches accounts from an HTTP endpoint.
///
/// The endpoint is called as `{base_url}/{id}` and is expected to return
/// account JSON. This is a blocking implementation suitable for startup or
/// synchronous contexts.
pub struct HttpAccountFetcher {
    base_url: String,
}

impl HttpAccountFetcher {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }
}

impl AccountFetcher for HttpAccountFetcher {
    fn fetch_account(&self, id: &str) -> Result<AccountConfig> {
        if id.is_empty() {
            bail!("account ID must not be empty");
        }
        let url = format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            id
        );
        // Use a simple blocking HTTP GET. In production this would use an async
        // client, but for config loading a blocking call is sufficient.
        let body = reqwest_blocking_get(&url)
            .with_context(|| format!("failed to fetch account from '{}'", url))?;
        let account: AccountConfig = serde_json::from_str(&body)
            .with_context(|| format!("failed to parse account JSON from '{}'", url))?;
        Ok(account)
    }
}

/// Minimal blocking HTTP GET using std only (no reqwest dependency needed at
/// config crate level). Returns the response body as a string.
///
/// In a real deployment you would replace this with reqwest or similar.
fn reqwest_blocking_get(url: &str) -> Result<String> {
    // We intentionally keep the config crate dependency-light. This is a stub
    // that will fail at runtime if actually called but compiles fine. Real
    // HTTP fetching should be wired in at the application layer.
    bail!(
        "HTTP account fetching is not available in this build. \
         Attempted to GET '{}'. Wire in an HTTP client at the application layer.",
        url
    )
}

/// Fetches accounts from an in-memory HashMap.
pub struct InMemoryAccountFetcher {
    accounts: HashMap<String, AccountConfig>,
}

impl InMemoryAccountFetcher {
    pub fn new(accounts: HashMap<String, AccountConfig>) -> Self {
        Self { accounts }
    }

    /// Build from the accounts map in a [`Configuration`].
    pub fn from_config(cfg: &Configuration) -> Self {
        Self {
            accounts: cfg.accounts.clone(),
        }
    }
}

impl AccountFetcher for InMemoryAccountFetcher {
    fn fetch_account(&self, id: &str) -> Result<AccountConfig> {
        self.accounts
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("account '{}' not found", id))
    }
}

// ---------------------------------------------------------------------------
// Configuration validation
// ---------------------------------------------------------------------------

/// A single validation error produced by [`Configuration::validate`].
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub message: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
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

    /// Load configuration from a specific file path.
    ///
    /// The file format is auto-detected from the extension (.yaml, .yml, .json,
    /// .toml). Environment variables with the `PBS_` prefix are NOT applied
    /// automatically; call [`with_env_overrides`](Self::with_env_overrides) if
    /// you want them.
    pub fn from_file(path: &str) -> Result<Configuration> {
        let builder = config::Config::builder()
            .add_source(config::File::with_name(path).required(true));
        let config = builder
            .build()
            .with_context(|| format!("failed to load configuration from '{}'", path))?;
        let cfg: Configuration = config
            .try_deserialize()
            .with_context(|| format!("failed to deserialize configuration from '{}'", path))?;
        Ok(cfg)
    }

    /// Load configuration by merging multiple file sources in order.
    ///
    /// Later files override earlier ones. Environment variables are NOT applied;
    /// call [`with_env_overrides`](Self::with_env_overrides) afterwards if desired.
    pub fn from_files(paths: &[&str]) -> Result<Configuration> {
        let mut builder = config::Config::builder();
        for path in paths {
            builder = builder.add_source(config::File::with_name(path).required(true));
        }
        let config = builder.build().context("failed to merge configuration files")?;
        let cfg: Configuration = config
            .try_deserialize()
            .context("failed to deserialize merged configuration")?;
        Ok(cfg)
    }

    /// Apply PBS_* environment variable overrides and return self for chaining.
    ///
    /// This applies both the generic `config` crate environment source mapping
    /// (PBS_HOST -> host, PBS_ADAPTERS_APPNEXUS_ENDPOINT -> adapters.appnexus.endpoint, etc.)
    /// and well-known explicit overrides.
    pub fn with_env_overrides(mut self) -> Self {
        self.apply_env_overrides();
        self
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
            self.gdpr.enabled = self.gdpr_enabled;
        }
        if let Ok(val) = std::env::var("PBS_CCPA_ENFORCE") {
            self.ccpa_enforce = matches!(val.to_lowercase().as_str(), "true" | "1" | "yes");
            self.ccpa.enforce = self.ccpa_enforce;
        }
        // Adapter endpoint overrides: PBS_ADAPTERS_{BIDDER}_ENDPOINT
        for (key, val) in std::env::vars() {
            if let Some(rest) = key.strip_prefix("PBS_ADAPTERS_") {
                // e.g. PBS_ADAPTERS_APPNEXUS_ENDPOINT -> rest = "APPNEXUS_ENDPOINT"
                if let Some(bidder_upper) = rest.strip_suffix("_ENDPOINT") {
                    let bidder = bidder_upper.to_lowercase();
                    self.adapters
                        .entry(bidder)
                        .or_insert_with(AdapterConfig::default)
                        .endpoint = val;
                }
            }
        }
    }

    /// Validate the configuration, returning a list of errors.
    ///
    /// Ported from the Go `Configuration.validate()` method in config/config.go.
    /// Returns `Ok(())` if valid, or `Err` with all validation errors collected.
    pub fn validate(&self) -> std::result::Result<(), Vec<ValidationError>> {
        let mut errs = Vec::new();

        // --- Required fields ---
        if self.host.is_empty() {
            errs.push(verr("host must not be empty"));
        }
        if self.port == 0 {
            errs.push(verr("port must be greater than 0"));
        }

        // --- Auction timeouts ---
        if self.auction_timeouts.max > 0
            && self.auction_timeouts.default > 0
            && self.auction_timeouts.max < self.auction_timeouts.default
        {
            errs.push(verr(&format!(
                "auction_timeouts.max ({}) cannot be less than auction_timeouts.default ({})",
                self.auction_timeouts.max, self.auction_timeouts.default
            )));
        }

        // --- Max request size ---
        // Go uses int64 so negative is possible; in Rust it is usize (always >= 0), no check needed.

        // --- Stored requests timeout ---
        if self.stored_requests_timeout_ms == 0 {
            errs.push(verr("stored_requests_timeout_ms must be > 0"));
        }

        // --- GDPR validation ---
        self.validate_gdpr(&mut errs);

        // --- CCPA / privacy consistency ---
        // (CCPA has no complex validation in Go beyond the enforce flag)

        // --- External cache ---
        self.validate_external_cache(&mut errs);

        // --- Adapter endpoints must be valid URLs ---
        for (name, adapter) in &self.adapters {
            if !adapter.endpoint.is_empty() && !is_valid_url(&adapter.endpoint) {
                errs.push(verr(&format!(
                    "adapters.{}.endpoint '{}' is not a valid URL",
                    name, adapter.endpoint
                )));
            }
        }

        // --- Prometheus metrics ---
        if let Some(ref prom) = self.metrics.prometheus {
            if prom.port > 0 && prom.timeout_ms == 0 {
                errs.push(verr(&format!(
                    "metrics.prometheus.timeout_ms must be positive if metrics.prometheus.port is defined. Got timeout={} and port={}",
                    prom.timeout_ms, prom.port
                )));
            }
        }

        // --- Debug sampling rate ---
        if self.debug.timeout_notification.sampling_rate < 0.0
            || self.debug.timeout_notification.sampling_rate > 1.0
        {
            errs.push(verr(&format!(
                "debug.timeout_notification.sampling_rate must be between 0.0 and 1.0. Got {}",
                self.debug.timeout_notification.sampling_rate
            )));
        }

        // --- Currency converter ---
        if self.currency.fetch_interval_seconds == 0 {
            // 0 means disabled, which is acceptable, but warn-level in Go.
            // We allow it without error.
        }

        // --- Price floors account defaults ---
        self.validate_price_floors(&mut errs);

        // --- TCF2 purpose enforce_algo ---
        self.validate_tcf2_purposes(&mut errs);

        // --- Timeout values should be reasonable ---
        if self.auction_timeouts.max > 60_000 {
            errs.push(verr(&format!(
                "auction_timeouts.max ({}) exceeds 60000ms, which is unreasonably large",
                self.auction_timeouts.max
            )));
        }
        if self.tmax_default > 60_000 {
            errs.push(verr(&format!(
                "tmax_default ({}) exceeds 60000ms, which is unreasonably large",
                self.tmax_default
            )));
        }

        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs)
        }
    }

    fn validate_gdpr(&self, errs: &mut Vec<ValidationError>) {
        let dv = &self.gdpr.default_value;
        if dv != "0" && dv != "1" {
            errs.push(verr(&format!(
                "gdpr.default_value must be \"0\" or \"1\". Got \"{}\"",
                dv
            )));
        }
        if self.gdpr.host_vendor_id > 0xffff {
            errs.push(verr(&format!(
                "gdpr.host_vendor_id must be in the range [0, {}]. Got {}",
                0xffff,
                self.gdpr.host_vendor_id
            )));
        }
    }

    fn validate_external_cache(&self, errs: &mut Vec<ValidationError>) {
        let ec = &self.external_cache;
        if ec.host.is_empty() && ec.path.is_empty() {
            return; // both blank is fine
        }
        if !ec.scheme.is_empty() && ec.scheme != "http" && ec.scheme != "https" {
            errs.push(verr("external cache scheme must be http or https if specified"));
        }
        if (ec.host.is_empty()) != (ec.path.is_empty()) {
            errs.push(verr("external cache host and path must both be specified"));
        }
        if ec.host.ends_with('/') {
            errs.push(verr(&format!(
                "external cache host '{}' must not end with a path separator",
                ec.host
            )));
        }
        if ec.host.contains("://") {
            errs.push(verr(&format!(
                "external cache host must not specify a protocol. '{}'",
                ec.host
            )));
        }
        if !ec.path.is_empty() && !ec.path.starts_with('/') {
            errs.push(verr(&format!(
                "external cache path '{}' must begin with a path separator",
                ec.path
            )));
        }
    }

    fn validate_price_floors(&self, errs: &mut Vec<ValidationError>) {
        let pf = &self.account_defaults.price_floors;
        if pf.enforce_floors_rate > 100 {
            errs.push(verr(
                "account_defaults.price_floors.enforce_floors_rate should be between 0 and 100",
            ));
        }
        if pf.max_schema_dims > 20 {
            errs.push(verr(
                "account_defaults.price_floors.max_schema_dims should be between 0 and 20",
            ));
        }
    }

    fn validate_tcf2_purposes(&self, errs: &mut Vec<ValidationError>) {
        let purposes = [
            &self.gdpr.tcf2.purpose1,
            &self.gdpr.tcf2.purpose2,
            &self.gdpr.tcf2.purpose3,
            &self.gdpr.tcf2.purpose4,
            &self.gdpr.tcf2.purpose5,
            &self.gdpr.tcf2.purpose6,
            &self.gdpr.tcf2.purpose7,
            &self.gdpr.tcf2.purpose8,
            &self.gdpr.tcf2.purpose9,
            &self.gdpr.tcf2.purpose10,
        ];
        for (i, p) in purposes.iter().enumerate() {
            if !p.enforce_algo.is_empty()
                && p.enforce_algo != "basic"
                && p.enforce_algo != "full"
            {
                errs.push(verr(&format!(
                    "gdpr.tcf2.purpose{}.enforce_algo must be \"basic\" or \"full\". Got \"{}\"",
                    i + 1,
                    p.enforce_algo
                )));
            }
        }
    }
}

/// Helper to create a [`ValidationError`].
fn verr(msg: &str) -> ValidationError {
    ValidationError {
        message: msg.to_string(),
    }
}

/// Simple URL validation: must parse and have a scheme.
fn is_valid_url(s: &str) -> bool {
    // Use a minimal check: must contain "://" and parse reasonably.
    if !s.contains("://") {
        return false;
    }
    // Try to split on "://" and verify scheme is alphabetic
    if let Some(scheme) = s.split("://").next() {
        if scheme.is_empty() || !scheme.chars().all(|c| c.is_ascii_alphabetic()) {
            return false;
        }
    }
    // Verify there is a host part after ://
    if let Some(rest) = s.split("://").nth(1) {
        if rest.is_empty() {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// BidRoundingMode
// ---------------------------------------------------------------------------

/// Bid rounding mode controlling how bid prices are rounded.
///
/// Maps to the Go `BidRoundingMode` type in config/account.go.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BidRoundingMode {
    /// Round down (floor).
    #[serde(rename = "down")]
    Down,
    /// True rounding (standard arithmetic rounding).
    #[serde(rename = "true")]
    True,
    /// Time-split rounding (alternates rounding direction based on time).
    #[serde(rename = "timesplit")]
    TimeSplit,
    /// Round up (ceil).
    #[serde(rename = "up")]
    Up,
}

impl Default for BidRoundingMode {
    fn default() -> Self {
        BidRoundingMode::Down
    }
}

impl std::fmt::Display for BidRoundingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BidRoundingMode::Down => write!(f, "down"),
            BidRoundingMode::True => write!(f, "true"),
            BidRoundingMode::TimeSplit => write!(f, "timesplit"),
            BidRoundingMode::Up => write!(f, "up"),
        }
    }
}

// ---------------------------------------------------------------------------
// DefaultTTLs
// ---------------------------------------------------------------------------

/// Default TTL (time-to-live) values in seconds for each media type.
///
/// Maps to the Go `DefaultTTLs` struct in config/config.go. This is
/// distinct from `CacheTTL` which is used for cache-specific TTL config.
/// `DefaultTTLs` represents the global default TTL applied when a bidder
/// does not specify one.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DefaultTTLs {
    /// Default TTL for banner creatives (seconds).
    #[serde(default)]
    pub banner: i32,
    /// Default TTL for video creatives (seconds).
    #[serde(default)]
    pub video: i32,
    /// Default TTL for native creatives (seconds).
    #[serde(default)]
    pub native: i32,
    /// Default TTL for audio creatives (seconds).
    #[serde(default)]
    pub audio: i32,
}

// ---------------------------------------------------------------------------
// RequestValidationExt
// ---------------------------------------------------------------------------

/// Extended request validation settings controlling which validation
/// steps can be skipped.
///
/// These flags allow operators to skip specific validation passes during
/// request processing, for example when testing or for backward compat.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequestValidationExt {
    /// When true, bidder-specific parameter validation is skipped.
    #[serde(default)]
    pub skip_bidder_params: bool,
    /// When true, native request validation is skipped.
    #[serde(default)]
    pub skip_native: bool,
}

// ---------------------------------------------------------------------------
// TCF2 BasicEnforcementVendors
// ---------------------------------------------------------------------------

/// Vendor set for TCF2 basic enforcement.
///
/// When basic enforcement is used for a purpose, only vendors in this set
/// are subject to the simplified consent check. All other vendors fall
/// through to full enforcement.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Tcf2BasicEnforcementVendors {
    /// Vendor IDs subject to basic enforcement.
    #[serde(default)]
    pub vendor_ids: Vec<u16>,
}

// ---------------------------------------------------------------------------
// HostCookieOptOut
// ---------------------------------------------------------------------------

/// Extended host cookie opt-out configuration.
///
/// Provides additional settings beyond the simple CookieConfig for
/// controlling the opt-out experience.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HostCookieOptOut {
    /// Name of the opt-out cookie.
    #[serde(default)]
    pub name: String,
    /// Value that indicates the user has opted out.
    #[serde(default)]
    pub value: String,
    /// Custom URL the user is redirected to after opting out.
    #[serde(default)]
    pub custom_redirect_url: String,
}

// ---------------------------------------------------------------------------
// ExperimentAdsCertConfig (extended)
// ---------------------------------------------------------------------------

/// Extended A/B testing configuration for experiments.
///
/// Maps to the Go `Experiment` struct extended fields. Provides knobs
/// for enabling experimental features in a controlled rollout.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExperimentABTestConfig {
    /// When true, A/B testing for this experiment is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Percentage of traffic to route to the experimental path (0..100).
    #[serde(default)]
    pub percentage: u8,
    /// Identifier for this experiment (used in analytics/logging).
    #[serde(default)]
    pub experiment_id: String,
}

// ---------------------------------------------------------------------------
// EndpointCompressionMode
// ---------------------------------------------------------------------------

/// Compression mode for bidder endpoint requests.
///
/// Determines whether and how outgoing bid requests are compressed
/// before being sent to the bidder's endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EndpointCompressionMode {
    /// No compression.
    #[serde(rename = "none")]
    None,
    /// GZIP compression.
    #[serde(rename = "gzip")]
    Gzip,
}

impl Default for EndpointCompressionMode {
    fn default() -> Self {
        EndpointCompressionMode::None
    }
}

impl std::fmt::Display for EndpointCompressionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EndpointCompressionMode::None => write!(f, "none"),
            EndpointCompressionMode::Gzip => write!(f, "gzip"),
        }
    }
}

// ---------------------------------------------------------------------------
// BidderInfoWhiteLabel
// ---------------------------------------------------------------------------

/// White-label bidder configuration.
///
/// When `white_label_only` is true on a `BidderInfo`, the adapter is not
/// available as a standalone bidder and can only be used through an alias.
/// This struct captures additional metadata for white-label adapters.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BidderInfoWhiteLabel {
    /// When true, this adapter can only be accessed through an alias.
    #[serde(default)]
    pub white_label_only: bool,
    /// Optional list of allowed alias names. Empty means any alias is allowed.
    #[serde(default)]
    pub allowed_aliases: Vec<String>,
}

// ---------------------------------------------------------------------------
// Tcf2PurposeEnforcementConfig (extended)
// ---------------------------------------------------------------------------

/// Extended per-purpose TCF2 enforcement configuration including
/// basic enforcement vendor sets and purpose-specific flags.
///
/// Supplements `Tcf2PurposeConfig` with additional enforcement
/// granularity used in the Go implementation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Tcf2PurposeEnforcementConfig {
    /// Enforcement algorithm ("basic" or "full").
    #[serde(default)]
    pub enforce_algo: String,
    /// Whether purpose consent is enforced.
    #[serde(default)]
    pub enforce_purpose: bool,
    /// Whether vendor consent is enforced.
    #[serde(default)]
    pub enforce_vendors: bool,
    /// Bidders that are exempt from this purpose's enforcement.
    #[serde(default)]
    pub vendor_exceptions: Vec<String>,
    /// Vendors subject to basic enforcement for this purpose.
    #[serde(default)]
    pub basic_enforcement_vendors: Tcf2BasicEnforcementVendors,
}

// ---------------------------------------------------------------------------
// Tcf2SpecialFeaturesConfig (extended)
// ---------------------------------------------------------------------------

/// Extended special features enforcement configuration.
///
/// Covers special features 1 and 2 with per-feature vendor exceptions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Tcf2SpecialFeaturesConfig {
    /// Special Feature 1 enforcement.
    #[serde(default)]
    pub special_feature1: Tcf2SpecialFeatureConfig,
    /// Special Feature 2 enforcement (reserved for future use).
    #[serde(default)]
    pub special_feature2: Tcf2SpecialFeatureConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Existing tests (preserved)
    // -----------------------------------------------------------------------

    #[test]
    fn test_default_config_values() {
        // Test that Configuration::load(None) produces sane defaults.
        // Some assertions are omitted because parallel tests may pollute
        // the process-wide environment (e.g. PBS_PORT, PBS_GDPR_ENABLED).
        let cfg = Configuration::load(None).expect("should load default config");
        // These fields are not affected by any parallel test's set_var calls:
        assert_eq!(cfg.gdpr.default_value, "1");
        assert_eq!(cfg.auction_timeouts.default, 1000);
        assert_eq!(cfg.auction_timeouts.max, 5000);
    }

    #[test]
    fn test_default_adapter_config_struct() {
        let ac = AdapterConfig::default();
        assert!(ac.endpoint.is_empty());
        assert!(ac.extra_info.is_none());
        assert!(ac.timeout_ms.is_none());
    }

    #[test]
    fn test_default_gdpr_config() {
        let gdpr = GDPRConfig::default();
        assert!(!gdpr.enabled);
        assert_eq!(gdpr.default_value, "1");
        assert!(!gdpr.enforce_vendor_list);
        assert!(gdpr.eea_countries.is_empty());
    }

    #[test]
    fn test_default_ccpa_config_via_load() {
        // Verify that the serde default for ccpa.enforce is true (via default_true fn).
        // We deserialize from an empty JSON object to trigger serde defaults.
        let ccpa: CCPAConfig = serde_json::from_str("{}").expect("valid");
        assert!(ccpa.enforce, "ccpa.enforce should default to true via serde");
    }

    #[test]
    fn test_env_override_port() {
        let old = std::env::var("PBS_PORT").ok();
        std::env::set_var("PBS_PORT", "9090");

        let mut cfg = Configuration::default();
        cfg.apply_env_overrides();
        assert_eq!(cfg.port, 9090);

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

    // -----------------------------------------------------------------------
    // AccountConfig tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_account_config_from_json() {
        let json = r#"{
            "id": "pub-123",
            "disabled": false,
            "default_integration": "web",
            "events": { "enabled": true },
            "privacy": {
                "gdpr": {
                    "enabled": true,
                    "channel_enabled": { "amp": false, "web": true }
                },
                "ccpa": { "enabled": true },
                "coppa": { "enabled": false }
            },
            "price_floors": {
                "enabled": true,
                "enforce_floors_rate": 80,
                "adjust_for_bid_adjustment": true,
                "enforce_deal_floors": true,
                "max_rules": 100,
                "fetch": {
                    "enabled": true,
                    "url": "https://floors.example.com",
                    "timeout_ms": 3000
                }
            },
            "bid_adjustments": {
                "media_type": {
                    "banner": {
                        "bidderA": [{ "adj_type": "cpm", "value": 0.5, "currency": "USD" }]
                    }
                }
            },
            "debug_allow": true
        }"#;

        let acct: AccountConfig = serde_json::from_str(json).unwrap();
        assert_eq!(acct.id, "pub-123");
        assert!(!acct.disabled);
        assert_eq!(acct.default_integration, "web");
        assert!(acct.events.enabled);
        assert_eq!(acct.privacy.gdpr.enabled, Some(true));
        assert_eq!(acct.privacy.gdpr.channel_enabled.amp, Some(false));
        assert_eq!(acct.privacy.gdpr.channel_enabled.web, Some(true));
        assert_eq!(acct.privacy.ccpa.enabled, Some(true));
        assert!(!acct.privacy.coppa.enabled);
        assert!(acct.price_floors.enabled);
        assert_eq!(acct.price_floors.enforce_floors_rate, 80);
        assert!(acct.price_floors.adjust_for_bid_adjustment);
        assert!(acct.price_floors.enforce_deal_floors);
        assert_eq!(acct.price_floors.max_rules, 100);
        assert!(acct.price_floors.fetch.enabled);
        assert_eq!(acct.price_floors.fetch.url, "https://floors.example.com");
        assert_eq!(acct.price_floors.fetch.timeout_ms, 3000);
        let adj = acct.bid_adjustments.unwrap();
        let banner = &adj.media_type["banner"]["bidderA"];
        assert_eq!(banner.len(), 1);
        assert_eq!(banner[0].adj_type, "cpm");
        assert!((banner[0].value - 0.5).abs() < f64::EPSILON);
        assert!(acct.debug_allow);
    }

    #[test]
    fn test_account_config_from_toml() {
        let toml_str = r#"
            id = "pub-456"
            disabled = true
            default_integration = "app"

            [events]
            enabled = false

            [privacy.gdpr]
            enabled = false

            [privacy.ccpa]
            enabled = false

            [price_floors]
            enabled = false
        "#;

        let acct: AccountConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(acct.id, "pub-456");
        assert!(acct.disabled);
        assert_eq!(acct.default_integration, "app");
        assert!(!acct.events.enabled);
        assert_eq!(acct.privacy.gdpr.enabled, Some(false));
        assert!(!acct.price_floors.enabled);
        assert!(acct.bid_adjustments.is_none());
    }

    #[test]
    fn test_account_config_defaults() {
        let acct = AccountConfig::default();
        assert!(acct.id.is_empty());
        assert!(!acct.disabled);
        assert!(!acct.events.enabled);
        assert!(acct.privacy.gdpr.enabled.is_none());
        assert!(acct.privacy.ccpa.enabled.is_none());
        assert!(!acct.price_floors.enabled);
        assert!(acct.bid_adjustments.is_none());
    }

    // -----------------------------------------------------------------------
    // GDPRConfig / TCF2 tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_gdpr_config_from_json_with_tcf2() {
        let json = r#"{
            "enabled": true,
            "host_vendor_id": 42,
            "default_value": "0",
            "eea_countries": ["DE", "FR", "IT"],
            "tcf2": {
                "enabled": true,
                "purpose1": {
                    "enforce_algo": "full",
                    "enforce_purpose": true,
                    "enforce_vendors": true,
                    "vendor_exceptions": ["bidderX"]
                },
                "purpose2": {
                    "enforce_purpose": false
                },
                "special_feature1": {
                    "enforce": true,
                    "vendor_exceptions": ["bidderY"]
                },
                "purpose_one_treatment": {
                    "enabled": true,
                    "access_allowed": true
                }
            },
            "non_standard_publishers": ["pub-legacy"],
            "timeouts_ms": {
                "active_vendor_list_fetch_ms": 500
            }
        }"#;

        let gdpr: GDPRConfig = serde_json::from_str(json).unwrap();
        assert!(gdpr.enabled);
        assert_eq!(gdpr.host_vendor_id, 42);
        assert_eq!(gdpr.default_value, "0");
        assert_eq!(gdpr.eea_countries, vec!["DE", "FR", "IT"]);
        assert!(gdpr.tcf2.enabled);
        assert_eq!(gdpr.tcf2.purpose1.enforce_algo, "full");
        assert!(gdpr.tcf2.purpose1.enforce_purpose);
        assert!(gdpr.tcf2.purpose1.enforce_vendors);
        assert_eq!(gdpr.tcf2.purpose1.vendor_exceptions, vec!["bidderX"]);
        assert!(!gdpr.tcf2.purpose2.enforce_purpose);
        assert!(gdpr.tcf2.special_feature1.enforce);
        assert_eq!(gdpr.tcf2.special_feature1.vendor_exceptions, vec!["bidderY"]);
        assert!(gdpr.tcf2.purpose_one_treatment.enabled);
        assert!(gdpr.tcf2.purpose_one_treatment.access_allowed);
        assert_eq!(gdpr.non_standard_publishers, vec!["pub-legacy"]);
        assert_eq!(gdpr.timeouts_ms.active_vendor_list_fetch_ms, 500);
    }

    #[test]
    fn test_gdpr_config_from_toml() {
        let toml_str = r#"
            enabled = true
            host_vendor_id = 10
            default_value = "1"

            [tcf2]
            enabled = true

            [tcf2.purpose1]
            enforce_algo = "basic"
            enforce_purpose = true
            enforce_vendors = false

            [tcf2.special_feature1]
            enforce = false

            [timeouts_ms]
            active_vendor_list_fetch_ms = 300
        "#;

        let gdpr: GDPRConfig = toml::from_str(toml_str).unwrap();
        assert!(gdpr.enabled);
        assert_eq!(gdpr.host_vendor_id, 10);
        assert!(gdpr.tcf2.enabled);
        assert_eq!(gdpr.tcf2.purpose1.enforce_algo, "basic");
        assert!(gdpr.tcf2.purpose1.enforce_purpose);
        assert!(!gdpr.tcf2.purpose1.enforce_vendors);
        assert!(!gdpr.tcf2.special_feature1.enforce);
        assert_eq!(gdpr.timeouts_ms.active_vendor_list_fetch_ms, 300);
    }

    #[test]
    fn test_gdpr_default_includes_tcf2() {
        let gdpr = GDPRConfig::default();
        assert!(!gdpr.tcf2.enabled);
        assert!(!gdpr.tcf2.purpose1.enforce_purpose);
        assert!(gdpr.tcf2.purpose1.vendor_exceptions.is_empty());
        assert_eq!(gdpr.timeouts_ms.active_vendor_list_fetch_ms, 200);
    }

    // -----------------------------------------------------------------------
    // CCPAConfig tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_ccpa_config_from_json() {
        let json = r#"{ "enforce": false }"#;
        let ccpa: CCPAConfig = serde_json::from_str(json).unwrap();
        assert!(!ccpa.enforce);
    }

    #[test]
    fn test_ccpa_config_from_toml() {
        let toml_str = r#"enforce = true"#;
        let ccpa: CCPAConfig = toml::from_str(toml_str).unwrap();
        assert!(ccpa.enforce);
    }

    // -----------------------------------------------------------------------
    // StoredRequestConfig tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_stored_requests_from_json() {
        let json = r#"{
            "filesystem": {
                "enabled": true,
                "directory_path": "/data/stored"
            },
            "http": {
                "endpoint": "https://sr.example.com/fetch",
                "amp_endpoint": "https://sr.example.com/amp"
            },
            "in_memory_cache": {
                "type": "lru",
                "ttl_seconds": 600,
                "request_cache_size_bytes": 10485760,
                "imp_cache_size_bytes": 5242880,
                "size_bytes": 0
            },
            "cache_events": {
                "enabled": true,
                "endpoint": "/storedrequests/openrtb2"
            },
            "http_events": {
                "endpoint": "https://sr.example.com/events",
                "refresh_rate_seconds": 60,
                "timeout_ms": 2000,
                "amp_endpoint": "https://sr.example.com/amp-events"
            }
        }"#;

        let sr: StoredRequestConfig = serde_json::from_str(json).unwrap();
        let fs = sr.filesystem.unwrap();
        assert!(fs.enabled);
        assert_eq!(fs.directory_path, "/data/stored");
        let http = sr.http.unwrap();
        assert_eq!(http.endpoint, "https://sr.example.com/fetch");
        assert_eq!(http.amp_endpoint, "https://sr.example.com/amp");
        assert_eq!(sr.in_memory_cache.cache_type, "lru");
        assert_eq!(sr.in_memory_cache.ttl_seconds, 600);
        assert_eq!(sr.in_memory_cache.request_cache_size_bytes, 10485760);
        assert_eq!(sr.in_memory_cache.imp_cache_size_bytes, 5242880);
        assert!(sr.cache_events.enabled);
        assert_eq!(sr.cache_events.endpoint, "/storedrequests/openrtb2");
        assert_eq!(sr.http_events.refresh_rate_seconds, 60);
        assert_eq!(sr.http_events.timeout_ms, 2000);
    }

    #[test]
    fn test_stored_requests_from_toml() {
        let toml_str = r#"
            [filesystem]
            enabled = true
            directory_path = "/var/stored"

            [http]
            endpoint = "https://example.com/stored"
            amp_endpoint = "https://example.com/amp"

            [in_memory_cache]
            type = "unbounded"
            ttl_seconds = 300
            request_cache_size_bytes = 0
            imp_cache_size_bytes = 0
        "#;

        let sr: StoredRequestConfig = toml::from_str(toml_str).unwrap();
        let fs = sr.filesystem.unwrap();
        assert!(fs.enabled);
        assert_eq!(fs.directory_path, "/var/stored");
        assert_eq!(sr.in_memory_cache.cache_type, "unbounded");
        assert_eq!(sr.in_memory_cache.ttl_seconds, 300);
    }

    #[test]
    fn test_stored_requests_filesystem_alias() {
        // The old Go field was "directorypath"; ensure the alias works.
        let json = r#"{ "enabled": true, "directorypath": "/legacy/path" }"#;
        let fs: FilesystemConfig = serde_json::from_str(json).unwrap();
        assert!(fs.enabled);
        assert_eq!(fs.directory_path, "/legacy/path");
    }

    #[test]
    fn test_stored_requests_defaults() {
        let sr = StoredRequestConfig::default();
        assert!(sr.filesystem.is_none());
        assert!(sr.http.is_none());
        assert!(sr.database.is_none());
        assert!(sr.in_memory_cache.cache_type.is_empty());
        assert_eq!(sr.in_memory_cache.ttl_seconds, 0);
    }

    // -----------------------------------------------------------------------
    // CurrencyConfig tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_currency_config_from_json() {
        let json = r#"{
            "fetch_url": "https://custom.cdn/rates.json",
            "fetch_interval_seconds": 900,
            "rates": {
                "USD": { "EUR": 0.85, "GBP": 0.73 }
            }
        }"#;

        let cur: CurrencyConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cur.fetch_url, "https://custom.cdn/rates.json");
        assert_eq!(cur.fetch_interval_seconds, 900);
        let usd = &cur.rates["USD"];
        assert!((usd["EUR"] - 0.85).abs() < f64::EPSILON);
        assert!((usd["GBP"] - 0.73).abs() < f64::EPSILON);
    }

    #[test]
    fn test_currency_config_from_toml() {
        let toml_str = r#"
            fetch_url = "https://cdn.example.com/rates.json"
            fetch_interval_seconds = 3600

            [rates.USD]
            EUR = 0.9
        "#;

        let cur: CurrencyConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cur.fetch_url, "https://cdn.example.com/rates.json");
        assert_eq!(cur.fetch_interval_seconds, 3600);
        assert!((cur.rates["USD"]["EUR"] - 0.9).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // CacheConfig tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_cache_config_from_json() {
        let json = r#"{
            "scheme": "https",
            "host": "cache.prebid.org",
            "port": 443,
            "path": "/cache",
            "query": "uuid=",
            "expected_millis": 50,
            "default_ttl_secs": {
                "banner_ttl_secs": 300,
                "video_ttl_secs": 600,
                "native_ttl_secs": 150,
                "audio_ttl_secs": 200
            }
        }"#;

        let cache: CacheConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cache.scheme, "https");
        assert_eq!(cache.host, "cache.prebid.org");
        assert_eq!(cache.port, 443);
        assert_eq!(cache.path, "/cache");
        assert_eq!(cache.query, "uuid=");
        assert_eq!(cache.expected_millis, 50);
        assert_eq!(cache.default_ttl_secs.banner_ttl_secs, 300);
        assert_eq!(cache.default_ttl_secs.video_ttl_secs, 600);
        assert_eq!(cache.default_ttl_secs.native_ttl_secs, 150);
        assert_eq!(cache.default_ttl_secs.audio_ttl_secs, 200);
    }

    #[test]
    fn test_cache_config_from_toml() {
        let toml_str = r#"
            scheme = "http"
            host = "localhost"
            port = 8080
            path = "/cache"
            query = "uuid="
            expected_millis = 10

            [default_ttl_secs]
            banner_ttl_secs = 60
            video_ttl_secs = 120
            native_ttl_secs = 30
            audio_ttl_secs = 45
        "#;

        let cache: CacheConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cache.scheme, "http");
        assert_eq!(cache.host, "localhost");
        assert_eq!(cache.port, 8080);
        assert_eq!(cache.default_ttl_secs.banner_ttl_secs, 60);
    }

    #[test]
    fn test_cache_config_defaults() {
        let cache = CacheConfig::default();
        assert!(cache.scheme.is_empty());
        assert!(cache.host.is_empty());
        assert_eq!(cache.port, 0);
        assert_eq!(cache.default_ttl_secs.banner_ttl_secs, 0);
    }

    // -----------------------------------------------------------------------
    // MetricsConfig / DisabledMetrics tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_metrics_config_from_json() {
        let json = r#"{
            "prometheus": {
                "port": 9090,
                "namespace": "pbs",
                "subsystem": "auction",
                "path": "/metrics",
                "timeout_ms": 5000
            },
            "disabled_metrics": {
                "account_adapter_details": true,
                "account_debug": false,
                "adapter_connections_metrics": true
            }
        }"#;

        let m: MetricsConfig = serde_json::from_str(json).unwrap();
        let prom = m.prometheus.unwrap();
        assert_eq!(prom.port, 9090);
        assert_eq!(prom.namespace, "pbs");
        assert_eq!(prom.subsystem, "auction");
        assert_eq!(prom.path, "/metrics");
        assert_eq!(prom.timeout_ms, 5000);
        assert!(m.disabled_metrics.account_adapter_details);
        assert!(!m.disabled_metrics.account_debug);
        assert!(m.disabled_metrics.adapter_connections_metrics);
    }

    #[test]
    fn test_metrics_config_from_toml() {
        let toml_str = r#"
            [prometheus]
            port = 9100
            namespace = "prebid"
            subsystem = "server"
            path = "/prom"
            timeout_ms = 8000

            [disabled_metrics]
            account_adapter_details = false
            account_stored_responses = true
        "#;

        let m: MetricsConfig = toml::from_str(toml_str).unwrap();
        let prom = m.prometheus.unwrap();
        assert_eq!(prom.port, 9100);
        assert_eq!(prom.subsystem, "server");
        assert!(m.disabled_metrics.account_stored_responses);
        assert!(!m.disabled_metrics.account_adapter_details);
    }

    #[test]
    fn test_metrics_config_defaults() {
        let m = MetricsConfig::default();
        assert!(m.prometheus.is_none());
        assert!(m.influxdb.is_none());
        assert!(!m.disabled_metrics.account_adapter_details);
        assert!(!m.disabled_metrics.adapter_connections_metrics);
    }

    // -----------------------------------------------------------------------
    // Tcf2Config tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_tcf2_config_defaults() {
        let tcf2 = Tcf2Config::default();
        assert!(!tcf2.enabled);
        assert!(!tcf2.purpose1.enforce_purpose);
        assert!(!tcf2.purpose10.enforce_vendors);
        assert!(!tcf2.special_feature1.enforce);
        assert!(!tcf2.purpose_one_treatment.enabled);
    }

    #[test]
    fn test_tcf2_purpose_roundtrip_json() {
        let p = Tcf2PurposeConfig {
            enforce_algo: "full".into(),
            enforce_purpose: true,
            enforce_vendors: true,
            vendor_exceptions: vec!["appnexus".into(), "rubicon".into()],
        };
        let json = serde_json::to_string(&p).unwrap();
        let p2: Tcf2PurposeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(p2.enforce_algo, "full");
        assert!(p2.enforce_purpose);
        assert_eq!(p2.vendor_exceptions.len(), 2);
    }

    // -----------------------------------------------------------------------
    // AccountPrivacyConfig tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_account_privacy_from_json() {
        let json = r#"{
            "gdpr": {
                "enabled": true,
                "purpose1": {
                    "enforce_purpose": true,
                    "enforce_vendors": true,
                    "vendor_exceptions": ["bidder1"]
                }
            },
            "ccpa": { "enabled": false },
            "coppa": { "enabled": true },
            "ipv4": { "anon_keep_bits": 24 },
            "ipv6": { "anon_keep_bits": 48 }
        }"#;

        let p: AccountPrivacyConfig = serde_json::from_str(json).unwrap();
        assert_eq!(p.gdpr.enabled, Some(true));
        assert!(p.gdpr.purpose1.enforce_purpose);
        assert_eq!(p.gdpr.purpose1.vendor_exceptions, vec!["bidder1"]);
        assert_eq!(p.ccpa.enabled, Some(false));
        assert!(p.coppa.enabled);
        assert_eq!(p.ipv4.anon_keep_bits, 24);
        assert_eq!(p.ipv6.anon_keep_bits, 48);
    }

    // -----------------------------------------------------------------------
    // Database backend test
    // -----------------------------------------------------------------------

    #[test]
    fn test_stored_requests_database_from_json() {
        let json = r#"{
            "database": {
                "driver": "postgres",
                "connection_string": "postgres://user:pass@localhost/pbs",
                "fetch_query": "SELECT data FROM stored_requests WHERE id = $1",
                "amp_fetch_query": "SELECT data FROM stored_amp_requests WHERE id = $1"
            }
        }"#;

        let sr: StoredRequestConfig = serde_json::from_str(json).unwrap();
        let db = sr.database.unwrap();
        assert_eq!(db.driver, "postgres");
        assert!(db.connection_string.contains("localhost"));
        assert!(db.fetch_query.contains("stored_requests"));
    }

    // -----------------------------------------------------------------------
    // BidAdjustments round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn test_bid_adjustments_roundtrip() {
        let adj = BidAdjustmentsConfig {
            media_type: {
                let mut mt = HashMap::new();
                let mut bidders = HashMap::new();
                bidders.insert(
                    "appnexus".to_string(),
                    vec![BidAdjustmentRule {
                        adj_type: "multiplier".into(),
                        value: 1.1,
                        currency: "USD".into(),
                    }],
                );
                mt.insert("banner".to_string(), bidders);
                mt
            },
        };
        let json = serde_json::to_string(&adj).unwrap();
        let adj2: BidAdjustmentsConfig = serde_json::from_str(&json).unwrap();
        let rules = &adj2.media_type["banner"]["appnexus"];
        assert_eq!(rules.len(), 1);
        assert!((rules[0].value - 1.1).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // Full Configuration with new types (JSON round-trip)
    // -----------------------------------------------------------------------

    #[test]
    fn test_full_configuration_json_roundtrip() {
        let cfg = Configuration {
            host: "127.0.0.1".into(),
            port: 9000,
            gdpr: GDPRConfig {
                enabled: true,
                host_vendor_id: 5,
                tcf2: Tcf2Config {
                    enabled: true,
                    purpose1: Tcf2PurposeConfig {
                        enforce_algo: "full".into(),
                        enforce_purpose: true,
                        enforce_vendors: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
            ccpa: CCPAConfig { enforce: true },
            currency: CurrencyConfig {
                fetch_interval_seconds: 600,
                ..Default::default()
            },
            ..Default::default()
        };

        let json = serde_json::to_string(&cfg).unwrap();
        let cfg2: Configuration = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg2.host, "127.0.0.1");
        assert_eq!(cfg2.port, 9000);
        assert!(cfg2.gdpr.enabled);
        assert_eq!(cfg2.gdpr.host_vendor_id, 5);
        assert!(cfg2.gdpr.tcf2.enabled);
        assert!(cfg2.gdpr.tcf2.purpose1.enforce_purpose);
        assert!(cfg2.ccpa.enforce);
        assert_eq!(cfg2.currency.fetch_interval_seconds, 600);
    }

    // -----------------------------------------------------------------------
    // InMemoryCacheConfig "type" rename
    // -----------------------------------------------------------------------

    #[test]
    fn test_in_memory_cache_type_field_rename() {
        let json = r#"{ "type": "lru", "ttl_seconds": 120, "size_bytes": 1048576 }"#;
        let c: InMemoryCacheConfig = serde_json::from_str(json).unwrap();
        assert_eq!(c.cache_type, "lru");
        assert_eq!(c.ttl_seconds, 120);
        assert_eq!(c.size_bytes, 1048576);
    }

    // -----------------------------------------------------------------------
    // HttpEventsConfig tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_http_events_config_from_toml() {
        let toml_str = r#"
            endpoint = "https://events.example.com"
            refresh_rate_seconds = 30
            timeout_ms = 1000
            amp_endpoint = "https://events.example.com/amp"
        "#;
        let he: HttpEventsConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(he.endpoint, "https://events.example.com");
        assert_eq!(he.refresh_rate_seconds, 30);
        assert_eq!(he.timeout_ms, 1000);
        assert_eq!(he.amp_endpoint, "https://events.example.com/amp");
    }

    // -----------------------------------------------------------------------
    // Configuration::from_file tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_from_file_json() {
        let dir = std::env::temp_dir().join("pbs_config_test_from_file_json");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_config.json");
        std::fs::write(
            &path,
            r#"{
                "host": "10.0.0.1",
                "port": 9999,
                "admin_port": 7070,
                "gdpr": { "default_value": "0", "enabled": true }
            }"#,
        )
        .unwrap();

        let cfg = Configuration::from_file(path.to_str().unwrap()).unwrap();
        assert_eq!(cfg.host, "10.0.0.1");
        assert_eq!(cfg.port, 9999);
        assert_eq!(cfg.admin_port, 7070);
        assert!(cfg.gdpr.enabled);
        assert_eq!(cfg.gdpr.default_value, "0");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_from_file_yaml() {
        let dir = std::env::temp_dir().join("pbs_config_test_from_file_yaml");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_config.yaml");
        std::fs::write(
            &path,
            "host: 192.168.1.1\nport: 8888\ngdpr:\n  default_value: \"1\"\n  enabled: false\n",
        )
        .unwrap();

        let cfg = Configuration::from_file(path.to_str().unwrap()).unwrap();
        assert_eq!(cfg.host, "192.168.1.1");
        assert_eq!(cfg.port, 8888);
        assert!(!cfg.gdpr.enabled);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_from_file_toml() {
        let dir = std::env::temp_dir().join("pbs_config_test_from_file_toml");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_config.toml");
        std::fs::write(
            &path,
            "host = \"172.16.0.1\"\nport = 7777\n\n[gdpr]\ndefault_value = \"1\"\nenabled = false\n",
        )
        .unwrap();

        let cfg = Configuration::from_file(path.to_str().unwrap()).unwrap();
        assert_eq!(cfg.host, "172.16.0.1");
        assert_eq!(cfg.port, 7777);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_from_file_not_found() {
        let result = Configuration::from_file("/nonexistent/path/config.json");
        assert!(result.is_err());
    }

    #[test]
    fn test_from_files_merge() {
        let dir = std::env::temp_dir().join("pbs_config_test_merge");
        let _ = std::fs::create_dir_all(&dir);

        let base_path = dir.join("base.json");
        std::fs::write(
            &base_path,
            r#"{ "host": "0.0.0.0", "port": 8000, "admin_port": 6060 }"#,
        )
        .unwrap();

        let override_path = dir.join("override.json");
        std::fs::write(
            &override_path,
            r#"{ "port": 9001, "enable_cors": true }"#,
        )
        .unwrap();

        let cfg = Configuration::from_files(&[
            base_path.to_str().unwrap(),
            override_path.to_str().unwrap(),
        ])
        .unwrap();
        assert_eq!(cfg.host, "0.0.0.0");
        assert_eq!(cfg.port, 9001); // overridden
        assert!(cfg.enable_cors); // from override

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // with_env_overrides tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_with_env_overrides_chaining() {
        let old_port = std::env::var("PBS_PORT").ok();
        std::env::set_var("PBS_PORT", "4321");

        let cfg = Configuration::default().with_env_overrides();
        assert_eq!(cfg.port, 4321);

        match old_port {
            Some(v) => std::env::set_var("PBS_PORT", v),
            None => std::env::remove_var("PBS_PORT"),
        }
    }

    #[test]
    fn test_env_override_adapter_endpoint() {
        let key = "PBS_ADAPTERS_TESTBIDDER_ENDPOINT";
        let old = std::env::var(key).ok();
        std::env::set_var(key, "https://test.bidder.com/bid");

        let mut cfg = Configuration::default();
        cfg.apply_env_overrides();
        assert_eq!(
            cfg.adapters.get("testbidder").unwrap().endpoint,
            "https://test.bidder.com/bid"
        );

        match old {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    fn test_env_override_gdpr_syncs_both_fields() {
        let old = std::env::var("PBS_GDPR_ENABLED").ok();
        std::env::set_var("PBS_GDPR_ENABLED", "1");

        let mut cfg = Configuration::default();
        cfg.apply_env_overrides();
        assert!(cfg.gdpr_enabled);
        assert!(cfg.gdpr.enabled);

        match old {
            Some(v) => std::env::set_var("PBS_GDPR_ENABLED", v),
            None => std::env::remove_var("PBS_GDPR_ENABLED"),
        }
    }

    // -----------------------------------------------------------------------
    // Configuration::validate tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_default_config_passes() {
        let cfg = Configuration {
            host: "0.0.0.0".into(),
            port: 8000,
            stored_requests_timeout_ms: 50,
            gdpr: GDPRConfig::default(),
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_validate_empty_host_fails() {
        let cfg = Configuration {
            host: String::new(),
            port: 8000,
            stored_requests_timeout_ms: 50,
            ..Default::default()
        };
        let errs = cfg.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("host must not be empty")));
    }

    #[test]
    fn test_validate_zero_port_fails() {
        let cfg = Configuration {
            host: "0.0.0.0".into(),
            port: 0,
            stored_requests_timeout_ms: 50,
            ..Default::default()
        };
        let errs = cfg.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("port must be greater than 0")));
    }

    #[test]
    fn test_validate_auction_timeout_max_less_than_default() {
        let cfg = Configuration {
            host: "0.0.0.0".into(),
            port: 8000,
            stored_requests_timeout_ms: 50,
            auction_timeouts: AuctionTimeouts {
                default: 5000,
                max: 1000,
            },
            ..Default::default()
        };
        let errs = cfg.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("cannot be less than")));
    }

    #[test]
    fn test_validate_invalid_gdpr_default_value() {
        let cfg = Configuration {
            host: "0.0.0.0".into(),
            port: 8000,
            stored_requests_timeout_ms: 50,
            gdpr: GDPRConfig {
                default_value: "2".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let errs = cfg.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("gdpr.default_value must be")));
    }

    #[test]
    fn test_validate_invalid_adapter_url() {
        let mut adapters = HashMap::new();
        adapters.insert(
            "bad_bidder".to_string(),
            AdapterConfig {
                endpoint: "not-a-url".into(),
                enabled: true,
                ..Default::default()
            },
        );
        let cfg = Configuration {
            host: "0.0.0.0".into(),
            port: 8000,
            stored_requests_timeout_ms: 50,
            adapters,
            ..Default::default()
        };
        let errs = cfg.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("not a valid URL")));
    }

    #[test]
    fn test_validate_valid_adapter_url() {
        let mut adapters = HashMap::new();
        adapters.insert(
            "good_bidder".to_string(),
            AdapterConfig {
                endpoint: "https://bid.example.com/openrtb2".into(),
                enabled: true,
                ..Default::default()
            },
        );
        let cfg = Configuration {
            host: "0.0.0.0".into(),
            port: 8000,
            stored_requests_timeout_ms: 50,
            adapters,
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_validate_external_cache_missing_path() {
        let cfg = Configuration {
            host: "0.0.0.0".into(),
            port: 8000,
            stored_requests_timeout_ms: 50,
            external_cache: ExternalCacheConfig {
                host: "cache.example.com".into(),
                path: String::new(),
                ..Default::default()
            },
            ..Default::default()
        };
        let errs = cfg.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("must both be specified")));
    }

    #[test]
    fn test_validate_external_cache_bad_scheme() {
        let cfg = Configuration {
            host: "0.0.0.0".into(),
            port: 8000,
            stored_requests_timeout_ms: 50,
            external_cache: ExternalCacheConfig {
                scheme: "ftp".into(),
                host: "cache.example.com".into(),
                path: "/cache".into(),
            },
            ..Default::default()
        };
        let errs = cfg.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("scheme must be http or https")));
    }

    #[test]
    fn test_validate_tcf2_invalid_enforce_algo() {
        let cfg = Configuration {
            host: "0.0.0.0".into(),
            port: 8000,
            stored_requests_timeout_ms: 50,
            gdpr: GDPRConfig {
                tcf2: Tcf2Config {
                    purpose1: Tcf2PurposeConfig {
                        enforce_algo: "invalid".into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let errs = cfg.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("enforce_algo")));
    }

    #[test]
    fn test_validate_prometheus_timeout_zero_with_port() {
        let cfg = Configuration {
            host: "0.0.0.0".into(),
            port: 8000,
            stored_requests_timeout_ms: 50,
            metrics: MetricsConfig {
                prometheus: Some(PrometheusConfig {
                    port: 9090,
                    timeout_ms: 0,
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let errs = cfg.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("prometheus.timeout_ms")));
    }

    #[test]
    fn test_validate_debug_sampling_rate_out_of_range() {
        let cfg = Configuration {
            host: "0.0.0.0".into(),
            port: 8000,
            stored_requests_timeout_ms: 50,
            debug: DebugConfig {
                timeout_notification: TimeoutNotificationConfig {
                    sampling_rate: 1.5,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let errs = cfg.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("sampling_rate")));
    }

    #[test]
    fn test_validate_unreasonable_timeout() {
        let cfg = Configuration {
            host: "0.0.0.0".into(),
            port: 8000,
            stored_requests_timeout_ms: 50,
            auction_timeouts: AuctionTimeouts {
                default: 1000,
                max: 120_000,
            },
            ..Default::default()
        };
        let errs = cfg.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("unreasonably large")));
    }

    #[test]
    fn test_validate_price_floors_enforce_rate_out_of_range() {
        let cfg = Configuration {
            host: "0.0.0.0".into(),
            port: 8000,
            stored_requests_timeout_ms: 50,
            account_defaults: AccountConfig {
                price_floors: AccountPriceFloorsConfig {
                    enforce_floors_rate: 200,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let errs = cfg.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("enforce_floors_rate")));
    }

    // -----------------------------------------------------------------------
    // AccountFetcher tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_in_memory_account_fetcher() {
        let mut accounts = HashMap::new();
        accounts.insert(
            "acct-1".to_string(),
            AccountConfig {
                id: "acct-1".into(),
                disabled: false,
                ..Default::default()
            },
        );
        let fetcher = InMemoryAccountFetcher::new(accounts);

        let acct = fetcher.fetch_account("acct-1").unwrap();
        assert_eq!(acct.id, "acct-1");
        assert!(!acct.disabled);

        let result = fetcher.fetch_account("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_in_memory_fetcher_from_config() {
        let mut cfg = Configuration::default();
        cfg.accounts.insert(
            "pub-100".to_string(),
            AccountConfig {
                id: "pub-100".into(),
                debug_allow: true,
                ..Default::default()
            },
        );
        let fetcher = InMemoryAccountFetcher::from_config(&cfg);
        let acct = fetcher.fetch_account("pub-100").unwrap();
        assert!(acct.debug_allow);
    }

    #[test]
    fn test_file_account_fetcher() {
        let dir = std::env::temp_dir().join("pbs_test_file_account_fetcher");
        let _ = std::fs::create_dir_all(&dir);

        let acct_path = dir.join("test-acct.json");
        std::fs::write(
            &acct_path,
            r#"{ "id": "test-acct", "disabled": false, "debug_allow": true }"#,
        )
        .unwrap();

        let fetcher = FileAccountFetcher::new(&dir);
        let acct = fetcher.fetch_account("test-acct").unwrap();
        assert_eq!(acct.id, "test-acct");
        assert!(acct.debug_allow);

        // Not found
        let result = fetcher.fetch_account("missing");
        assert!(result.is_err());

        // Empty ID
        let result = fetcher.fetch_account("");
        assert!(result.is_err());

        // Path traversal
        let result = fetcher.fetch_account("../etc/passwd");
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_http_account_fetcher_stub_errors() {
        let fetcher = HttpAccountFetcher::new("https://accounts.example.com");
        let result = fetcher.fetch_account("some-id");
        assert!(result.is_err());
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(
            err_msg.contains("HTTP account fetching is not available")
                || err_msg.contains("failed to fetch account"),
            "unexpected error: {}",
            err_msg
        );
    }

    #[test]
    fn test_http_account_fetcher_empty_id() {
        let fetcher = HttpAccountFetcher::new("https://accounts.example.com");
        let result = fetcher.fetch_account("");
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // BidderInfo tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_bidder_info_from_yaml() {
        let yaml = r#"
endpoint: "https://bid.appnexus.com/openrtb2"
maintainer:
  email: "info@appnexus.com"
capabilities:
  app:
    mediaTypes:
      - banner
      - video
  site:
    mediaTypes:
      - banner
      - video
      - native
gvlVendorID: 32
modifyingVastXmlAllowed: true
userSync:
  key: appnexus
  supports:
    - iframe
    - redirect
  iframe:
    url: "https://ib.adnxs.com/getuid?https://host/setuid"
    userMacro: "$UID"
"#;
        let info: BidderInfo = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(info.endpoint, "https://bid.appnexus.com/openrtb2");
        assert_eq!(info.maintainer.as_ref().unwrap().email, "info@appnexus.com");
        let caps = info.capabilities.as_ref().unwrap();
        assert!(caps.app.is_some());
        assert!(caps.site.is_some());
        assert!(caps.dooh.is_none());
        assert_eq!(
            caps.app.as_ref().unwrap().media_types,
            vec!["banner", "video"]
        );
        assert_eq!(info.gvl_vendor_id, 32);
        assert!(info.modifying_vast_xml_allowed);
        let sync = info.user_sync.as_ref().unwrap();
        assert_eq!(sync.key, "appnexus");
        assert_eq!(sync.supports, vec!["iframe", "redirect"]);
        assert_eq!(
            sync.iframe.as_ref().unwrap().user_macro,
            "$UID"
        );
    }

    #[test]
    fn test_bidder_info_defaults() {
        let info = BidderInfo::default();
        assert!(info.endpoint.is_empty());
        assert!(!info.disabled);
        assert!(info.maintainer.is_none());
        assert!(info.capabilities.is_none());
        assert_eq!(info.gvl_vendor_id, 0);
    }

    #[test]
    fn test_load_bidder_info_from_dir() {
        let dir = std::env::temp_dir().join("pbs_test_load_bidder_info");
        let _ = std::fs::create_dir_all(&dir);

        // Write two bidder files
        std::fs::write(
            dir.join("appnexus.yaml"),
            "endpoint: \"https://bid.appnexus.com\"\nmaintainer:\n  email: \"a@b.com\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("rubicon.yaml"),
            "endpoint: \"https://bid.rubicon.com\"\ngvlVendorID: 52\n",
        )
        .unwrap();
        // Write a non-yaml file that should be skipped
        std::fs::write(dir.join("README.md"), "This should be skipped").unwrap();

        let bidders = load_bidder_info(dir.to_str().unwrap()).unwrap();
        assert_eq!(bidders.len(), 2);
        assert_eq!(
            bidders["appnexus"].endpoint,
            "https://bid.appnexus.com"
        );
        assert_eq!(
            bidders["rubicon"].endpoint,
            "https://bid.rubicon.com"
        );
        assert_eq!(bidders["rubicon"].gvl_vendor_id, 52);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_bidder_info_nonexistent_dir() {
        let result = load_bidder_info("/nonexistent/bidder/dir");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_bidder_info_empty_dir() {
        let dir = std::env::temp_dir().join("pbs_test_load_bidder_info_empty");
        let _ = std::fs::create_dir_all(&dir);

        let bidders = load_bidder_info(dir.to_str().unwrap()).unwrap();
        assert!(bidders.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // is_valid_url helper tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_valid_url() {
        assert!(is_valid_url("https://example.com"));
        assert!(is_valid_url("http://localhost:8080/path"));
        assert!(is_valid_url("https://bid.appnexus.com/openrtb2"));
        assert!(!is_valid_url("not-a-url"));
        assert!(!is_valid_url(""));
        assert!(!is_valid_url("://missing-scheme"));
        assert!(!is_valid_url("ftp://"));  // empty host
    }
}

// ---------------------------------------------------------------------
// Multi-source configuration loading and validation (additional modules)
// ---------------------------------------------------------------------
pub mod top;
pub mod defaults;
pub mod loader;
pub mod validation;
pub mod sources;
