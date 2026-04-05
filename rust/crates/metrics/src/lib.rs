use prometheus::{GaugeVec, HistogramOpts, HistogramVec, IntCounterVec, Opts, Registry};

/// Status of an auction request
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestStatus {
    Ok,
    BadInput,
    BadServerResponse,
    Unknown,
    Timeout,
}

impl RequestStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RequestStatus::Ok => "ok",
            RequestStatus::BadInput => "bad_input",
            RequestStatus::BadServerResponse => "bad_server_response",
            RequestStatus::Unknown => "unknown",
            RequestStatus::Timeout => "timeout",
        }
    }
}

/// Status of a bidder response
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BidderStatus {
    Got,
    TimedOut,
    NoBid,
    Error,
}

impl BidderStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BidderStatus::Got => "got_bids",
            BidderStatus::TimedOut => "timed_out",
            BidderStatus::NoBid => "no_bid",
            BidderStatus::Error => "error",
        }
    }
}

/// Metrics is the primary trait for recording auction metrics.
/// Both `PrometheusMetrics` and `NoopMetrics` implement this interface.
pub trait Metrics: Send + Sync {
    /// Record an HTTP request with endpoint, status code, and duration.
    fn record_request(&self, endpoint: &str, status: u16, duration_ms: u64);
    /// Record a successful bid from a bidder.
    fn record_bid(&self, bidder: &str, bid_type: &str);
    /// Record a no-bid response from a bidder.
    fn record_no_bid(&self, bidder: &str);
    /// Record an error from a bidder or system component.
    fn record_error(&self, bidder: &str, err_type: &str);
    /// Render all metrics in Prometheus text format.
    fn to_text(&self) -> String;
}

/// MetricsEngine defines the interface for recording metrics (legacy; kept for
/// backward compatibility with existing call-sites in exchange and endpoints).
pub trait MetricsEngine: Send + Sync {
    fn record_request(&self, entry_point: &str, status: RequestStatus);
    fn record_bidder_request(&self, bidder: &str);
    fn record_bidder_response(&self, bidder: &str, status: BidderStatus, duration_ms: u64);
    fn record_bid_count(&self, bidder: &str, bid_type: &str);
    /// Record a bidder-level error by error type (e.g. "timeout", "bad_server_response").
    fn record_bidder_error(&self, bidder: &str, err_type: &str);
    /// Record the total duration of a completed auction for a given entry point.
    fn record_auction_duration(&self, entry_point: &str, duration_ms: u64);
    fn record_connection_accepted(&self);
    fn record_connection_closed(&self);
}

/// NoopMetrics is a no-op implementation of both traits for testing.
pub struct NoopMetrics;

impl Metrics for NoopMetrics {
    fn record_request(&self, _endpoint: &str, _status: u16, _duration_ms: u64) {}
    fn record_bid(&self, _bidder: &str, _bid_type: &str) {}
    fn record_no_bid(&self, _bidder: &str) {}
    fn record_error(&self, _bidder: &str, _err_type: &str) {}
    fn to_text(&self) -> String {
        String::new()
    }
}

impl MetricsEngine for NoopMetrics {
    fn record_request(&self, _entry_point: &str, _status: RequestStatus) {}
    fn record_bidder_request(&self, _bidder: &str) {}
    fn record_bidder_response(&self, _bidder: &str, _status: BidderStatus, _duration_ms: u64) {}
    fn record_bid_count(&self, _bidder: &str, _bid_type: &str) {}
    fn record_bidder_error(&self, _bidder: &str, _err_type: &str) {}
    fn record_auction_duration(&self, _entry_point: &str, _duration_ms: u64) {}
    fn record_connection_accepted(&self) {}
    fn record_connection_closed(&self) {}
}

/// Histogram bucket boundaries (ms) for request and auction duration.
///
/// Covers 50 ms up to 2000 ms to match the Go implementation's distribution.
const REQUEST_DURATION_BUCKETS: &[f64] = &[
    50.0, 100.0, 200.0, 300.0, 500.0, 750.0, 1000.0, 1500.0, 2000.0,
];

/// PrometheusMetrics is a Prometheus-backed implementation of both `Metrics`
/// and `MetricsEngine` traits.
pub struct PrometheusMetrics {
    registry: Registry,
    /// Total auction / HTTP request counter — labels: entry_point, status (string)
    requests_total: IntCounterVec,
    /// HTTP-level request latency histogram — labels: entry_point, status_code (numeric string)
    request_duration_ms: HistogramVec,
    /// Total number of requests fanned out to each bidder — label: bidder
    bidder_requests_total: IntCounterVec,
    /// Bidder response-time histogram — labels: bidder, status
    bidder_response_time_ms: HistogramVec,
    /// Total auction duration histogram — label: entry_point
    auction_duration_ms: HistogramVec,
    /// Total bids received — labels: bidder, type
    bids_total: IntCounterVec,
    /// Total no-bid responses — label: bidder
    no_bids_total: IntCounterVec,
    /// Total errors — labels: bidder, err_type
    errors_total: IntCounterVec,
    /// Active bidder count gauge — label: bidder
    active_bidders: GaugeVec,
    connections_accepted: prometheus::IntCounter,
    connections_closed: prometheus::IntCounter,
}

impl PrometheusMetrics {
    pub fn new(namespace: &str) -> Result<Self, prometheus::Error> {
        let registry = Registry::new();

        // ── Counters ─────────────────────────────────────────────────────────

        let requests_total = IntCounterVec::new(
            Opts::new("requests_total", "Total number of auction requests")
                .namespace(namespace),
            &["entry_point", "status"],
        )?;

        let bidder_requests_total = IntCounterVec::new(
            Opts::new("bidder_requests_total", "Total number of requests to each bidder")
                .namespace(namespace),
            &["bidder"],
        )?;

        let bids_total = IntCounterVec::new(
            Opts::new("bids_total", "Total number of bids received from bidders")
                .namespace(namespace),
            &["bidder", "type"],
        )?;

        let no_bids_total = IntCounterVec::new(
            Opts::new("no_bids_total", "Total number of no-bid responses from bidders")
                .namespace(namespace),
            &["bidder"],
        )?;

        let errors_total = IntCounterVec::new(
            Opts::new("errors_total", "Total number of errors by component and type")
                .namespace(namespace),
            &["bidder", "err_type"],
        )?;

        let connections_accepted = prometheus::IntCounter::with_opts(
            Opts::new("connections_accepted", "Total number of accepted connections")
                .namespace(namespace),
        )?;

        let connections_closed = prometheus::IntCounter::with_opts(
            Opts::new("connections_closed", "Total number of closed connections")
                .namespace(namespace),
        )?;

        // ── Histograms ───────────────────────────────────────────────────────

        let request_duration_ms = HistogramVec::new(
            HistogramOpts::new(
                "request_duration_ms",
                "End-to-end HTTP request duration in milliseconds",
            )
            .namespace(namespace)
            .buckets(REQUEST_DURATION_BUCKETS.to_vec()),
            &["entry_point", "status_code"],
        )?;

        let bidder_response_time_ms = HistogramVec::new(
            HistogramOpts::new(
                "bidder_response_time_ms",
                "Time taken for bidder to respond in milliseconds",
            )
            .namespace(namespace)
            .buckets(vec![25.0, 50.0, 100.0, 200.0, 400.0, 800.0, 1600.0]),
            &["bidder", "status"],
        )?;

        let auction_duration_ms = HistogramVec::new(
            HistogramOpts::new(
                "auction_duration_ms",
                "Total auction duration in milliseconds",
            )
            .namespace(namespace)
            .buckets(REQUEST_DURATION_BUCKETS.to_vec()),
            &["entry_point"],
        )?;

        // ── Gauges ───────────────────────────────────────────────────────────

        let active_bidders = GaugeVec::new(
            Opts::new("active_bidders", "Number of active bidders registered in this instance")
                .namespace(namespace),
            &["bidder"],
        )?;

        // ── Register all metrics ─────────────────────────────────────────────

        registry.register(Box::new(requests_total.clone()))?;
        registry.register(Box::new(request_duration_ms.clone()))?;
        registry.register(Box::new(bidder_requests_total.clone()))?;
        registry.register(Box::new(bidder_response_time_ms.clone()))?;
        registry.register(Box::new(auction_duration_ms.clone()))?;
        registry.register(Box::new(bids_total.clone()))?;
        registry.register(Box::new(no_bids_total.clone()))?;
        registry.register(Box::new(errors_total.clone()))?;
        registry.register(Box::new(active_bidders.clone()))?;
        registry.register(Box::new(connections_accepted.clone()))?;
        registry.register(Box::new(connections_closed.clone()))?;

        Ok(Self {
            registry,
            requests_total,
            request_duration_ms,
            bidder_requests_total,
            bidder_response_time_ms,
            auction_duration_ms,
            bids_total,
            no_bids_total,
            errors_total,
            active_bidders,
            connections_accepted,
            connections_closed,
        })
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Gather all metrics in Prometheus text format.
    pub fn gather_text(&self) -> String {
        use prometheus::Encoder;
        let encoder = prometheus::TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap_or_default();
        String::from_utf8(buffer).unwrap_or_default()
    }

    /// Record the total duration of a completed auction.
    pub fn record_auction_duration(&self, entry_point: &str, duration_ms: u64) {
        self.auction_duration_ms
            .with_label_values(&[entry_point])
            .observe(duration_ms as f64);
    }

    /// Mark a bidder as active (gauge = 1) so it appears in the metrics output.
    pub fn set_bidder_active(&self, bidder: &str, active: bool) {
        let value = if active { 1.0 } else { 0.0 };
        self.active_bidders.with_label_values(&[bidder]).set(value);
    }

    /// Record an HTTP-level request with its status code and end-to-end duration.
    ///
    /// This inherent method avoids trait-disambiguation ambiguity at call sites
    /// where both `Metrics` and `MetricsEngine` are in scope.
    pub fn record_http_request(&self, endpoint: &str, status_code: u16, duration_ms: u64) {
        let status_str = status_code.to_string();
        self.requests_total
            .with_label_values(&[endpoint, &status_str])
            .inc();
        self.request_duration_ms
            .with_label_values(&[endpoint, &status_str])
            .observe(duration_ms as f64);
    }
}

// ── Metrics (primary / new trait) ────────────────────────────────────────────

impl Metrics for PrometheusMetrics {
    fn record_request(&self, endpoint: &str, status: u16, duration_ms: u64) {
        let status_str = status.to_string();
        // Also bump the legacy requests_total counter using the numeric status as string.
        self.requests_total
            .with_label_values(&[endpoint, &status_str])
            .inc();
        self.request_duration_ms
            .with_label_values(&[endpoint, &status_str])
            .observe(duration_ms as f64);
    }

    fn record_bid(&self, bidder: &str, bid_type: &str) {
        self.bids_total.with_label_values(&[bidder, bid_type]).inc();
    }

    fn record_no_bid(&self, bidder: &str) {
        self.no_bids_total.with_label_values(&[bidder]).inc();
    }

    fn record_error(&self, bidder: &str, err_type: &str) {
        self.errors_total.with_label_values(&[bidder, err_type]).inc();
    }

    fn to_text(&self) -> String {
        self.gather_text()
    }
}

// ── MetricsEngine (legacy trait) ─────────────────────────────────────────────

impl MetricsEngine for PrometheusMetrics {
    fn record_request(&self, entry_point: &str, status: RequestStatus) {
        self.requests_total
            .with_label_values(&[entry_point, status.as_str()])
            .inc();
    }

    fn record_bidder_request(&self, bidder: &str) {
        self.bidder_requests_total
            .with_label_values(&[bidder])
            .inc();
    }

    fn record_bidder_response(&self, bidder: &str, status: BidderStatus, duration_ms: u64) {
        self.bidder_response_time_ms
            .with_label_values(&[bidder, status.as_str()])
            .observe(duration_ms as f64);
    }

    fn record_bid_count(&self, bidder: &str, bid_type: &str) {
        self.bids_total
            .with_label_values(&[bidder, bid_type])
            .inc();
    }

    fn record_bidder_error(&self, bidder: &str, err_type: &str) {
        self.errors_total
            .with_label_values(&[bidder, err_type])
            .inc();
    }

    fn record_auction_duration(&self, entry_point: &str, duration_ms: u64) {
        self.auction_duration_ms
            .with_label_values(&[entry_point])
            .observe(duration_ms as f64);
    }

    fn record_connection_accepted(&self) {
        self.connections_accepted.inc();
    }

    fn record_connection_closed(&self) {
        self.connections_closed.inc();
    }
}
