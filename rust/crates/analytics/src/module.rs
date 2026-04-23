//! The [`PbsAnalyticsModule`] trait that all analytics backends must
//! implement.

use async_trait::async_trait;

use crate::events::{
    AmpObject, AuctionObject, CookieSyncObject, NotificationEvent, SetUidObject, VideoObject,
};

/// Trait implemented by every analytics backend.
///
/// Methods are async and take shared references to events so that a single
/// event can be fanned out to many modules concurrently. Implementations are
/// expected to be cheap to call and non-panicking; errors should be surfaced
/// via `tracing` rather than returned, so the trait intentionally returns
/// `()`.
#[async_trait]
pub trait PbsAnalyticsModule: Send + Sync {
    /// Human readable module name, used in log messages.
    fn name(&self) -> &str {
        "unnamed"
    }

    /// Log a successful or failed `/openrtb2/auction` transaction.
    async fn log_auction_object(&self, evt: &AuctionObject);

    /// Log a successful or failed `/openrtb2/video` transaction.
    async fn log_video_object(&self, evt: &VideoObject);

    /// Log a successful or failed `/cookie_sync` transaction.
    async fn log_cookie_sync_object(&self, evt: &CookieSyncObject);

    /// Log a successful or failed `/setuid` transaction.
    async fn log_set_uid_object(&self, evt: &SetUidObject);

    /// Log a successful or failed `/openrtb2/amp` transaction.
    async fn log_amp_object(&self, evt: &AmpObject);

    /// Log a `/event` notification.
    async fn log_notification_event(&self, evt: &NotificationEvent);

    /// Gracefully shut the module down, flushing any buffered state.
    async fn shutdown(&self);
}
