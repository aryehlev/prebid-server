use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

/// Shared application state threaded through axum handlers.
pub type AppState = Arc<AppStateInner>;

pub struct AppStateInner {
    pub exchange: pbs_exchange::Exchange,
    pub version: String,
}

/// Response body for GET /status.
#[derive(serde::Serialize)]
pub struct StatusResponse {
    pub ok: &'static str,
}

/// GET /status — liveness probe.
pub async fn status_handler() -> Json<StatusResponse> {
    Json(StatusResponse { ok: "ok" })
}

/// GET /version — returns the binary version.
pub async fn version_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "revision": state.version }))
}

/// POST /openrtb2/auction — runs the full auction and returns a BidResponse.
pub async fn auction_handler(
    State(state): State<AppState>,
    Json(bid_request): Json<openrtb::BidRequest>,
) -> Response {
    let auction_req = pbs_exchange::AuctionRequest {
        bid_request,
        account: None,
        user_syncs: None,
        start_time: std::time::Instant::now(),
    };

    match state.exchange.hold_auction(auction_req).await {
        Ok(auction_response) => {
            (StatusCode::OK, Json(auction_response.bid_response)).into_response()
        }
        Err(e) => {
            tracing::error!("Auction error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

/// GET /info/bidders — lists all known bidder names.
pub async fn info_bidders_handler() -> Json<serde_json::Value> {
    // Return an empty list; populated once adapters are registered.
    let bidders: Vec<String> = Vec::new();
    Json(serde_json::json!(bidders))
}

/// Builds the axum Router with all routes wired up.
pub fn create_router(state: AppState) -> axum::Router {
    axum::Router::new()
        .route("/status", axum::routing::get(status_handler))
        .route("/version", axum::routing::get(version_handler))
        .route("/openrtb2/auction", axum::routing::post(auction_handler))
        .route("/info/bidders", axum::routing::get(info_bidders_handler))
        .with_state(state)
}
