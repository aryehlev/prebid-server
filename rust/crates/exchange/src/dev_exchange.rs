//! Dev/test helper for constructing a minimally-wired [`Exchange`].
//!
//! This module exists so that downstream crates (notably the `server` crate)
//! can spin up a functional `Exchange` in development and integration tests
//! without having to re-derive the full production wiring (account fetchers,
//! stored-request backends, hook executors, metrics engines, analytics
//! modules, cache clients, currency converters, and so on).
//!
//! The returned exchange is deliberately "dev-grade":
//!
//! * **No adapters.** The bidder map is empty, so any real auction request
//!   will come back with zero seatbids. This is the right default for a
//!   dev server that should accept requests without crashing.
//! * **No metrics / analytics / hooks / cache / stored responses.** These
//!   are all optional on [`Exchange`] and are left as `None`, which the
//!   production code paths already handle gracefully.
//! * **Default reqwest client.** [`Exchange::new`] builds a 10-second
//!   timeout client internally, which is fine for local use.
//!
//! If you need richer dev wiring (real adapters, a tmpfs stored-requests
//! fetcher, a noop currency converter, etc.), extend [`Exchange::new`]'s
//! output in your own crate by mutating the `Arc`'s contents *before*
//! cloning it into a shared state — or construct your own `Exchange`
//! directly using its public fields.
//!
//! NOTE: this file is the ONLY modification the server crate is allowed to
//! introduce into `pbs-exchange`. Do not add production logic here.

use std::collections::HashMap;
use std::sync::Arc;

use crate::Exchange;

/// Build a minimally-configured [`Exchange`] suitable for development and
/// integration tests. All optional dependencies (metrics, analytics, hooks,
/// cache client, stored responses) are left as `None`.
///
/// Returns an `Arc<Exchange>` so the caller can cheaply share it across
/// tasks and clones of application state.
pub fn dev_exchange() -> Arc<Exchange> {
    Arc::new(Exchange::new(HashMap::new()))
}
