use prometheus::{HistogramOpts, HistogramVec, IntCounterVec, Opts, Registry};

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

/// MetricsEngine defines the interface for recording metrics
pub trait MetricsEngine: Send + Sync {
    fn record_request(&self, entry_point: &str, status: RequestStatus);
    fn record_bidder_request(&self, bidder: &str);
    fn record_bidder_response(&self, bidder: &str, status: BidderStatus, duration_ms: u64);
    fn record_bid_count(&self, bidder: &str, bid_type: &str);
    fn record_connection_accepted(&self);
    fn record_connection_closed(&self);
}

/// NoopMetrics is a no-op implementation of MetricsEngine for testing
pub struct NoopMetrics;

impl MetricsEngine for NoopMetrics {
    fn record_request(&self, _entry_point: &str, _status: RequestStatus) {}
    fn record_bidder_request(&self, _bidder: &str) {}
    fn record_bidder_response(&self, _bidder: &str, _status: BidderStatus, _duration_ms: u64) {}
    fn record_bid_count(&self, _bidder: &str, _bid_type: &str) {}
    fn record_connection_accepted(&self) {}
    fn record_connection_closed(&self) {}
}

/// PrometheusMetrics is a Prometheus-backed implementation of MetricsEngine
pub struct PrometheusMetrics {
    registry: Registry,
    requests_total: IntCounterVec,
    bidder_requests_total: IntCounterVec,
    bidder_response_time_ms: HistogramVec,
    bids_total: IntCounterVec,
    connections_accepted: prometheus::IntCounter,
    connections_closed: prometheus::IntCounter,
}

impl PrometheusMetrics {
    pub fn new(namespace: &str) -> Result<Self, prometheus::Error> {
        let registry = Registry::new();

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

        let bidder_response_time_ms = HistogramVec::new(
            HistogramOpts::new(
                "bidder_response_time_ms",
                "Time taken for bidder to respond in milliseconds",
            )
            .namespace(namespace)
            .buckets(vec![25.0, 50.0, 100.0, 200.0, 400.0, 800.0, 1600.0]),
            &["bidder", "status"],
        )?;

        let bids_total = IntCounterVec::new(
            Opts::new("bids_total", "Total number of bids received from bidders")
                .namespace(namespace),
            &["bidder", "type"],
        )?;

        let connections_accepted = prometheus::IntCounter::with_opts(
            Opts::new("connections_accepted", "Total number of accepted connections")
                .namespace(namespace),
        )?;

        let connections_closed = prometheus::IntCounter::with_opts(
            Opts::new("connections_closed", "Total number of closed connections")
                .namespace(namespace),
        )?;

        registry.register(Box::new(requests_total.clone()))?;
        registry.register(Box::new(bidder_requests_total.clone()))?;
        registry.register(Box::new(bidder_response_time_ms.clone()))?;
        registry.register(Box::new(bids_total.clone()))?;
        registry.register(Box::new(connections_accepted.clone()))?;
        registry.register(Box::new(connections_closed.clone()))?;

        Ok(Self {
            registry,
            requests_total,
            bidder_requests_total,
            bidder_response_time_ms,
            bids_total,
            connections_accepted,
            connections_closed,
        })
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Gather all metrics in Prometheus text format
    pub fn gather_text(&self) -> String {
        use prometheus::Encoder;
        let encoder = prometheus::TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap_or_default();
        String::from_utf8(buffer).unwrap_or_default()
    }
}

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

    fn record_connection_accepted(&self) {
        self.connections_accepted.inc();
    }

    fn record_connection_closed(&self) {
        self.connections_closed.inc();
    }
}
