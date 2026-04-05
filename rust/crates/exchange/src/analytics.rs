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
