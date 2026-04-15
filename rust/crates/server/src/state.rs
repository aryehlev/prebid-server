//! Shared application state threaded through the axum router.
//!
//! [`AppState`] bundles together the handful of long-lived dependencies that
//! the HTTP handlers in [`crate::router`] need to do real work:
//!
//! * an [`account::AccountFetcher`] for resolving publisher accounts;
//! * a [`analytics::PbsAnalyticsModule`] for fire-and-forget event logging;
//! * a [`pbs_metrics::PrometheusMetrics`] exporter for `/metrics`;
//! * a [`usersync::StandardChooser`] for `/cookie_sync`;
//! * the top-level [`pbs_config::top::Configuration`] so handlers can peek at
//!   static bidder info, host-cookie config, etc.
//!
//! Every field is wrapped in an `Arc` so the state itself is cheap to clone
//! into `State<Arc<AppState>>` extractors.
//!
//! For local development and integration tests, [`AppState::dev`] wires up
//! no-op defaults: a null analytics backend, a fresh (namespaced) Prometheus
//! registry, an empty chooser, a default configuration, and an account
//! fetcher that returns `NotFound` for every id. This keeps the binary
//! runnable end-to-end while the rest of the Rust port is still being wired
//! up.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

/// Shared state injected into every axum handler.
///
/// Cheap to clone: every field is already reference counted.
#[derive(Clone)]
pub struct AppState {
    /// Pluggable account fetcher (file/memory/HTTP backends live elsewhere).
    pub account_fetcher: Arc<dyn account::AccountFetcher>,
    /// Analytics fan-out module. Handlers should call the relevant
    /// `log_*_object` method once per completed request.
    pub analytics: Arc<dyn analytics::PbsAnalyticsModule>,
    /// Prometheus metrics exporter backing the `/metrics` endpoint.
    pub metrics: Arc<pbs_metrics::PrometheusMetrics>,
    /// Bidder selection engine for `/cookie_sync`.
    pub chooser: Arc<usersync::StandardChooser>,
    /// Top-level prebid-server configuration (multi-source variant).
    pub config: Arc<pbs_config::top::Configuration>,
    /// Shared reference to the `pbs-exchange` engine used for
    /// `/openrtb2/*` handlers. `None` in minimal test configurations where
    /// no exchange is wired up.
    pub exchange: Option<Arc<pbs_exchange::Exchange>>,
    /// Pre-built `pbs_endpoints::AppState` used to proxy auction/amp/video
    /// requests straight into the production endpoint handlers. `None` when
    /// no exchange has been wired up; when `Some`, the router forwards the
    /// openrtb2 routes through these handlers.
    pub endpoints_state: Option<pbs_endpoints::AppState>,
}

impl AppState {
    /// Wire up a development-mode state with no-op defaults.
    ///
    /// The returned state is safe to serve HTTP traffic on: every dependency
    /// is a working implementation (not a panic-on-use stub), but they
    /// intentionally do nothing beyond satisfying the trait contracts.
    pub fn dev() -> Self {
        // A tiny in-process account fetcher that always reports NotFound.
        // This matches the "no backend configured" behaviour of the Go
        // server and keeps /setuid / /cookie_sync endpoints wire-compatible
        // without requiring a real accounts directory.
        let account_fetcher: Arc<dyn account::AccountFetcher> =
            Arc::new(EmptyAccountFetcher::default());

        let analytics: Arc<dyn analytics::PbsAnalyticsModule> =
            Arc::new(analytics::NullAnalyticsBackend::new());

        // Namespace the metrics registry so that repeated AppState::dev()
        // calls inside a single process do not collide on global metric
        // names. PrometheusMetrics::new only fails on duplicate registration,
        // which should not happen in a fresh registry, so we unwrap.
        let metrics = Arc::new(
            pbs_metrics::PrometheusMetrics::new("prebid_server")
                .expect("PrometheusMetrics::new must succeed on a fresh registry"),
        );

        // Empty chooser: knows no bidders, so every sync request evaluates
        // to UnknownBidder. This is the right default until the real
        // bidder->syncer map is populated from config.
        let chooser = Arc::new(usersync::StandardChooser::new(HashMap::new()));

        let config = Arc::new(pbs_config::top::Configuration::default());

        // Wire up the dev exchange + a matching `pbs_endpoints::AppState`
        // so that /openrtb2/auction, /openrtb2/video and /openrtb2/amp can
        // actually proxy into the production endpoint handlers instead of
        // returning 501 Not Implemented. The endpoint state owns its own
        // `Exchange` by value (that's the contract `AppStateInner` exposes);
        // we keep an additional `Arc<Exchange>` on `AppState.exchange` for
        // future callers that want a shared reference.
        let exchange = Some(pbs_exchange::dev_exchange::dev_exchange());
        let endpoints_state = Some(build_dev_endpoints_state());

        Self {
            account_fetcher,
            analytics,
            metrics,
            chooser,
            config,
            exchange,
            endpoints_state,
        }
    }
}

/// Construct a minimally-wired `pbs_endpoints::AppState` that matches the
/// no-op defaults produced by [`AppState::dev`]. This is lifted nearly
/// verbatim from `pbs_endpoints`'s own unit-test helper: an empty exchange,
/// an empty stored-requests fetcher, a fresh (namespaced) Prometheus
/// registry, and default configs everywhere else.
fn build_dev_endpoints_state() -> pbs_endpoints::AppState {
    use pbs_exchange::privacy::ActivityControl;

    // Use a distinct namespace from the main server metrics registry so that
    // the two independent `PrometheusMetrics` instances do not confuse
    // anyone reading `/metrics` — the router serves its own registry and the
    // endpoint handlers record into this one.
    let endpoint_metrics = pbs_metrics::PrometheusMetrics::new("prebid_server_endpoints")
        .expect("PrometheusMetrics::new must succeed on a fresh registry");

    // The endpoint handlers still own the `Exchange` by value, so we build a
    // second empty exchange here. Both this one and the `Arc<Exchange>` on
    // `AppState.exchange` come from the same `dev_exchange` helper, so they
    // are behaviourally identical.
    let exchange = pbs_exchange::Exchange::new(HashMap::new());

    Arc::new(pbs_endpoints::AppStateInner {
        exchange,
        version: env!("CARGO_PKG_VERSION").to_string(),
        revision: "dev".to_string(),
        bidder_info: HashMap::new(),
        bidder_params: HashMap::new(),
        bidder_sync_info: HashMap::new(),
        host_cookie: pbs_endpoints::HostCookieConfig::default(),
        status_response: None,
        stored_requests: Arc::new(pbs_endpoints::StoredRequestFetcher::empty()),
        metrics: Arc::new(endpoint_metrics),
        max_request_size: 0,
        gdpr_enabled: false,
        accounts: HashMap::new(),
        currency_converter: None,
        account_required: false,
        activity_control: ActivityControl::default(),
    })
}

/// Minimal no-op `AccountFetcher` that always reports [`account::AccountError::NotFound`].
///
/// Used as the default backend in [`AppState::dev`] so that handlers can
/// exercise the full `AccountFetcher` trait object path without a real
/// accounts directory wired up.
#[derive(Debug, Default, Clone, Copy)]
struct EmptyAccountFetcher;

#[async_trait]
impl account::AccountFetcher for EmptyAccountFetcher {
    async fn fetch_account(
        &self,
        id: &str,
    ) -> Result<account::Account, account::AccountError> {
        Err(account::AccountError::NotFound(id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_state_constructs() {
        let _state = AppState::dev();
    }
}
