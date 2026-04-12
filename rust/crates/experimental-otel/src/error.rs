//! Error types for the experimental OpenTelemetry integration.

use thiserror::Error;

/// Errors produced by this crate.
#[derive(Debug, Error)]
pub enum OtelError {
    /// Failed to build or install the OTLP tracer pipeline.
    #[error("failed to initialize OTLP tracer: {0}")]
    TracerInit(String),

    /// Failed to build or install the OTLP meter pipeline.
    #[error("failed to initialize OTLP meter: {0}")]
    MeterInit(String),

    /// The provided configuration is invalid.
    #[error("invalid OpenTelemetry configuration: {0}")]
    InvalidConfig(String),

    /// Failed to install a `tracing` subscriber layer.
    #[error("failed to install tracing subscriber: {0}")]
    SubscriberInstall(String),

    /// Generic / wrapped upstream error.
    #[error("opentelemetry error: {0}")]
    Other(String),
}
