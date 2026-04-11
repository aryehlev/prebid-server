//! Analytics module for prebid-server.
//!
//! Provides the [`AnalyticsModule`] trait that analytics backends must implement,
//! along with concrete implementations:
//!
//! - [`NoopAnalyticsModule`] -- silently discards all events (for disabled scenarios).
//! - [`FileLogger`] -- writes JSON-serialized events to a file asynchronously.
//! - [`LogAggregator`] -- fans out each event to multiple analytics modules.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// Request type tag (mirrors Go's filesystem.RequestType)
// ---------------------------------------------------------------------------

/// Identifies the endpoint that generated an analytics event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequestType {
    #[serde(rename = "/openrtb2/auction")]
    Auction,
    #[serde(rename = "/openrtb2/video")]
    Video,
    #[serde(rename = "/cookie_sync")]
    CookieSync,
    #[serde(rename = "/set_uid")]
    SetUid,
    #[serde(rename = "/openrtb2/amp")]
    Amp,
    #[serde(rename = "/event")]
    NotificationEvent,
}

// ---------------------------------------------------------------------------
// Event / notification enums (mirrors Go analytics/event.go)
// ---------------------------------------------------------------------------

/// The type of notification event Prebid Server can receive for an ad.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventType {
    Win,
    Imp,
    Vast,
}

/// Response format for an event endpoint response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseFormat {
    /// Returns HTTP 200 with an empty body.
    #[serde(rename = "b")]
    Blank,
    /// Returns HTTP 200 with a PNG body.
    #[serde(rename = "i")]
    Image,
}

/// VAST event sub-type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VastType {
    Start,
    FirstQuartile,
    MidPoint,
    ThirdQuartile,
    Complete,
}

/// Whether analytics processing is enabled for a given notification event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalyticsEnabled {
    #[serde(rename = "1")]
    Enabled,
    #[serde(rename = "0")]
    Disabled,
}

// ---------------------------------------------------------------------------
// Loggable objects -- one per endpoint
// ---------------------------------------------------------------------------

/// Loggable object of a transaction at `/openrtb2/auction`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuctionObject {
    pub status: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    pub start_time: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hook_execution_outcome: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seat_non_bid: Vec<serde_json::Value>,
}

/// Loggable object of a transaction at `/openrtb2/video`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoObject {
    pub status: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_request: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_response: Option<serde_json::Value>,
    pub start_time: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seat_non_bid: Vec<serde_json::Value>,
}

/// Loggable object of a transaction at `/cookie_sync`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieSyncObject {
    pub status: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bidder_status: Vec<CookieSyncBidder>,
}

/// Status of a single bidder within a cookie-sync response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieSyncBidder {
    pub bidder: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_cookie: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usersync: Option<UsersyncInfo>,
}

/// Usersync URL and type for a bidder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsersyncInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub sync_type: Option<String>,
}

/// Loggable object of a transaction at `/set_uid`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetUIDObject {
    pub status: i32,
    pub bidder: String,
    pub uid: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    pub success: bool,
}

/// Loggable object of a transaction at `/openrtb2/amp`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmpObject {
    pub status: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auction_response: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub amp_targeting_values: HashMap<String, String>,
    #[serde(default)]
    pub origin: String,
    pub start_time: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hook_execution_outcome: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seat_non_bid: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<serde_json::Value>,
}

/// Request payload for an `/event` notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRequest {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub event_type: Option<EventType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analytics: Option<AnalyticsEnabled>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vtype: Option<VastType>,
}

/// Loggable object of a transaction at `/event`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<EventRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
}

// ---------------------------------------------------------------------------
// Legacy AuctionEvent -- kept for backward compatibility with existing callers
// ---------------------------------------------------------------------------

/// Simplified auction event used by the exchange for quick logging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuctionEvent {
    pub timestamp: i64,
    pub request_id: String,
    pub status: String,
    pub bidder_count: usize,
    pub bid_count: usize,
    pub total_revenue: f64,
}

// ---------------------------------------------------------------------------
// Trait: AnalyticsModule
// ---------------------------------------------------------------------------

/// Core analytics trait. Every analytics backend must implement this.
///
/// All methods are async-compatible and take `&self` so implementations can be
/// shared behind an `Arc`. Errors during logging should be handled internally
/// (e.g. via `tracing::error!`) rather than propagated to callers.
#[async_trait]
pub trait AnalyticsModule: Send + Sync {
    /// Log an auction endpoint transaction.
    async fn log_auction_object(&self, ao: &AuctionObject);

    /// Log a video endpoint transaction.
    async fn log_video_object(&self, vo: &VideoObject);

    /// Log a cookie-sync endpoint transaction.
    async fn log_cookie_sync_object(&self, cso: &CookieSyncObject);

    /// Log a set-uid endpoint transaction.
    async fn log_setuid_object(&self, so: &SetUIDObject);

    /// Log an AMP endpoint transaction.
    async fn log_amp_object(&self, ao: &AmpObject);

    /// Log a notification event.
    async fn log_notification_event(&self, ne: &NotificationEvent);

    /// Gracefully shut down the module (flush buffers, etc.).
    async fn shutdown(&self);
}

// ---------------------------------------------------------------------------
// Legacy trait alias -- lets existing `Exchange.analytics` keep compiling
// ---------------------------------------------------------------------------

/// Legacy trait kept for backward compatibility. New code should prefer
/// [`AnalyticsModule`].
pub trait AnalyticsBackend: Send + Sync {
    fn log_auction(&self, event: AuctionEvent);
}

// ---------------------------------------------------------------------------
// NoopAnalyticsModule
// ---------------------------------------------------------------------------

/// A no-op analytics module that silently discards all events.
/// Use this when analytics is disabled.
pub struct NoopAnalyticsModule;

#[async_trait]
impl AnalyticsModule for NoopAnalyticsModule {
    async fn log_auction_object(&self, _ao: &AuctionObject) {}
    async fn log_video_object(&self, _vo: &VideoObject) {}
    async fn log_cookie_sync_object(&self, _cso: &CookieSyncObject) {}
    async fn log_setuid_object(&self, _so: &SetUIDObject) {}
    async fn log_amp_object(&self, _ao: &AmpObject) {}
    async fn log_notification_event(&self, _ne: &NotificationEvent) {}
    async fn shutdown(&self) {}
}

impl AnalyticsBackend for NoopAnalyticsModule {
    fn log_auction(&self, _: AuctionEvent) {}
}

// ---------------------------------------------------------------------------
// FileLogger
// ---------------------------------------------------------------------------

/// Envelope written for each event so the log file is self-describing.
#[derive(Serialize)]
struct LogEnvelope<T: Serialize> {
    #[serde(rename = "type")]
    request_type: RequestType,
    #[serde(flatten)]
    payload: T,
}

/// Internal message type sent over the channel to the writer task.
enum LogMessage {
    Line(String),
    Shutdown(tokio::sync::oneshot::Sender<()>),
}

/// Async file logger that serializes analytics objects to JSON, one line per
/// event, and writes them to a file via a dedicated background task.
pub struct FileLogger {
    sender: mpsc::UnboundedSender<LogMessage>,
}

impl FileLogger {
    /// Create a new `FileLogger` that appends JSON lines to `path`.
    ///
    /// The file is created if it does not exist. A background tokio task is
    /// spawned to perform the actual I/O so callers are never blocked.
    pub fn new(path: &str) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let path = path.to_string();

        tokio::spawn(async move {
            let file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await;

            let mut file = match file {
                Ok(f) => f,
                Err(e) => {
                    tracing::error!(path = %path, error = %e, "failed to open analytics log file");
                    return;
                }
            };

            while let Some(msg) = rx.recv().await {
                match msg {
                    LogMessage::Line(line) => {
                        if let Err(e) = file.write_all(line.as_bytes()).await {
                            tracing::error!(error = %e, "failed to write analytics line");
                        }
                        if let Err(e) = file.flush().await {
                            tracing::error!(error = %e, "failed to flush analytics file");
                        }
                    }
                    LogMessage::Shutdown(done) => {
                        let _ = file.flush().await;
                        let _ = done.send(());
                        break;
                    }
                }
            }
        });

        Self { sender: tx }
    }

    /// Serialize and enqueue a log line. Errors are logged via `tracing`.
    fn enqueue<T: Serialize>(&self, request_type: RequestType, payload: &T) {
        let envelope = LogEnvelope {
            request_type,
            payload,
        };
        match serde_json::to_string(&envelope) {
            Ok(json) => {
                let line = format!("{}\n", json);
                if self.sender.send(LogMessage::Line(line)).is_err() {
                    tracing::error!("analytics file writer channel closed");
                }
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    "failed to serialize analytics object"
                );
            }
        }
    }
}

#[async_trait]
impl AnalyticsModule for FileLogger {
    async fn log_auction_object(&self, ao: &AuctionObject) {
        self.enqueue(RequestType::Auction, ao);
    }

    async fn log_video_object(&self, vo: &VideoObject) {
        self.enqueue(RequestType::Video, vo);
    }

    async fn log_cookie_sync_object(&self, cso: &CookieSyncObject) {
        self.enqueue(RequestType::CookieSync, cso);
    }

    async fn log_setuid_object(&self, so: &SetUIDObject) {
        self.enqueue(RequestType::SetUid, so);
    }

    async fn log_amp_object(&self, ao: &AmpObject) {
        self.enqueue(RequestType::Amp, ao);
    }

    async fn log_notification_event(&self, ne: &NotificationEvent) {
        self.enqueue(RequestType::NotificationEvent, ne);
    }

    async fn shutdown(&self) {
        tracing::info!("FileLogger shutting down, flushing buffer");
        let (tx, rx) = tokio::sync::oneshot::channel();
        if self.sender.send(LogMessage::Shutdown(tx)).is_ok() {
            let _ = rx.await;
        }
    }
}

impl AnalyticsBackend for FileLogger {
    fn log_auction(&self, event: AuctionEvent) {
        match serde_json::to_string(&event) {
            Ok(json) => {
                let line = format!("{}\n", json);
                let _ = self.sender.send(LogMessage::Line(line));
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to serialize legacy AuctionEvent");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// HttpAnalytics — batch events and POST to HTTP endpoint
// ---------------------------------------------------------------------------

/// HTTP analytics backend that batches events and POSTs them to a configurable
/// endpoint. Mirrors Go analytics/pubstack/pubstack_module.go pattern.
pub struct HttpAnalytics {
    sender: mpsc::UnboundedSender<HttpAnalyticsMessage>,
}

enum HttpAnalyticsMessage {
    Event(String),
    Shutdown(tokio::sync::oneshot::Sender<()>),
}

/// Configuration for the HTTP analytics backend.
#[derive(Debug, Clone)]
pub struct HttpAnalyticsConfig {
    /// HTTP endpoint to POST events to.
    pub endpoint: String,
    /// Maximum number of events to batch before sending.
    pub batch_size: usize,
    /// Maximum time (ms) to wait before flushing a partial batch.
    pub flush_interval_ms: u64,
    /// HTTP request timeout in ms.
    pub timeout_ms: u64,
}

impl Default for HttpAnalyticsConfig {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            batch_size: 100,
            flush_interval_ms: 5000,
            timeout_ms: 10000,
        }
    }
}

impl HttpAnalytics {
    /// Create a new HTTP analytics backend. Spawns a background task for batching.
    pub fn new(config: HttpAnalyticsConfig) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<HttpAnalyticsMessage>();

        tokio::spawn(async move {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_millis(config.timeout_ms))
                .build()
                .unwrap_or_default();

            let mut batch: Vec<String> = Vec::with_capacity(config.batch_size);
            let flush_interval = tokio::time::Duration::from_millis(config.flush_interval_ms);
            let mut timer = tokio::time::interval(flush_interval);

            loop {
                tokio::select! {
                    msg = rx.recv() => {
                        match msg {
                            Some(HttpAnalyticsMessage::Event(json)) => {
                                batch.push(json);
                                if batch.len() >= config.batch_size {
                                    Self::flush_batch(&client, &config.endpoint, &mut batch).await;
                                }
                            }
                            Some(HttpAnalyticsMessage::Shutdown(done)) => {
                                if !batch.is_empty() {
                                    Self::flush_batch(&client, &config.endpoint, &mut batch).await;
                                }
                                let _ = done.send(());
                                break;
                            }
                            None => break,
                        }
                    }
                    _ = timer.tick() => {
                        if !batch.is_empty() {
                            Self::flush_batch(&client, &config.endpoint, &mut batch).await;
                        }
                    }
                }
            }
        });

        Self { sender: tx }
    }

    async fn flush_batch(client: &reqwest::Client, endpoint: &str, batch: &mut Vec<String>) {
        if endpoint.is_empty() || batch.is_empty() {
            batch.clear();
            return;
        }
        let payload = format!("[{}]", batch.join(","));
        batch.clear();

        // Retry with backoff
        for attempt in 0..3u32 {
            match client.post(endpoint).body(payload.clone()).send().await {
                Ok(resp) if resp.status().is_success() => return,
                Ok(resp) => {
                    tracing::warn!(
                        status = %resp.status(), attempt, "HTTP analytics flush failed"
                    );
                }
                Err(e) => {
                    tracing::warn!(error = %e, attempt, "HTTP analytics flush error");
                }
            }
            if attempt < 2 {
                tokio::time::sleep(tokio::time::Duration::from_millis(100 * (1 << attempt))).await;
            }
        }
    }

    fn enqueue<T: Serialize>(&self, request_type: RequestType, payload: &T) {
        let envelope = LogEnvelope { request_type, payload };
        match serde_json::to_string(&envelope) {
            Ok(json) => {
                let _ = self.sender.send(HttpAnalyticsMessage::Event(json));
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to serialize analytics event for HTTP");
            }
        }
    }
}

#[async_trait]
impl AnalyticsModule for HttpAnalytics {
    async fn log_auction_object(&self, ao: &AuctionObject) {
        self.enqueue(RequestType::Auction, ao);
    }
    async fn log_video_object(&self, vo: &VideoObject) {
        self.enqueue(RequestType::Video, vo);
    }
    async fn log_cookie_sync_object(&self, cso: &CookieSyncObject) {
        self.enqueue(RequestType::CookieSync, cso);
    }
    async fn log_setuid_object(&self, so: &SetUIDObject) {
        self.enqueue(RequestType::SetUid, so);
    }
    async fn log_amp_object(&self, ao: &AmpObject) {
        self.enqueue(RequestType::Amp, ao);
    }
    async fn log_notification_event(&self, ne: &NotificationEvent) {
        self.enqueue(RequestType::NotificationEvent, ne);
    }
    async fn shutdown(&self) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if self.sender.send(HttpAnalyticsMessage::Shutdown(tx)).is_ok() {
            let _ = rx.await;
        }
    }
}

// ---------------------------------------------------------------------------
// LogAnalytics — structured logging backend using tracing
// ---------------------------------------------------------------------------

/// A simple analytics backend that emits events using the `tracing` crate.
pub struct LogAnalytics {
    /// Log level to use (e.g. "info", "debug").
    pub level: String,
}

impl LogAnalytics {
    pub fn new(level: &str) -> Self {
        Self { level: level.to_string() }
    }

    fn log_json<T: Serialize>(&self, request_type: RequestType, payload: &T) {
        if let Ok(json) = serde_json::to_string(payload) {
            match self.level.as_str() {
                "debug" => tracing::debug!(request_type = ?request_type, payload = %json, "analytics"),
                "trace" => tracing::trace!(request_type = ?request_type, payload = %json, "analytics"),
                _ => tracing::info!(request_type = ?request_type, payload = %json, "analytics"),
            }
        }
    }
}

#[async_trait]
impl AnalyticsModule for LogAnalytics {
    async fn log_auction_object(&self, ao: &AuctionObject) {
        self.log_json(RequestType::Auction, ao);
    }
    async fn log_video_object(&self, vo: &VideoObject) {
        self.log_json(RequestType::Video, vo);
    }
    async fn log_cookie_sync_object(&self, cso: &CookieSyncObject) {
        self.log_json(RequestType::CookieSync, cso);
    }
    async fn log_setuid_object(&self, so: &SetUIDObject) {
        self.log_json(RequestType::SetUid, so);
    }
    async fn log_amp_object(&self, ao: &AmpObject) {
        self.log_json(RequestType::Amp, ao);
    }
    async fn log_notification_event(&self, ne: &NotificationEvent) {
        self.log_json(RequestType::NotificationEvent, ne);
    }
    async fn shutdown(&self) {}
}

// ---------------------------------------------------------------------------
// LogAggregator
// ---------------------------------------------------------------------------

/// Dispatches analytics events to multiple [`AnalyticsModule`] implementations.
///
/// This mirrors the Go `analytics.enabledAnalytics` pattern where several
/// modules can be active at the same time (e.g. file + pubstack).
pub struct LogAggregator {
    modules: Vec<Arc<dyn AnalyticsModule>>,
}

impl LogAggregator {
    /// Create an aggregator with the given modules.
    pub fn new(modules: Vec<Arc<dyn AnalyticsModule>>) -> Self {
        Self { modules }
    }

    /// Returns `true` if the aggregator has no modules.
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// Returns the number of registered modules.
    pub fn len(&self) -> usize {
        self.modules.len()
    }
}

#[async_trait]
impl AnalyticsModule for LogAggregator {
    async fn log_auction_object(&self, ao: &AuctionObject) {
        for m in &self.modules {
            m.log_auction_object(ao).await;
        }
    }

    async fn log_video_object(&self, vo: &VideoObject) {
        for m in &self.modules {
            m.log_video_object(vo).await;
        }
    }

    async fn log_cookie_sync_object(&self, cso: &CookieSyncObject) {
        for m in &self.modules {
            m.log_cookie_sync_object(cso).await;
        }
    }

    async fn log_setuid_object(&self, so: &SetUIDObject) {
        for m in &self.modules {
            m.log_setuid_object(so).await;
        }
    }

    async fn log_amp_object(&self, ao: &AmpObject) {
        for m in &self.modules {
            m.log_amp_object(ao).await;
        }
    }

    async fn log_notification_event(&self, ne: &NotificationEvent) {
        for m in &self.modules {
            m.log_notification_event(ne).await;
        }
    }

    async fn shutdown(&self) {
        for m in &self.modules {
            m.shutdown().await;
        }
    }
}

// ---------------------------------------------------------------------------
// PubstackAnalytics — per-channel buffered analytics (mirrors Go pubstack)
// ---------------------------------------------------------------------------

/// Feature flags controlling which event types Pubstack collects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PubstackFeatures {
    #[serde(default)]
    pub auction: bool,
    #[serde(default)]
    pub video: bool,
    #[serde(default)]
    pub amp: bool,
    #[serde(default)]
    pub cookie_sync: bool,
    #[serde(default)]
    pub set_uid: bool,
}

impl Default for PubstackFeatures {
    fn default() -> Self {
        Self {
            auction: true,
            video: true,
            amp: true,
            cookie_sync: true,
            set_uid: true,
        }
    }
}

/// Remote configuration returned by the Pubstack `/bootstrap` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PubstackRemoteConfig {
    #[serde(rename = "scopeId")]
    pub scope_id: String,
    pub endpoint: String,
    pub features: HashMap<String, bool>,
}

/// Configuration for the Pubstack analytics module.
#[derive(Debug, Clone)]
pub struct PubstackConfig {
    /// Unique scope / publisher identifier.
    pub scope_id: String,
    /// Base endpoint URL (e.g. `https://analytics.pubstack.io`).
    pub endpoint: String,
    /// Feature flags for each event type.
    pub features: PubstackFeatures,
    /// Maximum number of events per channel buffer before flush.
    pub max_event_count: usize,
    /// Maximum byte size of the gzip buffer before flush.
    pub max_byte_size: usize,
    /// Maximum time between flushes.
    pub flush_interval: std::time::Duration,
    /// HTTP request timeout.
    pub http_timeout: std::time::Duration,
    /// Interval at which to poll the remote endpoint for config updates.
    /// Set to `None` to disable remote configuration refresh.
    pub config_refresh_interval: Option<std::time::Duration>,
}

impl Default for PubstackConfig {
    fn default() -> Self {
        Self {
            scope_id: String::new(),
            endpoint: String::new(),
            features: PubstackFeatures::default(),
            max_event_count: 2000,
            max_byte_size: 100 * 1024 * 1024, // 100 MiB
            flush_interval: std::time::Duration::from_secs(30),
            http_timeout: std::time::Duration::from_secs(10),
            config_refresh_interval: Some(std::time::Duration::from_secs(600)),
        }
    }
}

/// Envelope that adds `scope` to each Pubstack event payload.
#[derive(Serialize)]
struct PubstackEnvelope<'a, T: Serialize> {
    scope: &'a str,
    #[serde(flatten)]
    payload: &'a T,
}

/// Internal message sent to a Pubstack event channel worker.
enum PubstackEventMsg {
    Event(Vec<u8>),
    Shutdown(tokio::sync::oneshot::Sender<()>),
}

/// A single event-type channel that buffers gzipped data and flushes to an
/// HTTP intake endpoint. Mirrors Go `eventchannel.EventChannel`.
struct PubstackEventChannel {
    tx: mpsc::UnboundedSender<PubstackEventMsg>,
}

impl PubstackEventChannel {
    fn new(
        client: reqwest::Client,
        intake_url: String,
        max_event_count: usize,
        max_byte_size: usize,
        flush_interval: std::time::Duration,
    ) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<PubstackEventMsg>();

        tokio::spawn(async move {
            use flate2::write::GzEncoder;
            use flate2::Compression;
            use std::io::Write;

            let mut gz = GzEncoder::new(Vec::new(), Compression::default());
            let mut event_count: usize = 0;
            let mut raw_size: usize = 0;
            let mut timer = tokio::time::interval(flush_interval);

            // Helper closure-like async fn — we inline it as a macro for clarity.
            macro_rules! do_flush {
                () => {
                    if event_count > 0 {
                        // Finish gzip stream, get compressed bytes.
                        let payload = match gz.finish() {
                            Ok(bytes) => bytes,
                            Err(e) => {
                                tracing::warn!(error = %e, "[pubstack] gzip finish failed");
                                Vec::new()
                            }
                        };
                        if !payload.is_empty() {
                            let send_url = intake_url.clone();
                            let send_client = client.clone();
                            tokio::spawn(async move {
                                let res = send_client
                                    .post(&send_url)
                                    .header("Content-Type", "application/octet-stream")
                                    .header("Content-Encoding", "gzip")
                                    .body(payload)
                                    .send()
                                    .await;
                                match res {
                                    Ok(resp) if resp.status().is_success() => {}
                                    Ok(resp) => {
                                        tracing::warn!(
                                            status = %resp.status(),
                                            "[pubstack] intake flush got non-200"
                                        );
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = %e, "[pubstack] intake flush failed");
                                    }
                                }
                            });
                        }
                        // Reset for next batch.
                        gz = GzEncoder::new(Vec::new(), Compression::default());
                        event_count = 0;
                        raw_size = 0;
                    }
                };
            }

            loop {
                tokio::select! {
                    msg = rx.recv() => {
                        match msg {
                            Some(PubstackEventMsg::Event(data)) => {
                                raw_size += data.len();
                                event_count += 1;
                                if let Err(e) = gz.write_all(&data) {
                                    tracing::warn!(error = %e, "[pubstack] gzip write failed, skipping event");
                                    continue;
                                }
                                if event_count >= max_event_count || raw_size >= max_byte_size {
                                    do_flush!();
                                }
                            }
                            Some(PubstackEventMsg::Shutdown(done)) => {
                                do_flush!();
                                let _ = done.send(());
                                let _ = (&gz, event_count, raw_size);
                                break;
                            }
                            None => {
                                do_flush!();
                                let _ = (&gz, event_count, raw_size);
                                break;
                            }
                        }
                    }
                    _ = timer.tick() => {
                        do_flush!();
                    }
                }
            }
        });

        Self { tx }
    }

    fn push(&self, data: Vec<u8>) {
        let _ = self.tx.send(PubstackEventMsg::Event(data));
    }

    async fn close(&self) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if self.tx.send(PubstackEventMsg::Shutdown(tx)).is_ok() {
            let _ = rx.await;
        }
    }
}

/// Pubstack analytics module.
///
/// Buffers events per event-type (auction, video, amp, cookie_sync, set_uid)
/// and sends them in gzip-compressed batches to the Pubstack intake endpoint.
/// Feature flags control which event types are collected.  Optionally polls the
/// remote `/bootstrap` endpoint for configuration updates.
///
/// Mirrors Go `analytics/pubstack/pubstack_module.go`.
pub struct PubstackAnalytics {
    config: tokio::sync::RwLock<PubstackConfig>,
    channels: tokio::sync::RwLock<HashMap<String, PubstackEventChannel>>,
}

impl PubstackAnalytics {
    /// Create a new Pubstack analytics module with the given configuration.
    ///
    /// Spawns per-channel background tasks for each enabled feature, and
    /// optionally a configuration-refresh task.
    pub fn new(config: PubstackConfig) -> Arc<Self> {
        let client = reqwest::Client::builder()
            .timeout(config.http_timeout)
            .build()
            .unwrap_or_default();

        let channels = Self::build_channels(&config, &client);

        let module = Arc::new(Self {
            config: tokio::sync::RwLock::new(config.clone()),
            channels: tokio::sync::RwLock::new(channels),
        });

        // Optionally spawn config refresh task.
        if let Some(interval) = config.config_refresh_interval {
            let weak = Arc::downgrade(&module);
            let refresh_client = client;
            let scope = config.scope_id.clone();
            let endpoint = config.endpoint.clone();

            tokio::spawn(async move {
                let mut timer = tokio::time::interval(interval);
                loop {
                    timer.tick().await;
                    let strong = match weak.upgrade() {
                        Some(s) => s,
                        None => break, // module dropped, stop.
                    };

                    let url = format!("{}/bootstrap?scopeId={}", endpoint, scope);
                    match refresh_client.get(&url).send().await {
                        Ok(resp) if resp.status().is_success() => {
                            if let Ok(remote) = resp.json::<PubstackRemoteConfig>().await {
                                let new_features = PubstackFeatures {
                                    auction: *remote.features.get("auction").unwrap_or(&false),
                                    video: *remote.features.get("video").unwrap_or(&false),
                                    amp: *remote.features.get("amp").unwrap_or(&false),
                                    cookie_sync: *remote.features.get("cookiesync").unwrap_or(&false),
                                    set_uid: *remote.features.get("setuid").unwrap_or(&false),
                                };
                                let mut cfg = strong.config.write().await;
                                cfg.features = new_features;
                                cfg.endpoint = remote.endpoint;
                                // Rebuild channels with new config.
                                let new_channels = Self::build_channels(&cfg, &refresh_client);
                                let mut ch_lock = strong.channels.write().await;
                                // Close old channels.
                                for (_, old_ch) in ch_lock.drain() {
                                    old_ch.close().await;
                                }
                                *ch_lock = new_channels;
                                tracing::info!("[pubstack] Configuration refreshed");
                            }
                        }
                        Ok(resp) => {
                            tracing::warn!(
                                status = %resp.status(),
                                "[pubstack] Config refresh got non-200"
                            );
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "[pubstack] Config refresh failed");
                        }
                    }
                }
            });
        }

        module
    }

    fn build_channels(
        config: &PubstackConfig,
        client: &reqwest::Client,
    ) -> HashMap<String, PubstackEventChannel> {
        let mut channels = HashMap::new();
        let features = [
            ("auction", config.features.auction),
            ("video", config.features.video),
            ("amp", config.features.amp),
            ("cookiesync", config.features.cookie_sync),
            ("setuid", config.features.set_uid),
        ];
        for (name, enabled) in features {
            if enabled {
                let intake_url = format!("{}/intake/{}", config.endpoint, name);
                channels.insert(
                    name.to_string(),
                    PubstackEventChannel::new(
                        client.clone(),
                        intake_url,
                        config.max_event_count,
                        config.max_byte_size,
                        config.flush_interval,
                    ),
                );
            }
        }
        channels
    }

    fn serialize_with_scope<T: Serialize>(scope: &str, payload: &T) -> Option<Vec<u8>> {
        let envelope = PubstackEnvelope { scope, payload };
        match serde_json::to_vec(&envelope) {
            Ok(mut data) => {
                data.push(b'\n');
                Some(data)
            }
            Err(e) => {
                tracing::error!(error = %e, "[pubstack] serialization failed");
                None
            }
        }
    }

    async fn push_if_enabled<T: Serialize>(&self, channel_name: &str, payload: &T) {
        let cfg = self.config.read().await;
        let scope = cfg.scope_id.clone();
        drop(cfg);

        if let Some(data) = Self::serialize_with_scope(&scope, payload) {
            let channels = self.channels.read().await;
            if let Some(ch) = channels.get(channel_name) {
                ch.push(data);
            }
            // If channel doesn't exist, the feature is disabled — silently drop.
        }
    }
}

#[async_trait]
impl AnalyticsModule for PubstackAnalytics {
    async fn log_auction_object(&self, ao: &AuctionObject) {
        self.push_if_enabled("auction", ao).await;
    }

    async fn log_video_object(&self, vo: &VideoObject) {
        self.push_if_enabled("video", vo).await;
    }

    async fn log_cookie_sync_object(&self, cso: &CookieSyncObject) {
        self.push_if_enabled("cookiesync", cso).await;
    }

    async fn log_setuid_object(&self, so: &SetUIDObject) {
        self.push_if_enabled("setuid", so).await;
    }

    async fn log_amp_object(&self, ao: &AmpObject) {
        self.push_if_enabled("amp", ao).await;
    }

    async fn log_notification_event(&self, _ne: &NotificationEvent) {
        // Pubstack does not process notification events (mirrors Go no-op).
    }

    async fn shutdown(&self) {
        tracing::info!("[pubstack] Shutting down, flushing all channels");
        let channels = self.channels.read().await;
        for (_, ch) in channels.iter() {
            ch.close().await;
        }
    }
}

// ---------------------------------------------------------------------------
// AgmaAnalytics — AGMA-compliant analytics (mirrors Go agma module)
// ---------------------------------------------------------------------------

/// Event type tag used in AGMA analytics payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgmaEventType {
    Auction,
    Amp,
    Video,
}

/// Configuration for one AGMA publisher / site account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgmaAccount {
    /// AGMA account code sent in each event.
    pub code: String,
    /// Publisher ID to match against the request.
    pub publisher_id: String,
    /// Optional site / app ID filter. If empty, all sites for the publisher match.
    #[serde(default)]
    pub site_app_id: String,
}

/// Configuration for the AGMA analytics module.
#[derive(Debug, Clone)]
pub struct AgmaConfig {
    /// HTTP endpoint to POST events to.
    pub endpoint: String,
    /// Accounts to track — at least one must be configured.
    pub accounts: Vec<AgmaAccount>,
    /// Maximum number of events to buffer before flush.
    pub max_event_count: usize,
    /// Maximum byte size of the JSON buffer before flush.
    pub max_buffer_size: usize,
    /// Maximum time between flushes.
    pub flush_interval: std::time::Duration,
    /// HTTP request timeout.
    pub http_timeout: std::time::Duration,
    /// Whether to gzip-compress the payload.
    pub gzip: bool,
}

impl Default for AgmaConfig {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            accounts: Vec::new(),
            max_event_count: 2000,
            max_buffer_size: 100 * 1024 * 1024,
            flush_interval: std::time::Duration::from_secs(60),
            http_timeout: std::time::Duration::from_secs(10),
            gzip: false,
        }
    }
}

/// Log object written for each AGMA event (mirrors Go `agma.logObject`).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgmaLogObject {
    #[serde(rename = "type")]
    event_type: AgmaEventType,
    id: String,
    code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    site: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    app: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<serde_json::Value>,
    created_at: DateTime<Utc>,
}

/// Message sent to the AGMA background worker.
enum AgmaMsg {
    Event(Vec<u8>),
    Shutdown(tokio::sync::oneshot::Sender<()>),
}

/// AGMA analytics module.
///
/// Buffers JSON-serialized events and periodically sends them in a JSON array
/// to the configured AGMA endpoint.  Only auction, video, and AMP events with
/// HTTP 200 status are tracked.  Publisher/site matching and (in the Go
/// implementation) GDPR consent checks are performed before buffering.
///
/// The Rust port performs publisher/site matching but does not parse TCF
/// consent strings (that logic can be layered on later once a TCF library
/// is available in the Rust workspace).
///
/// Mirrors Go `analytics/agma/agma_module.go`.
pub struct AgmaAnalytics {
    tx: mpsc::UnboundedSender<AgmaMsg>,
    config: AgmaConfig,
}

impl AgmaAnalytics {
    /// Create a new AGMA analytics module. Returns an error if no accounts
    /// are configured.
    pub fn new(config: AgmaConfig) -> Result<Self, String> {
        if config.accounts.is_empty() {
            return Err("AgmaAnalytics requires at least one account".to_string());
        }

        let client = reqwest::Client::builder()
            .timeout(config.http_timeout)
            .build()
            .unwrap_or_default();

        let endpoint = config.endpoint.clone();
        let gzip = config.gzip;
        let max_event_count = config.max_event_count;
        let max_buffer_size = config.max_buffer_size;
        let flush_interval = config.flush_interval;

        let (tx, mut rx) = mpsc::unbounded_channel::<AgmaMsg>();

        tokio::spawn(async move {
            // Buffer: we build a JSON array manually like the Go code does.
            let mut buf: Vec<u8> = Vec::with_capacity(4096);
            buf.push(b'[');
            let mut event_count: usize = 0;
            let mut timer = tokio::time::interval(flush_interval);

            macro_rules! do_flush {
                () => {
                    if event_count > 0 && buf.len() > 1 {
                        // Remove trailing comma and close the JSON array.
                        if buf.last() == Some(&b',') {
                            buf.pop();
                        }
                        buf.push(b']');

                        let payload = std::mem::replace(&mut buf, Vec::with_capacity(4096));
                        buf.push(b'[');
                        event_count = 0;

                        let send_client = client.clone();
                        let send_endpoint = endpoint.clone();
                        tokio::spawn(async move {
                            let body: Vec<u8> = if gzip {
                                match Self::compress_gzip(&payload) {
                                    Ok(compressed) => compressed,
                                    Err(e) => {
                                        tracing::error!(error = %e, "[agma] gzip compression failed");
                                        return;
                                    }
                                }
                            } else {
                                payload
                            };

                            let mut req = send_client
                                .post(&send_endpoint)
                                .header("Content-Type", "application/json");

                            if gzip {
                                req = req.header("Content-Encoding", "gzip");
                            }

                            match req.body(body).send().await {
                                Ok(resp) if resp.status().is_success() => {}
                                Ok(resp) => {
                                    tracing::warn!(
                                        status = %resp.status(),
                                        "[agma] flush got non-200"
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "[agma] flush failed");
                                }
                            }
                        });
                    }
                };
            }

            loop {
                tokio::select! {
                    msg = rx.recv() => {
                        match msg {
                            Some(AgmaMsg::Event(data)) => {
                                buf.extend_from_slice(&data);
                                buf.push(b',');
                                event_count += 1;
                                if event_count >= max_event_count || buf.len() >= max_buffer_size {
                                    do_flush!();
                                }
                            }
                            Some(AgmaMsg::Shutdown(done)) => {
                                do_flush!();
                                let _ = done.send(());
                                let _ = event_count;
                                break;
                            }
                            None => {
                                do_flush!();
                                let _ = event_count;
                                break;
                            }
                        }
                    }
                    _ = timer.tick() => {
                        do_flush!();
                    }
                }
            }
        });

        Ok(Self { tx, config })
    }

    fn compress_gzip(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data)?;
        encoder.finish()
    }

    /// Extract `(publisher_id, site_or_app_id)` from the request JSON.
    fn extract_publisher_and_site(request: &serde_json::Value) -> (String, String) {
        let mut publisher_id = String::new();
        let mut app_site_id = String::new();

        if let Some(site) = request.get("site") {
            if let Some(pub_obj) = site.get("publisher") {
                if let Some(id) = pub_obj.get("id").and_then(|v| v.as_str()) {
                    publisher_id = id.to_string();
                }
            }
            if let Some(id) = site.get("id").and_then(|v| v.as_str()) {
                app_site_id = id.to_string();
            }
        }
        if let Some(app) = request.get("app") {
            if let Some(pub_obj) = app.get("publisher") {
                if let Some(id) = pub_obj.get("id").and_then(|v| v.as_str()) {
                    publisher_id = id.to_string();
                }
            }
            if let Some(id) = app.get("id").and_then(|v| v.as_str()) {
                app_site_id = id.to_string();
            }
            if app_site_id.is_empty() {
                if let Some(bundle) = app.get("bundle").and_then(|v| v.as_str()) {
                    app_site_id = bundle.to_string();
                }
            }
        }

        (publisher_id, app_site_id)
    }

    /// Check whether the request matches a configured account. Returns the
    /// account code if so.
    fn match_account(&self, request: &serde_json::Value) -> Option<String> {
        let (publisher_id, app_site_id) = Self::extract_publisher_and_site(request);
        if publisher_id.is_empty() && app_site_id.is_empty() {
            return None;
        }

        for account in &self.config.accounts {
            if account.publisher_id == publisher_id {
                if account.site_app_id.is_empty() {
                    return Some(account.code.clone());
                }
                if account.site_app_id == app_site_id {
                    return Some(account.code.clone());
                }
            }
        }
        None
    }

    /// Build an AGMA log object from request JSON and enqueue it.
    fn try_enqueue(
        &self,
        event_type: AgmaEventType,
        status: i32,
        request: &Option<serde_json::Value>,
        start_time: DateTime<Utc>,
    ) {
        if status != 200 {
            return;
        }
        let request = match request {
            Some(r) => r,
            None => return,
        };

        let code = match self.match_account(request) {
            Some(c) => c,
            None => return,
        };

        let request_id = request
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let log_obj = AgmaLogObject {
            event_type,
            id: request_id,
            code,
            site: request.get("site").cloned(),
            app: request.get("app").cloned(),
            device: request.get("device").cloned(),
            user: request.get("user").cloned(),
            created_at: start_time,
        };

        match serde_json::to_vec(&log_obj) {
            Ok(data) => {
                let _ = self.tx.send(AgmaMsg::Event(data));
            }
            Err(e) => {
                tracing::error!(error = %e, "[agma] serialization failed");
            }
        }
    }
}

#[async_trait]
impl AnalyticsModule for AgmaAnalytics {
    async fn log_auction_object(&self, ao: &AuctionObject) {
        self.try_enqueue(AgmaEventType::Auction, ao.status, &ao.request, ao.start_time);
    }

    async fn log_video_object(&self, vo: &VideoObject) {
        self.try_enqueue(AgmaEventType::Video, vo.status, &vo.request, vo.start_time);
    }

    async fn log_cookie_sync_object(&self, _cso: &CookieSyncObject) {
        // AGMA does not track cookie sync events (mirrors Go no-op).
    }

    async fn log_setuid_object(&self, _so: &SetUIDObject) {
        // AGMA does not track set-uid events (mirrors Go no-op).
    }

    async fn log_amp_object(&self, ao: &AmpObject) {
        self.try_enqueue(AgmaEventType::Amp, ao.status, &ao.request, ao.start_time);
    }

    async fn log_notification_event(&self, _ne: &NotificationEvent) {
        // AGMA does not track notification events (mirrors Go no-op).
    }

    async fn shutdown(&self) {
        tracing::info!("[agma] Shutting down, flushing buffer");
        let (tx, rx) = tokio::sync::oneshot::channel();
        if self.tx.send(AgmaMsg::Shutdown(tx)).is_ok() {
            let _ = rx.await;
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::AsyncReadExt;

    /// A spy module that counts calls for testing.
    struct SpyModule {
        auction_count: AtomicUsize,
        video_count: AtomicUsize,
        cookie_sync_count: AtomicUsize,
        setuid_count: AtomicUsize,
        amp_count: AtomicUsize,
        notification_count: AtomicUsize,
        shutdown_count: AtomicUsize,
    }

    impl SpyModule {
        fn new() -> Self {
            Self {
                auction_count: AtomicUsize::new(0),
                video_count: AtomicUsize::new(0),
                cookie_sync_count: AtomicUsize::new(0),
                setuid_count: AtomicUsize::new(0),
                amp_count: AtomicUsize::new(0),
                notification_count: AtomicUsize::new(0),
                shutdown_count: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl AnalyticsModule for SpyModule {
        async fn log_auction_object(&self, _ao: &AuctionObject) {
            self.auction_count.fetch_add(1, Ordering::SeqCst);
        }
        async fn log_video_object(&self, _vo: &VideoObject) {
            self.video_count.fetch_add(1, Ordering::SeqCst);
        }
        async fn log_cookie_sync_object(&self, _cso: &CookieSyncObject) {
            self.cookie_sync_count.fetch_add(1, Ordering::SeqCst);
        }
        async fn log_setuid_object(&self, _so: &SetUIDObject) {
            self.setuid_count.fetch_add(1, Ordering::SeqCst);
        }
        async fn log_amp_object(&self, _ao: &AmpObject) {
            self.amp_count.fetch_add(1, Ordering::SeqCst);
        }
        async fn log_notification_event(&self, _ne: &NotificationEvent) {
            self.notification_count.fetch_add(1, Ordering::SeqCst);
        }
        async fn shutdown(&self) {
            self.shutdown_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn sample_auction_object() -> AuctionObject {
        AuctionObject {
            status: 200,
            errors: vec![],
            request: None,
            response: None,
            account: Some("test-account".to_string()),
            start_time: Utc::now(),
            hook_execution_outcome: vec![],
            seat_non_bid: vec![],
        }
    }

    #[tokio::test]
    async fn noop_does_not_panic() {
        let noop = NoopAnalyticsModule;
        noop.log_auction_object(&sample_auction_object()).await;
        noop.log_video_object(&VideoObject {
            status: 200,
            errors: vec![],
            request: None,
            response: None,
            video_request: None,
            video_response: None,
            start_time: Utc::now(),
            seat_non_bid: vec![],
        })
        .await;
        noop.shutdown().await;
    }

    #[tokio::test]
    async fn aggregator_dispatches_to_all_modules() {
        let spy1 = Arc::new(SpyModule::new());
        let spy2 = Arc::new(SpyModule::new());

        let aggregator = LogAggregator::new(vec![spy1.clone(), spy2.clone()]);
        assert_eq!(aggregator.len(), 2);
        assert!(!aggregator.is_empty());

        let ao = sample_auction_object();
        aggregator.log_auction_object(&ao).await;
        aggregator.log_auction_object(&ao).await;

        assert_eq!(spy1.auction_count.load(Ordering::SeqCst), 2);
        assert_eq!(spy2.auction_count.load(Ordering::SeqCst), 2);

        aggregator.shutdown().await;
        assert_eq!(spy1.shutdown_count.load(Ordering::SeqCst), 1);
        assert_eq!(spy2.shutdown_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn file_logger_writes_json_lines() {
        let dir = std::env::temp_dir().join("pbs_analytics_test");
        let _ = tokio::fs::create_dir_all(&dir).await;
        let path = dir.join("test_log.jsonl");
        let path_str = path.to_string_lossy().to_string();

        // Remove previous test output if present.
        let _ = tokio::fs::remove_file(&path).await;

        let logger = FileLogger::new(&path_str);

        let ao = sample_auction_object();
        logger.log_auction_object(&ao).await;

        let so = SetUIDObject {
            status: 200,
            bidder: "appnexus".to_string(),
            uid: "abc123".to_string(),
            errors: vec![],
            success: true,
        };
        logger.log_setuid_object(&so).await;

        // Shutdown flushes the writer.
        logger.shutdown().await;

        let mut contents = String::new();
        tokio::fs::File::open(&path)
            .await
            .expect("log file should exist")
            .read_to_string(&mut contents)
            .await
            .expect("should read log file");

        let lines: Vec<&str> = contents.trim().lines().collect();
        assert_eq!(lines.len(), 2, "expected 2 log lines, got: {}", contents);

        // First line should be an auction object.
        let v: serde_json::Value = serde_json::from_str(lines[0]).expect("valid JSON");
        assert_eq!(v["type"], "/openrtb2/auction");
        assert_eq!(v["status"], 200);

        // Second line should be a set-uid object.
        let v: serde_json::Value = serde_json::from_str(lines[1]).expect("valid JSON");
        assert_eq!(v["type"], "/set_uid");
        assert_eq!(v["bidder"], "appnexus");
        assert_eq!(v["success"], true);

        // Cleanup.
        let _ = tokio::fs::remove_file(&path).await;
    }

    #[test]
    fn request_type_serializes_correctly() {
        assert_eq!(
            serde_json::to_string(&RequestType::Auction).unwrap(),
            r#""/openrtb2/auction""#
        );
        assert_eq!(
            serde_json::to_string(&RequestType::CookieSync).unwrap(),
            r#""/cookie_sync""#
        );
    }

    #[test]
    fn cookie_sync_bidder_serialization() {
        let b = CookieSyncBidder {
            bidder: "rubicon".to_string(),
            no_cookie: true,
            usersync: Some(UsersyncInfo {
                url: Some("https://example.com/sync".to_string()),
                sync_type: Some("redirect".to_string()),
            }),
        };
        let json = serde_json::to_string(&b).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["bidder"], "rubicon");
        assert_eq!(v["no_cookie"], true);
        assert_eq!(v["usersync"]["type"], "redirect");
    }
}
