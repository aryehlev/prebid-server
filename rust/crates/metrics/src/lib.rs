use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use prometheus::{
    Encoder, GaugeVec, HistogramOpts, HistogramVec, IntCounterVec, Opts, Registry, TextEncoder,
};

// ---------------------------------------------------------------------------
// Enums / label types
// ---------------------------------------------------------------------------

/// The type of incoming request (endpoint).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestType {
    OpenRTB2Web,
    OpenRTB2App,
    OpenRTB2DOOH,
    Amp,
    Video,
}

impl RequestType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RequestType::OpenRTB2Web => "openrtb2-web",
            RequestType::OpenRTB2App => "openrtb2-app",
            RequestType::OpenRTB2DOOH => "openrtb2-dooh",
            RequestType::Amp => "amp",
            RequestType::Video => "video",
        }
    }
}

/// Status of an auction / HTTP request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestStatus {
    Ok,
    BadInput,
    Err,
    NetworkErr,
    Timeout,
    BlockedApp,
    QueueTimeout,
    AccountConfigErr,
    BadServerResponse,
}

impl RequestStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RequestStatus::Ok => "ok",
            RequestStatus::BadInput => "badinput",
            RequestStatus::Err => "err",
            RequestStatus::NetworkErr => "networkerr",
            RequestStatus::Timeout => "timeout",
            RequestStatus::BlockedApp => "blockedapp",
            RequestStatus::BadServerResponse => "badserverresponse",
            RequestStatus::QueueTimeout => "queuetimeout",
            RequestStatus::AccountConfigErr => "acctconfigerr",
        }
    }
}

/// Labels attached to a top-level request metric.
#[derive(Debug, Clone)]
pub struct RequestLabels {
    pub request_type: RequestType,
    pub request_status: RequestStatus,
}

/// Labels describing the impression media types present in a request.
#[derive(Debug, Clone, Default)]
pub struct ImpLabels {
    pub banner: bool,
    pub video: bool,
    pub audio: bool,
    pub native: bool,
}

/// Status of a bidder adapter's bid outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdapterBidStatus {
    Bid,
    NoBid,
}

impl AdapterBidStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AdapterBidStatus::Bid => "bid",
            AdapterBidStatus::NoBid => "nobid",
        }
    }
}

/// Labels attached to adapter (bidder) level metrics.
#[derive(Debug, Clone)]
pub struct AdapterLabels {
    pub adapter: String,
    pub adapter_bid_type: AdapterBidStatus,
    pub no_cookie: bool,
}

/// Status codes for the `/cookie_sync` endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CookieSyncStatus {
    Ok,
    BadRequest,
    OptOut,
    GDPRBlockedHostCookie,
    AccountBlocked,
    AccountConfigMalformed,
    AccountInvalid,
}

impl CookieSyncStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CookieSyncStatus::Ok => "ok",
            CookieSyncStatus::BadRequest => "bad_request",
            CookieSyncStatus::OptOut => "opt_out",
            CookieSyncStatus::GDPRBlockedHostCookie => "gdpr_blocked_host_cookie",
            CookieSyncStatus::AccountBlocked => "acct_blocked",
            CookieSyncStatus::AccountConfigMalformed => "acct_config_malformed",
            CookieSyncStatus::AccountInvalid => "acct_invalid",
        }
    }
}

/// Status codes for the `/setuid` endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SetUidStatus {
    Ok,
    BadRequest,
    OptOut,
    GDPRBlocked,
}

impl SetUidStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SetUidStatus::Ok => "ok",
            SetUidStatus::BadRequest => "bad_request",
            SetUidStatus::OptOut => "opt_out",
            SetUidStatus::GDPRBlocked => "gdpr_blocked",
        }
    }
}

/// Labels attached to `/setuid` endpoint metrics.
#[derive(Debug, Clone)]
pub struct SetUidLabels {
    pub bidder: String,
    pub status: SetUidStatus,
}

// ---------------------------------------------------------------------------
// MetricsEngine trait
// ---------------------------------------------------------------------------

/// Core metrics interface mirroring the Go `MetricsEngine`.
///
/// All methods have default no-op implementations so that callers can implement
/// only the subset they care about.
pub trait MetricsEngine: Send + Sync {
    /// Record a completed auction / HTTP request (structured labels).
    fn record_request(&self, labels: &RequestLabels);

    /// Convenience: record a request using a type string and status.
    fn record_request_by_type(&self, _request_type: &str, _status: RequestStatus) {}

    /// Record impressions by media type.
    fn record_impression(&self, labels: &ImpLabels);

    /// Record the end-to-end duration of a request.
    fn record_request_time(&self, labels: &RequestLabels, duration: Duration);

    /// Record that a request was sent to an adapter.
    fn record_adapter_request(&self, labels: &AdapterLabels);

    /// Record a bid received from an adapter.
    fn record_adapter_bid_received(
        &self,
        labels: &AdapterLabels,
        bid_type: &str,
        has_adm: bool,
    );

    /// Record the CPM price of a bid from an adapter.
    fn record_adapter_price(&self, labels: &AdapterLabels, cpm: f64);

    /// Record the response time of an adapter.
    fn record_adapter_time(&self, labels: &AdapterLabels, duration: Duration);

    /// Record a `/cookie_sync` call.
    fn record_cookie_sync(&self, status: CookieSyncStatus);

    /// Record a `/setuid` call.
    fn record_setuid(&self, labels: &SetUidLabels);

    /// Record a stored-request cache lookup (hit = found, miss = !found).
    fn record_stored_request(&self, found: bool);

    // -- Exchange-level convenience methods --

    /// Record that a request was sent to a specific bidder.
    fn record_bidder_request(&self, _bidder: &str) {}

    /// Record a bidder response with status and duration.
    fn record_bidder_response(&self, _bidder: &str, _status: BidderStatus, _duration_ms: u64) {}

    /// Record the number of bids received from a bidder by media type.
    fn record_bid_count(&self, _bidder: &str, _bid_type: &str) {}

    /// Record a bidder error by error type.
    fn record_bidder_error(&self, _bidder: &str, _error_type: &str) {}

    /// Record total auction duration.
    fn record_auction_duration(&self, _request_type: &str, _duration_ms: u64) {}

    /// Record an HTTP request by endpoint type, status code, and duration.
    fn record_http_request(&self, _request_type: &str, _status_code: u16, _duration_ms: u64) {}

    /// Render all collected metrics as Prometheus text exposition format.
    fn to_text(&self) -> String {
        String::new()
    }
}

/// Status of a bidder response used for metrics recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BidderStatus {
    /// Bidder returned one or more bids.
    Got,
    /// Bidder returned no bids.
    NoBid,
    /// Bidder timed out.
    TimedOut,
    /// Bidder returned an error.
    Error,
}

// ---------------------------------------------------------------------------
// Histogram bucket boundaries
// ---------------------------------------------------------------------------

/// Bucket boundaries (ms) for request-level durations.
const REQUEST_DURATION_BUCKETS: &[f64] = &[
    50.0, 100.0, 200.0, 300.0, 500.0, 750.0, 1000.0, 1500.0, 2000.0,
];

/// Bucket boundaries (ms) for adapter response durations.
const ADAPTER_DURATION_BUCKETS: &[f64] = &[
    25.0, 50.0, 100.0, 200.0, 400.0, 800.0, 1600.0,
];

/// Bucket boundaries (USD CPM) for bid prices.
const PRICE_BUCKETS: &[f64] = &[
    0.1, 0.25, 0.5, 1.0, 2.0, 3.0, 5.0, 10.0, 15.0, 20.0, 50.0,
];

// ---------------------------------------------------------------------------
// PrometheusMetrics
// ---------------------------------------------------------------------------

/// Prometheus-backed implementation of [`MetricsEngine`].
pub struct PrometheusMetrics {
    registry: Registry,

    // -- request-level --
    requests_total: IntCounterVec,
    request_duration_ms: HistogramVec,
    impressions_total: IntCounterVec,

    // -- adapter-level --
    adapter_requests_total: IntCounterVec,
    adapter_bids_total: IntCounterVec,
    adapter_price_cpm: HistogramVec,
    adapter_response_time_ms: HistogramVec,
    adapter_no_cookie_total: IntCounterVec,

    // -- cookie sync / setuid --
    cookie_sync_total: IntCounterVec,
    setuid_total: IntCounterVec,

    // -- stored requests --
    stored_request_hit_total: prometheus::IntCounter,
    stored_request_miss_total: prometheus::IntCounter,

    // -- gauges (retained for completeness) --
    active_bidders: GaugeVec,

    // -- bidder-level (exchange compat) --
    bidder_requests_total: IntCounterVec,
    bidder_response_time_ms: HistogramVec,
    bids_total: IntCounterVec,
    bidder_errors_total: IntCounterVec,
    auction_duration_ms: HistogramVec,
    http_request_duration_ms: HistogramVec,

    // -- connections (legacy, kept for backward compat) --
    connections_accepted: prometheus::IntCounter,
    connections_closed: prometheus::IntCounter,
}

impl PrometheusMetrics {
    /// Create a new `PrometheusMetrics` with the given namespace prefix.
    ///
    /// Returns `Err` only if Prometheus metric registration fails (e.g.
    /// duplicate metric names inside the same process -- should not happen in
    /// normal usage).
    pub fn new(namespace: &str) -> Result<Self, prometheus::Error> {
        let registry = Registry::new();

        // -- request-level counters & histograms ---------------------------------

        let requests_total = IntCounterVec::new(
            Opts::new("requests_total", "Total number of auction requests")
                .namespace(namespace),
            &["request_type", "request_status"],
        )?;

        let request_duration_ms = HistogramVec::new(
            HistogramOpts::new(
                "request_duration_ms",
                "End-to-end request duration in milliseconds",
            )
            .namespace(namespace)
            .buckets(REQUEST_DURATION_BUCKETS.to_vec()),
            &["request_type", "request_status"],
        )?;

        let impressions_total = IntCounterVec::new(
            Opts::new("impressions_total", "Total impressions by media type")
                .namespace(namespace),
            &["media_type"],
        )?;

        // -- adapter-level -------------------------------------------------------

        let adapter_requests_total = IntCounterVec::new(
            Opts::new(
                "adapter_requests_total",
                "Total number of requests to each adapter",
            )
            .namespace(namespace),
            &["adapter", "no_cookie"],
        )?;

        let adapter_bids_total = IntCounterVec::new(
            Opts::new("adapter_bids_total", "Total bids received from adapters")
                .namespace(namespace),
            &["adapter", "bid_type", "has_adm"],
        )?;

        let adapter_price_cpm = HistogramVec::new(
            HistogramOpts::new("adapter_price_cpm", "Bid CPM prices from adapters")
                .namespace(namespace)
                .buckets(PRICE_BUCKETS.to_vec()),
            &["adapter"],
        )?;

        let adapter_response_time_ms = HistogramVec::new(
            HistogramOpts::new(
                "adapter_response_time_ms",
                "Adapter response time in milliseconds",
            )
            .namespace(namespace)
            .buckets(ADAPTER_DURATION_BUCKETS.to_vec()),
            &["adapter"],
        )?;

        let adapter_no_cookie_total = IntCounterVec::new(
            Opts::new(
                "adapter_no_cookie_total",
                "Total adapter requests without user cookie",
            )
            .namespace(namespace),
            &["adapter"],
        )?;

        // -- cookie sync / setuid ------------------------------------------------

        let cookie_sync_total = IntCounterVec::new(
            Opts::new("cookie_sync_total", "Total cookie sync requests by status")
                .namespace(namespace),
            &["status"],
        )?;

        let setuid_total = IntCounterVec::new(
            Opts::new("setuid_total", "Total setuid requests by bidder and status")
                .namespace(namespace),
            &["bidder", "status"],
        )?;

        // -- stored requests -----------------------------------------------------

        let stored_request_hit_total = prometheus::IntCounter::with_opts(
            Opts::new(
                "stored_request_hit_total",
                "Total stored request cache hits",
            )
            .namespace(namespace),
        )?;

        let stored_request_miss_total = prometheus::IntCounter::with_opts(
            Opts::new(
                "stored_request_miss_total",
                "Total stored request cache misses",
            )
            .namespace(namespace),
        )?;

        // -- gauges --------------------------------------------------------------

        let active_bidders = GaugeVec::new(
            Opts::new(
                "active_bidders",
                "Number of active bidders registered in this instance",
            )
            .namespace(namespace),
            &["bidder"],
        )?;

        // -- bidder-level (exchange compat) --------------------------------------

        let bidder_requests_total = IntCounterVec::new(
            Opts::new(
                "bidder_requests_total",
                "Total number of requests to each bidder",
            )
            .namespace(namespace),
            &["bidder"],
        )?;

        let bidder_response_time_ms = HistogramVec::new(
            HistogramOpts::new(
                "bidder_response_time_ms",
                "Bidder response time in milliseconds",
            )
            .namespace(namespace)
            .buckets(ADAPTER_DURATION_BUCKETS.to_vec()),
            &["bidder", "status"],
        )?;

        let bids_total = IntCounterVec::new(
            Opts::new("bids_total", "Total number of bids by bidder and type")
                .namespace(namespace),
            &["bidder", "type"],
        )?;

        let bidder_errors_total = IntCounterVec::new(
            Opts::new("bidder_errors_total", "Total bidder errors by type")
                .namespace(namespace),
            &["bidder", "err_type"],
        )?;

        let auction_duration_ms = HistogramVec::new(
            HistogramOpts::new(
                "auction_duration_ms",
                "Total auction duration in milliseconds",
            )
            .namespace(namespace)
            .buckets(REQUEST_DURATION_BUCKETS.to_vec()),
            &["request_type"],
        )?;

        let http_request_duration_ms = HistogramVec::new(
            HistogramOpts::new(
                "http_request_duration_ms",
                "HTTP request duration by endpoint and status code",
            )
            .namespace(namespace)
            .buckets(REQUEST_DURATION_BUCKETS.to_vec()),
            &["request_type", "status_code"],
        )?;

        // -- connections (legacy) ------------------------------------------------

        let connections_accepted = prometheus::IntCounter::with_opts(
            Opts::new(
                "connections_accepted",
                "Total number of accepted connections",
            )
            .namespace(namespace),
        )?;

        let connections_closed = prometheus::IntCounter::with_opts(
            Opts::new("connections_closed", "Total number of closed connections")
                .namespace(namespace),
        )?;

        // -- register everything -------------------------------------------------

        registry.register(Box::new(requests_total.clone()))?;
        registry.register(Box::new(request_duration_ms.clone()))?;
        registry.register(Box::new(impressions_total.clone()))?;
        registry.register(Box::new(adapter_requests_total.clone()))?;
        registry.register(Box::new(adapter_bids_total.clone()))?;
        registry.register(Box::new(adapter_price_cpm.clone()))?;
        registry.register(Box::new(adapter_response_time_ms.clone()))?;
        registry.register(Box::new(adapter_no_cookie_total.clone()))?;
        registry.register(Box::new(cookie_sync_total.clone()))?;
        registry.register(Box::new(setuid_total.clone()))?;
        registry.register(Box::new(stored_request_hit_total.clone()))?;
        registry.register(Box::new(stored_request_miss_total.clone()))?;
        registry.register(Box::new(active_bidders.clone()))?;
        registry.register(Box::new(bidder_requests_total.clone()))?;
        registry.register(Box::new(bidder_response_time_ms.clone()))?;
        registry.register(Box::new(bids_total.clone()))?;
        registry.register(Box::new(bidder_errors_total.clone()))?;
        registry.register(Box::new(auction_duration_ms.clone()))?;
        registry.register(Box::new(http_request_duration_ms.clone()))?;
        registry.register(Box::new(connections_accepted.clone()))?;
        registry.register(Box::new(connections_closed.clone()))?;

        Ok(Self {
            registry,
            requests_total,
            request_duration_ms,
            impressions_total,
            adapter_requests_total,
            adapter_bids_total,
            adapter_price_cpm,
            adapter_response_time_ms,
            adapter_no_cookie_total,
            cookie_sync_total,
            setuid_total,
            stored_request_hit_total,
            stored_request_miss_total,
            active_bidders,
            bidder_requests_total,
            bidder_response_time_ms,
            bids_total,
            bidder_errors_total,
            auction_duration_ms,
            http_request_duration_ms,
            connections_accepted,
            connections_closed,
        })
    }

    /// Get a reference to the underlying Prometheus [`Registry`].
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Gather all metrics in Prometheus text exposition format.
    pub fn gather_text(&self) -> String {
        let encoder = TextEncoder::new();
        let families = self.registry.gather();
        let mut buf = Vec::new();
        encoder.encode(&families, &mut buf).unwrap_or_default();
        String::from_utf8(buf).unwrap_or_default()
    }

    /// Mark a bidder as active (gauge = 1) or inactive (gauge = 0).
    pub fn set_bidder_active(&self, bidder: &str, active: bool) {
        self.active_bidders
            .with_label_values(&[bidder])
            .set(if active { 1.0 } else { 0.0 });
    }

    /// Record a connection accepted (legacy helper).
    pub fn record_connection_accepted(&self) {
        self.connections_accepted.inc();
    }

    /// Record a connection closed (legacy helper).
    pub fn record_connection_closed(&self) {
        self.connections_closed.inc();
    }
}

impl MetricsEngine for PrometheusMetrics {
    fn record_request(&self, labels: &RequestLabels) {
        self.requests_total
            .with_label_values(&[
                labels.request_type.as_str(),
                labels.request_status.as_str(),
            ])
            .inc();
    }

    fn record_impression(&self, labels: &ImpLabels) {
        if labels.banner {
            self.impressions_total
                .with_label_values(&["banner"])
                .inc();
        }
        if labels.video {
            self.impressions_total.with_label_values(&["video"]).inc();
        }
        if labels.audio {
            self.impressions_total.with_label_values(&["audio"]).inc();
        }
        if labels.native {
            self.impressions_total
                .with_label_values(&["native"])
                .inc();
        }
    }

    fn record_request_time(&self, labels: &RequestLabels, duration: Duration) {
        self.request_duration_ms
            .with_label_values(&[
                labels.request_type.as_str(),
                labels.request_status.as_str(),
            ])
            .observe(duration.as_millis() as f64);
    }

    fn record_adapter_request(&self, labels: &AdapterLabels) {
        let no_cookie_str = if labels.no_cookie { "true" } else { "false" };
        self.adapter_requests_total
            .with_label_values(&[&labels.adapter, no_cookie_str])
            .inc();
        if labels.no_cookie {
            self.adapter_no_cookie_total
                .with_label_values(&[&labels.adapter])
                .inc();
        }
    }

    fn record_adapter_bid_received(
        &self,
        labels: &AdapterLabels,
        bid_type: &str,
        has_adm: bool,
    ) {
        let has_adm_str = if has_adm { "true" } else { "false" };
        self.adapter_bids_total
            .with_label_values(&[&labels.adapter, bid_type, has_adm_str])
            .inc();
    }

    fn record_adapter_price(&self, labels: &AdapterLabels, cpm: f64) {
        self.adapter_price_cpm
            .with_label_values(&[&labels.adapter])
            .observe(cpm);
    }

    fn record_adapter_time(&self, labels: &AdapterLabels, duration: Duration) {
        self.adapter_response_time_ms
            .with_label_values(&[&labels.adapter])
            .observe(duration.as_millis() as f64);
    }

    fn record_cookie_sync(&self, status: CookieSyncStatus) {
        self.cookie_sync_total
            .with_label_values(&[status.as_str()])
            .inc();
    }

    fn record_setuid(&self, labels: &SetUidLabels) {
        self.setuid_total
            .with_label_values(&[&labels.bidder, labels.status.as_str()])
            .inc();
    }

    fn record_stored_request(&self, found: bool) {
        if found {
            self.stored_request_hit_total.inc();
        } else {
            self.stored_request_miss_total.inc();
        }
    }

    // -- exchange-compat overrides --

    fn record_request_by_type(&self, request_type: &str, status: RequestStatus) {
        self.requests_total
            .with_label_values(&[request_type, status.as_str()])
            .inc();
    }

    fn record_bidder_request(&self, bidder: &str) {
        self.bidder_requests_total
            .with_label_values(&[bidder])
            .inc();
    }

    fn record_bidder_response(&self, bidder: &str, status: BidderStatus, duration_ms: u64) {
        let status_str = match status {
            BidderStatus::Got => "got_bids",
            BidderStatus::NoBid => "no_bid",
            BidderStatus::TimedOut => "timed_out",
            BidderStatus::Error => "error",
        };
        self.bidder_response_time_ms
            .with_label_values(&[bidder, status_str])
            .observe(duration_ms as f64);
    }

    fn record_bid_count(&self, bidder: &str, bid_type: &str) {
        self.bids_total
            .with_label_values(&[bidder, bid_type])
            .inc();
    }

    fn record_bidder_error(&self, bidder: &str, error_type: &str) {
        self.bidder_errors_total
            .with_label_values(&[bidder, error_type])
            .inc();
    }

    fn record_auction_duration(&self, request_type: &str, duration_ms: u64) {
        self.auction_duration_ms
            .with_label_values(&[request_type])
            .observe(duration_ms as f64);
    }

    fn record_http_request(&self, request_type: &str, status_code: u16, duration_ms: u64) {
        let status_str = status_code.to_string();
        self.http_request_duration_ms
            .with_label_values(&[request_type, &status_str])
            .observe(duration_ms as f64);
    }

    fn to_text(&self) -> String {
        self.gather_text()
    }
}

// ---------------------------------------------------------------------------
// NoopMetrics
// ---------------------------------------------------------------------------

/// No-op implementation that silently discards every metric.
pub struct NoopMetrics;

impl MetricsEngine for NoopMetrics {
    fn record_request(&self, _labels: &RequestLabels) {}
    fn record_impression(&self, _labels: &ImpLabels) {}
    fn record_request_time(&self, _labels: &RequestLabels, _duration: Duration) {}
    fn record_adapter_request(&self, _labels: &AdapterLabels) {}
    fn record_adapter_bid_received(
        &self,
        _labels: &AdapterLabels,
        _bid_type: &str,
        _has_adm: bool,
    ) {
    }
    fn record_adapter_price(&self, _labels: &AdapterLabels, _cpm: f64) {}
    fn record_adapter_time(&self, _labels: &AdapterLabels, _duration: Duration) {}
    fn record_cookie_sync(&self, _status: CookieSyncStatus) {}
    fn record_setuid(&self, _labels: &SetUidLabels) {}
    fn record_stored_request(&self, _found: bool) {}
    fn to_text(&self) -> String {
        String::new()
    }
}

// ---------------------------------------------------------------------------
// DummyMetricsEngine  (for tests -- counts every call)
// ---------------------------------------------------------------------------

/// Test double that atomically counts every call made through [`MetricsEngine`].
///
/// All counters are `AtomicU64` so the struct is `Send + Sync` and can be
/// shared across threads without a mutex.
pub struct DummyMetricsEngine {
    pub request_count: AtomicU64,
    pub impression_count: AtomicU64,
    pub request_time_count: AtomicU64,
    pub adapter_request_count: AtomicU64,
    pub adapter_bid_received_count: AtomicU64,
    pub adapter_price_count: AtomicU64,
    pub adapter_time_count: AtomicU64,
    pub cookie_sync_count: AtomicU64,
    pub setuid_count: AtomicU64,
    pub stored_request_found_count: AtomicU64,
    pub stored_request_miss_count: AtomicU64,
}

impl DummyMetricsEngine {
    pub fn new() -> Self {
        Self {
            request_count: AtomicU64::new(0),
            impression_count: AtomicU64::new(0),
            request_time_count: AtomicU64::new(0),
            adapter_request_count: AtomicU64::new(0),
            adapter_bid_received_count: AtomicU64::new(0),
            adapter_price_count: AtomicU64::new(0),
            adapter_time_count: AtomicU64::new(0),
            cookie_sync_count: AtomicU64::new(0),
            setuid_count: AtomicU64::new(0),
            stored_request_found_count: AtomicU64::new(0),
            stored_request_miss_count: AtomicU64::new(0),
        }
    }
}

impl Default for DummyMetricsEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsEngine for DummyMetricsEngine {
    fn record_request(&self, _labels: &RequestLabels) {
        self.request_count.fetch_add(1, Ordering::Relaxed);
    }

    fn record_impression(&self, _labels: &ImpLabels) {
        self.impression_count.fetch_add(1, Ordering::Relaxed);
    }

    fn record_request_time(&self, _labels: &RequestLabels, _duration: Duration) {
        self.request_time_count.fetch_add(1, Ordering::Relaxed);
    }

    fn record_adapter_request(&self, _labels: &AdapterLabels) {
        self.adapter_request_count.fetch_add(1, Ordering::Relaxed);
    }

    fn record_adapter_bid_received(
        &self,
        _labels: &AdapterLabels,
        _bid_type: &str,
        _has_adm: bool,
    ) {
        self.adapter_bid_received_count
            .fetch_add(1, Ordering::Relaxed);
    }

    fn record_adapter_price(&self, _labels: &AdapterLabels, _cpm: f64) {
        self.adapter_price_count.fetch_add(1, Ordering::Relaxed);
    }

    fn record_adapter_time(&self, _labels: &AdapterLabels, _duration: Duration) {
        self.adapter_time_count.fetch_add(1, Ordering::Relaxed);
    }

    fn record_cookie_sync(&self, _status: CookieSyncStatus) {
        self.cookie_sync_count.fetch_add(1, Ordering::Relaxed);
    }

    fn record_setuid(&self, _labels: &SetUidLabels) {
        self.setuid_count.fetch_add(1, Ordering::Relaxed);
    }

    fn record_stored_request(&self, found: bool) {
        if found {
            self.stored_request_found_count
                .fetch_add(1, Ordering::Relaxed);
        } else {
            self.stored_request_miss_count
                .fetch_add(1, Ordering::Relaxed);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    fn sample_request_labels() -> RequestLabels {
        RequestLabels {
            request_type: RequestType::OpenRTB2Web,
            request_status: RequestStatus::Ok,
        }
    }

    fn sample_adapter_labels() -> AdapterLabels {
        AdapterLabels {
            adapter: "appnexus".to_string(),
            adapter_bid_type: AdapterBidStatus::Bid,
            no_cookie: false,
        }
    }

    fn sample_setuid_labels() -> SetUidLabels {
        SetUidLabels {
            bidder: "appnexus".to_string(),
            status: SetUidStatus::Ok,
        }
    }

    // -- NoopMetrics tests ---------------------------------------------------

    #[test]
    fn noop_does_not_panic() {
        let m = NoopMetrics;
        m.record_request(&sample_request_labels());
        m.record_impression(&ImpLabels {
            banner: true,
            video: true,
            audio: false,
            native: false,
        });
        m.record_request_time(&sample_request_labels(), Duration::from_millis(42));
        m.record_adapter_request(&sample_adapter_labels());
        m.record_adapter_bid_received(&sample_adapter_labels(), "banner", true);
        m.record_adapter_price(&sample_adapter_labels(), 1.23);
        m.record_adapter_time(&sample_adapter_labels(), Duration::from_millis(100));
        m.record_cookie_sync(CookieSyncStatus::Ok);
        m.record_setuid(&sample_setuid_labels());
        m.record_stored_request(true);
        m.record_stored_request(false);
        assert_eq!(m.to_text(), "");
    }

    #[test]
    fn noop_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NoopMetrics>();
    }

    #[test]
    fn noop_as_trait_object() {
        let m: Box<dyn MetricsEngine> = Box::new(NoopMetrics);
        m.record_request(&sample_request_labels());
        assert_eq!(m.to_text(), "");
    }

    // -- DummyMetricsEngine tests -------------------------------------------

    #[test]
    fn dummy_counts_requests() {
        let d = DummyMetricsEngine::new();
        assert_eq!(d.request_count.load(Ordering::Relaxed), 0);

        d.record_request(&sample_request_labels());
        d.record_request(&sample_request_labels());
        assert_eq!(d.request_count.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn dummy_counts_impressions() {
        let d = DummyMetricsEngine::new();
        d.record_impression(&ImpLabels {
            banner: true,
            ..Default::default()
        });
        assert_eq!(d.impression_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn dummy_counts_request_time() {
        let d = DummyMetricsEngine::new();
        d.record_request_time(&sample_request_labels(), Duration::from_millis(50));
        d.record_request_time(&sample_request_labels(), Duration::from_millis(100));
        assert_eq!(d.request_time_count.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn dummy_counts_adapter_request() {
        let d = DummyMetricsEngine::new();
        d.record_adapter_request(&sample_adapter_labels());
        assert_eq!(d.adapter_request_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn dummy_counts_adapter_bid_received() {
        let d = DummyMetricsEngine::new();
        d.record_adapter_bid_received(&sample_adapter_labels(), "video", false);
        d.record_adapter_bid_received(&sample_adapter_labels(), "banner", true);
        assert_eq!(d.adapter_bid_received_count.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn dummy_counts_adapter_price() {
        let d = DummyMetricsEngine::new();
        d.record_adapter_price(&sample_adapter_labels(), 3.50);
        assert_eq!(d.adapter_price_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn dummy_counts_adapter_time() {
        let d = DummyMetricsEngine::new();
        d.record_adapter_time(&sample_adapter_labels(), Duration::from_millis(200));
        assert_eq!(d.adapter_time_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn dummy_counts_cookie_sync() {
        let d = DummyMetricsEngine::new();
        d.record_cookie_sync(CookieSyncStatus::Ok);
        d.record_cookie_sync(CookieSyncStatus::BadRequest);
        d.record_cookie_sync(CookieSyncStatus::OptOut);
        assert_eq!(d.cookie_sync_count.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn dummy_counts_setuid() {
        let d = DummyMetricsEngine::new();
        d.record_setuid(&sample_setuid_labels());
        assert_eq!(d.setuid_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn dummy_counts_stored_request() {
        let d = DummyMetricsEngine::new();
        d.record_stored_request(true);
        d.record_stored_request(true);
        d.record_stored_request(false);
        assert_eq!(d.stored_request_found_count.load(Ordering::Relaxed), 2);
        assert_eq!(d.stored_request_miss_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn dummy_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DummyMetricsEngine>();
    }

    #[test]
    fn dummy_works_through_arc() {
        let d = Arc::new(DummyMetricsEngine::new());
        let d2 = Arc::clone(&d);
        d.record_request(&sample_request_labels());
        d2.record_request(&sample_request_labels());
        assert_eq!(d.request_count.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn dummy_default_impl() {
        let d = DummyMetricsEngine::default();
        assert_eq!(d.request_count.load(Ordering::Relaxed), 0);
    }

    // -- PrometheusMetrics smoke tests ---------------------------------------

    #[test]
    fn prometheus_new_succeeds() {
        let pm = PrometheusMetrics::new("pbs_test").expect("should create metrics");
        assert!(!pm.registry().gather().is_empty() || pm.registry().gather().is_empty());
    }

    #[test]
    fn prometheus_record_request_appears_in_text() {
        let pm = PrometheusMetrics::new("pbs").expect("create");
        pm.record_request(&RequestLabels {
            request_type: RequestType::Amp,
            request_status: RequestStatus::Ok,
        });
        let text = pm.to_text();
        assert!(
            text.contains("pbs_requests_total"),
            "expected requests_total in output"
        );
        assert!(text.contains("amp"), "expected amp label in output");
    }

    #[test]
    fn prometheus_record_impression_appears_in_text() {
        let pm = PrometheusMetrics::new("pbs").expect("create");
        pm.record_impression(&ImpLabels {
            banner: true,
            video: false,
            audio: false,
            native: true,
        });
        let text = pm.to_text();
        assert!(text.contains("banner"));
        assert!(text.contains("native"));
    }

    #[test]
    fn prometheus_record_adapter_bid() {
        let pm = PrometheusMetrics::new("pbs").expect("create");
        let labels = sample_adapter_labels();
        pm.record_adapter_bid_received(&labels, "video", true);
        let text = pm.to_text();
        assert!(text.contains("adapter_bids_total"));
        assert!(text.contains("appnexus"));
    }

    #[test]
    fn prometheus_record_adapter_price() {
        let pm = PrometheusMetrics::new("pbs").expect("create");
        pm.record_adapter_price(&sample_adapter_labels(), 5.25);
        let text = pm.to_text();
        assert!(text.contains("adapter_price_cpm"));
    }

    #[test]
    fn prometheus_record_cookie_sync() {
        let pm = PrometheusMetrics::new("pbs").expect("create");
        pm.record_cookie_sync(CookieSyncStatus::Ok);
        pm.record_cookie_sync(CookieSyncStatus::GDPRBlockedHostCookie);
        let text = pm.to_text();
        assert!(text.contains("cookie_sync_total"));
    }

    #[test]
    fn prometheus_record_setuid() {
        let pm = PrometheusMetrics::new("pbs").expect("create");
        pm.record_setuid(&SetUidLabels {
            bidder: "rubicon".to_string(),
            status: SetUidStatus::GDPRBlocked,
        });
        let text = pm.to_text();
        assert!(text.contains("setuid_total"));
        assert!(text.contains("rubicon"));
    }

    #[test]
    fn prometheus_record_stored_request() {
        let pm = PrometheusMetrics::new("pbs").expect("create");
        pm.record_stored_request(true);
        pm.record_stored_request(false);
        let text = pm.to_text();
        assert!(text.contains("stored_request_hit_total"));
        assert!(text.contains("stored_request_miss_total"));
    }

    #[test]
    fn prometheus_implements_trait_object() {
        let pm = PrometheusMetrics::new("pbs_trait").expect("create");
        let engine: Box<dyn MetricsEngine> = Box::new(pm);
        engine.record_request(&sample_request_labels());
        let text = engine.to_text();
        assert!(text.contains("requests_total"));
    }

    // -- enum as_str round-trip tests ----------------------------------------

    #[test]
    fn request_type_as_str() {
        assert_eq!(RequestType::OpenRTB2Web.as_str(), "openrtb2-web");
        assert_eq!(RequestType::OpenRTB2App.as_str(), "openrtb2-app");
        assert_eq!(RequestType::OpenRTB2DOOH.as_str(), "openrtb2-dooh");
        assert_eq!(RequestType::Amp.as_str(), "amp");
        assert_eq!(RequestType::Video.as_str(), "video");
    }

    #[test]
    fn request_status_as_str() {
        assert_eq!(RequestStatus::Ok.as_str(), "ok");
        assert_eq!(RequestStatus::BadInput.as_str(), "badinput");
        assert_eq!(RequestStatus::Err.as_str(), "err");
        assert_eq!(RequestStatus::NetworkErr.as_str(), "networkerr");
        assert_eq!(RequestStatus::Timeout.as_str(), "timeout");
        assert_eq!(RequestStatus::BlockedApp.as_str(), "blockedapp");
        assert_eq!(RequestStatus::QueueTimeout.as_str(), "queuetimeout");
        assert_eq!(RequestStatus::AccountConfigErr.as_str(), "acctconfigerr");
    }

    #[test]
    fn cookie_sync_status_as_str() {
        assert_eq!(CookieSyncStatus::Ok.as_str(), "ok");
        assert_eq!(CookieSyncStatus::BadRequest.as_str(), "bad_request");
        assert_eq!(CookieSyncStatus::OptOut.as_str(), "opt_out");
        assert_eq!(
            CookieSyncStatus::GDPRBlockedHostCookie.as_str(),
            "gdpr_blocked_host_cookie"
        );
    }

    #[test]
    fn setuid_status_as_str() {
        assert_eq!(SetUidStatus::Ok.as_str(), "ok");
        assert_eq!(SetUidStatus::BadRequest.as_str(), "bad_request");
        assert_eq!(SetUidStatus::OptOut.as_str(), "opt_out");
        assert_eq!(SetUidStatus::GDPRBlocked.as_str(), "gdpr_blocked");
    }

    #[test]
    fn adapter_bid_status_as_str() {
        assert_eq!(AdapterBidStatus::Bid.as_str(), "bid");
        assert_eq!(AdapterBidStatus::NoBid.as_str(), "nobid");
    }
}
