use std::sync::Arc;

use tokio::signal;
use axum::{
    body::Body,
    extract::State,
    http::{Request, Response, StatusCode},
    middleware::{self, Next},
};
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    timeout::TimeoutLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn load_bidder_sync_info(static_dir: &str) -> std::collections::HashMap<String, pbs_endpoints::BidderSyncInfo> {
    let mut map = std::collections::HashMap::new();
    let dir = format!("{}/bidder-info", static_dir);
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("Could not read bidder-info dir {}: {}", dir, e);
            return map;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") { continue; }
        let name = match path.file_stem().and_then(|s| s.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        // Parse the userSync section: look for redirect/iframe url lines
        // YAML structure under userSync:
        //   userSync:
        //     redirect:
        //       url: "..."
        //     iframe:
        //       url: "..."
        let mut in_user_sync = false;
        let mut in_redirect = false;
        let mut in_iframe = false;
        let mut redirect_url: Option<String> = None;
        let mut iframe_url: Option<String> = None;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') { continue; }
            // Track section depth by leading spaces
            let indent = line.len() - line.trim_start().len();
            if indent == 0 {
                in_user_sync = trimmed.starts_with("userSync:") || trimmed == "userSync:";
                in_redirect = false;
                in_iframe = false;
            } else if in_user_sync && indent >= 2 {
                if trimmed.starts_with("redirect:") || trimmed == "redirect:" {
                    in_redirect = true;
                    in_iframe = false;
                } else if trimmed.starts_with("iframe:") || trimmed == "iframe:" {
                    in_iframe = true;
                    in_redirect = false;
                } else if trimmed.starts_with("url:") && indent >= 4 {
                    let url_val = trimmed["url:".len()..].trim().trim_matches('"').to_string();
                    if !url_val.is_empty() {
                        if in_redirect && redirect_url.is_none() {
                            redirect_url = Some(url_val);
                        } else if in_iframe && iframe_url.is_none() {
                            iframe_url = Some(url_val);
                        }
                    }
                }
            }
        }
        if redirect_url.is_some() || iframe_url.is_some() {
            map.insert(name, pbs_endpoints::BidderSyncInfo { iframe_url, redirect_url });
        }
    }
    tracing::info!("Loaded usersync info for {} bidders", map.len());
    map
}

fn load_bidder_info(static_dir: &str) -> std::collections::HashMap<String, serde_json::Value> {
    let mut map = std::collections::HashMap::new();
    let dir = format!("{}/bidder-info", static_dir);
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("Could not read bidder-info dir {}: {}", dir, e);
            return map;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") { continue; }
        let name = match path.file_stem().and_then(|s| s.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        // Parse YAML key:value lines into a JSON object
        let mut obj = serde_json::Map::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('-') { continue; }
            if let Some(pos) = line.find(':') {
                let key = line[..pos].trim().to_string();
                let val = line[pos+1..].trim().trim_matches('"').to_string();
                if !key.is_empty() && !val.is_empty() {
                    obj.insert(key, serde_json::Value::String(val));
                }
            }
        }
        obj.insert("enabled".to_string(), serde_json::Value::Bool(true));
        map.insert(name, serde_json::Value::Object(obj));
    }
    tracing::info!("Loaded {} bidder info entries", map.len());
    map
}

fn load_bidder_params(static_dir: &str) -> std::collections::HashMap<String, serde_json::Value> {
    let mut map = std::collections::HashMap::new();
    let dir = format!("{}/bidder-params", static_dir);
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("Could not read bidder-params dir {}: {}", dir, e);
            return map;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") { continue; }
        let name = match path.file_stem().and_then(|s| s.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            map.insert(name, json);
        }
    }
    tracing::info!("Loaded {} bidder param schemas", map.len());
    map
}

fn read_endpoint_compression(static_dir: &str, bidder_name: &str) -> Option<String> {
    let path = format!("{}/bidder-info/{}.yaml", static_dir, bidder_name);
    let content = std::fs::read_to_string(&path).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("endpointCompression:") {
            if let Some(val) = trimmed.splitn(2, ':').nth(1) {
                let val = val.trim().trim_matches('"').to_string();
                if !val.is_empty() {
                    return Some(val);
                }
            }
        }
    }
    None
}

/// Generate a unique request ID.
/// Uses `uuid::Uuid::new_v4()` which is available in the workspace.
fn generate_request_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Axum middleware that echoes or injects an `X-Request-ID` header on every response.
async fn request_id_middleware(
    req: Request<Body>,
    next: Next,
) -> Response<Body> {
    // Extract existing request ID from the incoming request headers.
    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(generate_request_id);

    let mut response = next.run(req).await;

    // Attach the ID to the response.
    if let Ok(val) = axum::http::HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", val);
    }

    response
}

/// Axum middleware that logs every request: method, path, response status, and
/// wall-clock duration in milliseconds.
async fn request_logging_middleware(
    req: Request<Body>,
    next: Next,
) -> Response<Body> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let start = std::time::Instant::now();

    let response = next.run(req).await;

    let duration = start.elapsed();
    let status = response.status().as_u16();
    tracing::info!(
        method = %method,
        path = %path,
        status = status,
        duration_ms = duration.as_millis() as u64,
        "request completed"
    );

    response
}

/// Axum middleware that rejects requests whose `Content-Length` exceeds a
/// configured maximum body size.  The limit is carried via Axum `State`.
///
/// Only requests that declare a `Content-Length` header are checked; chunked
/// (streaming) bodies without the header are allowed through so that
/// downstream handlers can enforce their own limits.
async fn request_size_limit_middleware(
    State(max_size): State<usize>,
    req: Request<Body>,
    next: Next,
) -> Result<Response<Body>, StatusCode> {
    if let Some(content_length) = req
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
    {
        if content_length > max_size {
            tracing::warn!(
                content_length = content_length,
                max_size = max_size,
                path = %req.uri().path(),
                "request body too large"
            );
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
    }

    Ok(next.run(req).await)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize structured tracing from RUST_LOG env var, defaulting to "info".
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    println!("Prebid Server starting...");

    // Load configuration (YAML file optional; env vars always applied on top).
    let config_file = std::env::var("PBS_CONFIG_FILE").ok();
    let cfg = pbs_config::Configuration::load(config_file.as_deref())
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to load config, using defaults: {}", e);
            pbs_config::Configuration::default()
        });
    tracing::info!(
        "Config: port={} max_request_size={} gdpr_enabled={}",
        cfg.port, cfg.max_request_size, cfg.gdpr_enabled
    );

    // PBS_STATIC_DIR env var takes precedence over config file value (already handled
    // by apply_env_overrides), but the server historically defaulted to the static
    // dir under the repo root.  Keep backward compatibility.
    let static_dir = if cfg.static_dir == "./static" {
        std::env::var("PBS_STATIC_DIR")
            .unwrap_or_else(|_| "/home/user/prebid-server/static".to_string())
    } else {
        cfg.static_dir.clone()
    };

    let raw_adapters = pbs_adapters::registry::build_adapter_map();
    let adapter_count = raw_adapters.len();
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();
    let adapters: std::collections::HashMap<String, pbs_exchange::AdaptedBidder> = raw_adapters
        .into_iter()
        .map(|(name, bidder)| {
            let endpoint_compression = read_endpoint_compression(&static_dir, &name);
            let adapted = pbs_exchange::AdaptedBidder {
                bidder: std::sync::Arc::from(bidder),
                http_client: http_client.clone(),
                endpoint: String::new(),
                endpoint_compression,
            };
            (name, adapted)
        })
        .collect();
    let metrics = Arc::new(
        pbs_metrics::PrometheusMetrics::new("prebid").expect("metrics init"),
    );

    let mut exchange = pbs_exchange::Exchange::new(adapters);
    exchange.metrics = Some(metrics.clone() as Arc<dyn pbs_metrics::MetricsEngine>);

    // Load alias bidder map from config.
    exchange.aliases = cfg.aliases.clone();
    if !exchange.aliases.is_empty() {
        tracing::info!("Loaded {} bidder aliases from config", exchange.aliases.len());
    }

    // Load host SChain node from config.
    if let Some(node) = &cfg.schain_node {
        exchange.schain_node = Some(openrtb::SupplyChainNode {
            asi: node.asi.clone(),
            sid: node.sid.clone(),
            rid: node.rid.clone(),
            name: node.name.clone(),
            domain: node.domain.clone(),
            hp: node.hp,
            ext: None,
        });
        tracing::info!("Loaded host SChain node: asi={}", node.asi);
    }

    let stored_requests_dir = if cfg.stored_requests_dir.is_empty() || cfg.stored_requests_dir == "./stored_requests" {
        std::env::var("PBS_STORED_REQUESTS_DIR")
            .unwrap_or_else(|_| "./stored_requests".to_string())
    } else {
        cfg.stored_requests_dir.clone()
    };
    let stored_requests = Arc::new(
        pbs_endpoints::StoredRequestFetcher::from_directory(&stored_requests_dir)
    );
    // Wire stored auction responses into the exchange so that requests with
    // req.ext.prebid.storedauctionresponse.id skip bidder calls.
    exchange.stored_responses = Some(stored_requests.clone() as Arc<dyn pbs_exchange::StoredResponseFetcher>);

    let bidder_info = load_bidder_info(&static_dir);
    let bidder_sync_info = load_bidder_sync_info(&static_dir);
    let sync_info_count = bidder_sync_info.len();

    let state = Arc::new(pbs_endpoints::AppStateInner {
        exchange,
        version: env!("CARGO_PKG_VERSION").to_string(),
        revision: std::env::var("PBS_REVISION").unwrap_or_else(|_| "unknown".to_string()),
        bidder_info,
        bidder_params: load_bidder_params(&static_dir),
        bidder_sync_info,
        host_cookie: pbs_endpoints::HostCookieConfig::default(),
        status_response: None,
        stored_requests,
        metrics,
        max_request_size: cfg.max_request_size,
        gdpr_enabled: cfg.gdpr_enabled,
        accounts: cfg.accounts.clone(),
        currency_converter: None,
    });

    // Build CORS layer: permissive for all origins.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
        ]);

    let max_request_size = cfg.max_request_size as usize;

    let app = pbs_endpoints::create_router(state)
        // Request ID must run first so later layers see the header on responses.
        .layer(middleware::from_fn(request_id_middleware))
        // Log every request (method, path, status, duration).
        .layer(middleware::from_fn(request_logging_middleware))
        // Reject requests whose Content-Length exceeds the configured maximum.
        .layer(middleware::from_fn_with_state(
            max_request_size,
            request_size_limit_middleware,
        ))
        // Gzip-compress responses when the client sends Accept-Encoding: gzip.
        .layer(CompressionLayer::new())
        // Global 30-second request timeout.
        .layer(TimeoutLayer::new(std::time::Duration::from_secs(30)))
        // Permissive CORS headers.
        .layer(cors);

    // PORT env var (legacy) takes precedence, then PBS_PORT (via config), then config.port.
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(cfg.port);

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    println!("  - Adapters registered: {}", adapter_count);
    println!("  - Bidder sync info loaded: {} bidders", sync_info_count);
    println!("  - GDPR enabled: {}", cfg.gdpr_enabled);
    println!("  - Max request size: {}", cfg.max_request_size);
    println!("  - Listening on: {}", addr);
    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => { tracing::info!("Received Ctrl+C, shutting down"); },
        _ = terminate => { tracing::info!("Received SIGTERM, shutting down"); },
    }
}
