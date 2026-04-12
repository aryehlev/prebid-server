//! # experimental-otel
//!
//! **EXPERIMENTAL**: OpenTelemetry distributed tracing and metrics integration
//! for Prebid Server.
//!
//! This crate wires an OTLP (gRPC/tonic) tracer and meter pipeline into the
//! `tracing` ecosystem and exposes semantic-convention helpers for auction,
//! adapter, cache, and stored-request spans, plus a lazily-initialized
//! [`metrics::PbsMeters`] bundle of counters/histograms/up-down-counters.
//!
//! The API here is **unstable** and may change without notice. It is not wired
//! into the main server by default; enable it by constructing an
//! [`init::OtelConfig`] and calling [`init::init_otel`] during startup.
//!
//! ## Quick start
//!
//! ```no_run
//! use experimental_otel::init::{init_otel, OtelConfig};
//!
//! # async fn run() -> Result<(), experimental_otel::error::OtelError> {
//! let cfg = OtelConfig {
//!     service_name: "prebid-server".into(),
//!     service_version: env!("CARGO_PKG_VERSION").into(),
//!     otlp_endpoint: "http://localhost:4317".into(),
//!     sample_ratio: 0.1,
//!     resource_attributes: vec![("deployment.environment".into(), "dev".into())],
//! };
//! let _guard = init_otel(cfg)?;
//! # Ok(()) }
//! ```

pub mod error;
pub mod init;
pub mod metrics;
pub mod propagation;
pub mod spans;

pub use error::OtelError;
pub use init::{init_otel, OtelConfig, OtelGuard};
pub use metrics::PbsMeters;
pub use propagation::{extract_context_from_headers, inject_context_into_headers};
