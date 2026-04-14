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

        Self {
            account_fetcher,
            analytics,
            metrics,
            chooser,
            config,
        }
    }
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
