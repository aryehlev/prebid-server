//! No-op analytics backend, useful as a default and in tests.

use async_trait::async_trait;

use crate::events::{
    AmpObject, AuctionObject, CookieSyncObject, NotificationEvent, SetUidObject, VideoObject,
};
use crate::module::PbsAnalyticsModule;

/// Analytics backend that discards every event.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullAnalyticsBackend;

impl NullAnalyticsBackend {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PbsAnalyticsModule for NullAnalyticsBackend {
    fn name(&self) -> &str {
        "null"
    }
    async fn log_auction_object(&self, _evt: &AuctionObject) {}
    async fn log_video_object(&self, _evt: &VideoObject) {}
    async fn log_cookie_sync_object(&self, _evt: &CookieSyncObject) {}
    async fn log_set_uid_object(&self, _evt: &SetUidObject) {}
    async fn log_amp_object(&self, _evt: &AmpObject) {}
    async fn log_notification_event(&self, _evt: &NotificationEvent) {}
    async fn shutdown(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn null_backend_is_a_noop_trait_object() {
        let module: Arc<dyn PbsAnalyticsModule + Send + Sync> =
            Arc::new(NullAnalyticsBackend::new());
        let evt = AuctionObject::new("r", "a", 200, serde_json::json!({}));
        module.log_auction_object(&evt).await;
        module.shutdown().await;
        assert_eq!(module.name(), "null");
    }
}
