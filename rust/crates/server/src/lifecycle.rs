//! Graceful-shutdown helpers.
//!
//! The Go port of prebid-server shuts down on either `SIGINT` (Ctrl-C) or
//! `SIGTERM`.  This module exposes a single [`shutdown_signal`] future that
//! resolves as soon as either signal is received.

use tokio::signal;

/// Future that completes the first time either Ctrl-C (`SIGINT`) or `SIGTERM`
/// is delivered to the process.
///
/// Pass this to [`axum::serve`]'s `with_graceful_shutdown` to stop accepting
/// new connections and drain in-flight requests cleanly.
pub async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = signal::ctrl_c().await {
            tracing::error!("failed to install Ctrl+C handler: {e}");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(e) => {
                tracing::error!("failed to install SIGTERM handler: {e}");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("received Ctrl+C, initiating graceful shutdown");
        }
        _ = terminate => {
            tracing::info!("received SIGTERM, initiating graceful shutdown");
        }
    }
}
