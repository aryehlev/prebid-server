//! W3C Trace Context propagation helpers for `http::HeaderMap`.
//!
//! These adapters bridge the `http` crate's `HeaderMap` to the
//! OpenTelemetry `Extractor` / `Injector` traits so callers can pull
//! parent context off an incoming request or inject it into an outgoing
//! one without directly depending on the OTel propagation APIs.

use std::collections::HashMap;

use http::header::{HeaderMap, HeaderName, HeaderValue};
use opentelemetry::propagation::{Extractor, Injector};
use opentelemetry::Context;

/// `Extractor` adapter over `http::HeaderMap`.
struct HeaderMapExtractor<'a>(&'a HeaderMap);

impl<'a> Extractor for HeaderMapExtractor<'a> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|k| k.as_str()).collect()
    }
}

/// `Injector` adapter over `http::HeaderMap`.
struct HeaderMapInjector<'a>(&'a mut HeaderMap);

impl<'a> Injector for HeaderMapInjector<'a> {
    fn set(&mut self, key: &str, value: String) {
        if let (Ok(name), Ok(val)) = (HeaderName::try_from(key), HeaderValue::try_from(value)) {
            self.0.insert(name, val);
        }
    }
}

/// Extract an OpenTelemetry [`Context`] from the given HTTP request headers
/// using the globally-installed text-map propagator.
///
/// For the W3C trace-context propagator (installed by [`crate::init::init_otel`]),
/// this reads the `traceparent` and optional `tracestate` headers.
pub fn extract_context_from_headers(headers: &HeaderMap) -> Context {
    opentelemetry::global::get_text_map_propagator(|prop| {
        prop.extract(&HeaderMapExtractor(headers))
    })
}

/// Inject the given OpenTelemetry [`Context`] into the outgoing HTTP
/// headers using the globally-installed text-map propagator.
pub fn inject_context_into_headers(ctx: &Context, headers: &mut HeaderMap) {
    opentelemetry::global::get_text_map_propagator(|prop| {
        prop.inject_context(ctx, &mut HeaderMapInjector(headers));
    });
}

/// Extract a plain `HashMap<String, String>` of propagation fields from a
/// context. Primarily useful for logging / debugging.
pub fn context_to_map(ctx: &Context) -> HashMap<String, String> {
    struct MapInjector<'a>(&'a mut HashMap<String, String>);
    impl<'a> Injector for MapInjector<'a> {
        fn set(&mut self, key: &str, value: String) {
            self.0.insert(key.to_string(), value);
        }
    }
    let mut out = HashMap::new();
    opentelemetry::global::get_text_map_propagator(|prop| {
        prop.inject_context(ctx, &mut MapInjector(&mut out));
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::trace::TraceContextExt;
    use opentelemetry_sdk::propagation::TraceContextPropagator;

    fn ensure_propagator() {
        // Safe to call multiple times; overwrites the global propagator.
        opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
    }

    #[test]
    fn extract_parses_canned_traceparent() {
        ensure_propagator();
        let mut headers = HeaderMap::new();
        headers.insert(
            "traceparent",
            HeaderValue::from_static(
                "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
            ),
        );
        let ctx = extract_context_from_headers(&headers);
        let span = ctx.span();
        let sc = span.span_context();
        assert!(sc.is_valid(), "extracted span context should be valid");
        assert_eq!(
            format!("{:032x}", u128::from_be_bytes(sc.trace_id().to_bytes())),
            "0af7651916cd43dd8448eb211c80319c"
        );
    }

    #[test]
    fn inject_round_trip_preserves_trace_id() {
        ensure_propagator();
        let mut headers = HeaderMap::new();
        headers.insert(
            "traceparent",
            HeaderValue::from_static(
                "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            ),
        );
        let ctx = extract_context_from_headers(&headers);

        let mut out = HeaderMap::new();
        inject_context_into_headers(&ctx, &mut out);
        let tp = out
            .get("traceparent")
            .expect("traceparent must be injected")
            .to_str()
            .unwrap();
        assert!(tp.contains("4bf92f3577b34da6a3ce929d0e0e4736"));
    }
}
