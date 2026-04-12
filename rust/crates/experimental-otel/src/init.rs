//! Initialization of the OTLP tracer/meter pipelines and `tracing` layer.
//!
//! The entry point is [`init_otel`], which returns an [`OtelGuard`] that
//! shuts down the global providers on drop.
//!
//! If [`OtelConfig::otlp_endpoint`] is empty, a no-op tracer provider is
//! installed instead of exporting. This is useful for tests and for opt-in
//! deployments that ship the dependency without actually emitting telemetry.

use std::time::Duration;

use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::{Sampler, TracerProvider};
use opentelemetry_sdk::Resource;

use crate::error::OtelError;

/// Runtime configuration for the OpenTelemetry pipeline.
#[derive(Debug, Clone)]
pub struct OtelConfig {
    /// `service.name` resource attribute.
    pub service_name: String,
    /// `service.version` resource attribute.
    pub service_version: String,
    /// gRPC OTLP collector endpoint, e.g. `http://localhost:4317`.
    ///
    /// If empty, a no-op tracer provider is installed and no traffic is sent.
    pub otlp_endpoint: String,
    /// Ratio-based sampler probability, `0.0..=1.0`.
    pub sample_ratio: f64,
    /// Additional resource attributes to attach.
    pub resource_attributes: Vec<(String, String)>,
}

impl Default for OtelConfig {
    fn default() -> Self {
        Self {
            service_name: "prebid-server".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            otlp_endpoint: String::new(),
            sample_ratio: 1.0,
            resource_attributes: Vec::new(),
        }
    }
}

/// RAII guard that shuts down the installed tracer provider on drop.
///
/// Keep this alive for the lifetime of the program (typically in `main`).
#[must_use = "dropping OtelGuard immediately shuts down the tracer provider"]
pub struct OtelGuard {
    provider: Option<TracerProvider>,
    tracer: Option<opentelemetry_sdk::trace::Tracer>,
}

impl OtelGuard {
    fn new(provider: TracerProvider, tracer: opentelemetry_sdk::trace::Tracer) -> Self {
        Self {
            provider: Some(provider),
            tracer: Some(tracer),
        }
    }

    /// A guard that owns no provider — used for the noop fallback.
    fn noop() -> Self {
        Self {
            provider: None,
            tracer: None,
        }
    }

    /// Return a clone of the installed SDK tracer, if any. Returns `None`
    /// for the noop fallback.
    pub fn tracer(&self) -> Option<opentelemetry_sdk::trace::Tracer> {
        self.tracer.clone()
    }
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take() {
            // Best-effort shutdown; errors here are not actionable.
            let _ = provider.shutdown();
        }
    }
}

fn build_resource(cfg: &OtelConfig) -> Resource {
    let mut kvs: Vec<KeyValue> = Vec::with_capacity(2 + cfg.resource_attributes.len());
    kvs.push(KeyValue::new("service.name", cfg.service_name.clone()));
    kvs.push(KeyValue::new(
        "service.version",
        cfg.service_version.clone(),
    ));
    for (k, v) in &cfg.resource_attributes {
        kvs.push(KeyValue::new(k.clone(), v.clone()));
    }
    Resource::new(kvs)
}

/// Initialize OpenTelemetry using the given configuration.
///
/// * Installs the W3C trace-context propagator as the global propagator.
/// * If `otlp_endpoint` is empty, installs a no-op tracer provider and
///   returns a guard that does nothing on drop.
/// * Otherwise, builds an OTLP/gRPC (tonic) tracer pipeline on the Tokio
///   runtime, installs it as the global tracer provider, and returns a
///   guard that shuts it down on drop.
///
/// This function does **not** configure an OTLP meter pipeline; metrics are
/// exposed via [`crate::metrics::PbsMeters`] using the global meter, which
/// callers can wire to whatever meter provider they install.
pub fn init_otel(cfg: OtelConfig) -> Result<OtelGuard, OtelError> {
    if !(0.0..=1.0).contains(&cfg.sample_ratio) {
        return Err(OtelError::InvalidConfig(format!(
            "sample_ratio must be in 0.0..=1.0, got {}",
            cfg.sample_ratio
        )));
    }

    // Always install the W3C propagator so extract/inject works regardless
    // of whether we have a real exporter.
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

    if cfg.otlp_endpoint.is_empty() {
        // Noop fallback: install an SDK provider with no exporters. It still
        // honors the propagator and won't emit any traffic.
        let provider = TracerProvider::builder()
            .with_resource(build_resource(&cfg))
            .with_sampler(Sampler::AlwaysOff)
            .build();
        let tracer = provider.tracer(cfg.service_name.clone());
        opentelemetry::global::set_tracer_provider(provider.clone());
        return Ok(OtelGuard::new(provider, tracer));
    }

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(cfg.otlp_endpoint.clone())
        .with_timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| OtelError::TracerInit(e.to_string()))?;

    let provider = TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_resource(build_resource(&cfg))
        .with_sampler(Sampler::TraceIdRatioBased(cfg.sample_ratio))
        .build();

    let tracer = provider.tracer(cfg.service_name.clone());
    opentelemetry::global::set_tracer_provider(provider.clone());

    // Note: installing a `tracing-subscriber` layer globally can only be
    // done once per process and may conflict with the host application's
    // existing subscriber. We therefore build the tracer here (so callers
    // have a working global provider) and expose it via `OtelGuard::tracer`,
    // leaving subscriber installation to the caller — see [`tracing_layer`].
    Ok(OtelGuard::new(provider, tracer))
}

/// Build a `tracing-opentelemetry` layer wrapping the provided SDK tracer.
///
/// The SDK tracer implements `PreSampledTracer`, which the layer requires.
/// Obtain a tracer from [`OtelGuard::tracer`] (or directly from a
/// [`opentelemetry_sdk::trace::TracerProvider`]).
pub fn tracing_layer<S>(
    tracer: opentelemetry_sdk::trace::Tracer,
) -> tracing_opentelemetry::OpenTelemetryLayer<S, opentelemetry_sdk::trace::Tracer>
where
    S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
{
    tracing_opentelemetry::layer().with_tracer(tracer)
}

#[doc(hidden)]
pub fn __noop_guard_for_tests() -> OtelGuard {
    OtelGuard::noop()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_with_empty_endpoint_does_not_panic() {
        let cfg = OtelConfig {
            service_name: "test-svc".into(),
            service_version: "0.0.0".into(),
            otlp_endpoint: String::new(),
            sample_ratio: 1.0,
            resource_attributes: vec![("env".into(), "test".into())],
        };
        let guard = init_otel(cfg).expect("noop init should succeed");
        drop(guard);
    }

    #[test]
    fn invalid_sample_ratio_rejected() {
        let cfg = OtelConfig {
            sample_ratio: 2.0,
            ..OtelConfig::default()
        };
        assert!(init_otel(cfg).is_err());
    }
}
