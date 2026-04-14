//! Entry point for the `prebid-server` binary.
//!
//! This is intentionally thin: everything interesting lives in the library
//! modules (`config`, `router`, `lifecycle`).  The binary's only job is to
//! bootstrap tracing, load configuration, build the router, and serve it
//! under a graceful-shutdown future.

use std::process::ExitCode;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use std::sync::Arc;

use server::{
    config::ServerConfig,
    lifecycle::shutdown_signal,
    router::build_router,
    state::AppState,
};

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();

    let cfg = match ServerConfig::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("failed to load server config: {e}");
            return ExitCode::from(2);
        }
    };

    let addr = cfg.socket_addr();
    tracing::info!(
        host = %cfg.host,
        port = cfg.port,
        read_timeout_secs = cfg.read_timeout.as_secs(),
        write_timeout_secs = cfg.write_timeout.as_secs(),
        "starting prebid-server"
    );

    // Dev wiring: no-op analytics, empty chooser, fresh Prometheus registry,
    // default configuration. Replaced with real dependencies once the full
    // bootstrap pipeline lands.
    let state = Arc::new(AppState::dev());
    let app = build_router(&cfg, state);

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("failed to bind {addr}: {e}");
            return ExitCode::from(1);
        }
    };

    tracing::info!("listening on {addr}");

    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        tracing::error!("server error: {e}");
        return ExitCode::from(1);
    }

    tracing::info!("shutdown complete");
    ExitCode::SUCCESS
}

fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}
