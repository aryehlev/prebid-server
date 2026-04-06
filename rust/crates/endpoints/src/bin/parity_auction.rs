//! Parity auction bridge binary.
//!
//! Accepts a single fixture-path argument, constructs deterministic test state
//! in-process, calls `create_router(...).oneshot` for `POST /openrtb2/auction`,
//! and prints a JSON object with `status`, `body`, and `warnings`.
//!
//! This binary is invoked by the Go parity oracle via `exec.CommandContext`.

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use serde_json::Value;
use tower::util::ServiceExt as _;

use pbs_endpoints::{AppStateInner, StoredRequestFetcher, create_router};

#[derive(serde::Serialize)]
struct ParityOutput {
    status: u16,
    body: Value,
    warnings: Value,
}

fn build_test_state() -> pbs_endpoints::AppState {
    let metrics = pbs_metrics::PrometheusMetrics::new("parity")
        .expect("failed to create metrics");
    let exchange = pbs_exchange::Exchange::new(HashMap::new());
    Arc::new(AppStateInner {
        exchange,
        version: "parity".to_string(),
        revision: "parity-test".to_string(),
        bidder_info: HashMap::new(),
        bidder_params: HashMap::new(),
        host_cookie: Default::default(),
        status_response: None,
        stored_requests: Arc::new(StoredRequestFetcher::empty()),
        metrics: Arc::new(metrics),
        max_request_size: 256 * 1024,
        gdpr_enabled: false,
        accounts: HashMap::new(),
        bidder_sync_info: HashMap::new(),
        currency_converter: None,
    })
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: parity_auction <fixture-path>");
        std::process::exit(1);
    }

    let fixture_path = &args[1];

    // Read the fixture file and extract the mockBidRequest field.
    let fixture_data = match std::fs::read_to_string(fixture_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("failed to read fixture {}: {}", fixture_path, e);
            std::process::exit(1);
        }
    };

    let fixture: Value = match serde_json::from_str(&fixture_data) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("failed to parse fixture JSON: {}", e);
            std::process::exit(1);
        }
    };

    let bid_request = fixture
        .get("mockBidRequest")
        .unwrap_or(&fixture)
        .clone();

    let body_bytes = serde_json::to_vec(&bid_request).expect("serialize bid request");

    // Build the router and send the request via oneshot.
    let state = build_test_state();
    let router = create_router(state);

    let req = Request::builder()
        .method("POST")
        .uri("/openrtb2/auction")
        .header("content-type", "application/json")
        .body(Body::from(body_bytes))
        .expect("build request");

    let resp = router.oneshot(req).await.expect("oneshot failed");
    let status = resp.status();
    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body");

    let body_value: Value = serde_json::from_slice(&body_bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&body_bytes).to_string()));

    // Extract warnings from ext.warnings or ext.prebid.warnings.
    let warnings = body_value
        .get("ext")
        .and_then(|ext| {
            ext.get("warnings")
                .or_else(|| ext.get("prebid").and_then(|p| p.get("warnings")))
        })
        .cloned()
        .unwrap_or(Value::Null);

    let output = ParityOutput {
        status: status.as_u16(),
        body: body_value,
        warnings,
    };

    println!("{}", serde_json::to_string(&output).expect("serialize output"));
}
