//! Analytics bridge.
//!
//! Small helpers that build typed `analytics::events::*` values out of the
//! opaque `serde_json::Value` payloads that the exchange endpoints already
//! produce, and dispatch them through an [`analytics::AnalyticsRunner`].
//!
//! These functions are intentionally `async` so callers can `.await` them
//! inline. They do not panic on any failure: the underlying runner swallows
//! per-module errors and logs them.

use analytics::events::{
    AmpObject, AuctionObject, CookieSyncObject, NotificationEvent, SetUidObject,
};
use analytics::module::PbsAnalyticsModule;
use analytics::AnalyticsRunner;
use serde_json::{json, Value};

/// Build the combined request/response payload that gets stapled onto an
/// analytics event.
fn build_payload(req: &Value, resp: &Value) -> Value {
    json!({
        "request": req,
        "response": resp,
    })
}

/// Emit an `/openrtb2/auction` analytics event.
pub async fn emit_auction_event(
    runner: &AnalyticsRunner,
    req: &Value,
    resp: &Value,
    status: u16,
    request_id: &str,
    account_id: &str,
) {
    let evt = AuctionObject::new(request_id, account_id, status, build_payload(req, resp));
    runner.log_auction_object(&evt).await;
}

/// Emit an `/openrtb2/amp` analytics event.
pub async fn emit_amp_event(
    runner: &AnalyticsRunner,
    req: &Value,
    resp: &Value,
    status: u16,
    request_id: &str,
    account_id: &str,
) {
    let evt = AmpObject::new(request_id, account_id, status, build_payload(req, resp));
    runner.log_amp_object(&evt).await;
}

/// Emit a `/cookie_sync` analytics event.
pub async fn emit_cookie_sync_event(
    runner: &AnalyticsRunner,
    req: &Value,
    resp: &Value,
    status: u16,
    request_id: &str,
    account_id: &str,
) {
    let evt =
        CookieSyncObject::new(request_id, account_id, status, build_payload(req, resp));
    runner.log_cookie_sync_object(&evt).await;
}

/// Emit a `/setuid` analytics event.
pub async fn emit_setuid_event(
    runner: &AnalyticsRunner,
    req: &Value,
    resp: &Value,
    status: u16,
    request_id: &str,
    account_id: &str,
) {
    let evt = SetUidObject::new(request_id, account_id, status, build_payload(req, resp));
    runner.log_set_uid_object(&evt).await;
}

/// Emit an `/event` notification analytics event.
pub async fn emit_notification_event(
    runner: &AnalyticsRunner,
    req: &Value,
    resp: &Value,
    status: u16,
    request_id: &str,
    account_id: &str,
) {
    let evt =
        NotificationEvent::new(request_id, account_id, status, build_payload(req, resp));
    runner.log_notification_event(&evt).await;
}
