use std::sync::Arc;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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

    tracing::info!("Starting prebid-server (Rust port)");

    // Build the exchange with no adapters initially.
    // Adapter registration will be wired in here once adapter crates are complete.
    let exchange = pbs_exchange::Exchange::new(std::collections::HashMap::new());

    let static_dir = std::env::var("PBS_STATIC_DIR")
        .unwrap_or_else(|_| "/home/user/prebid-server/static".to_string());

    let state = Arc::new(pbs_endpoints::AppStateInner {
        exchange,
        version: env!("CARGO_PKG_VERSION").to_string(),
        revision: std::env::var("PBS_REVISION").unwrap_or_else(|_| "unknown".to_string()),
        bidder_info: load_bidder_info(&static_dir),
        bidder_params: load_bidder_params(&static_dir),
        host_cookie: pbs_endpoints::HostCookieConfig::default(),
        status_response: None,
    });

    let app = pbs_endpoints::create_router(state);

    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8000);

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
