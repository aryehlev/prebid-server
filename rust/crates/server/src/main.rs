use std::sync::Arc;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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

    let state = Arc::new(pbs_endpoints::AppStateInner {
        exchange,
        version: env!("CARGO_PKG_VERSION").to_string(),
        revision: std::env::var("PBS_REVISION").unwrap_or_else(|_| "unknown".to_string()),
        bidder_info: std::collections::HashMap::new(),
        bidder_params: std::collections::HashMap::new(),
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
