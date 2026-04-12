//! Analytics backend that emits events as structured tracing log lines.

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;

use crate::events::{
    AmpObject, AuctionObject, CookieSyncObject, NotificationEvent, SetUidObject, VideoObject,
};
use crate::module::PbsAnalyticsModule;

/// Analytics backend that logs every event as JSON via `tracing::info!`.
#[derive(Debug, Default, Clone, Copy)]
pub struct StdoutAnalyticsBackend;

impl StdoutAnalyticsBackend {
    pub fn new() -> Self {
        Self
    }
}

fn log_json<T: Serialize>(kind: &'static str, evt: &T) {
    match serde_json::to_value(evt) {
        Ok(value) => tracing::info!(kind, event = %value, "analytics event"),
        Err(e) => tracing::warn!(kind, error = %e, "failed to serialize analytics event"),
    }
}

// Explicit generic bound on Value to silence an unused-import warning when
// readers look for it — the helper above does all the work.
#[allow(dead_code)]
fn _keep_value_in_scope(_v: Value) {}

#[async_trait]
impl PbsAnalyticsModule for StdoutAnalyticsBackend {
    fn name(&self) -> &str {
        "stdout"
    }
    async fn log_auction_object(&self, evt: &AuctionObject) {
        log_json("auction", evt);
    }
    async fn log_video_object(&self, evt: &VideoObject) {
        log_json("video", evt);
    }
    async fn log_cookie_sync_object(&self, evt: &CookieSyncObject) {
        log_json("cookie_sync", evt);
    }
    async fn log_set_uid_object(&self, evt: &SetUidObject) {
        log_json("set_uid", evt);
    }
    async fn log_amp_object(&self, evt: &AmpObject) {
        log_json("amp", evt);
    }
    async fn log_notification_event(&self, evt: &NotificationEvent) {
        log_json("notification", evt);
    }
    async fn shutdown(&self) {}
}
