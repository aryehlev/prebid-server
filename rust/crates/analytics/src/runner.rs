//! Fan-out runner that dispatches a single event to every registered
//! analytics module concurrently.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::time::timeout;

use crate::events::{
    AmpObject, AuctionObject, CookieSyncObject, NotificationEvent, SetUidObject, VideoObject,
};
use crate::module::PbsAnalyticsModule;

/// Default per-module dispatch timeout. Mirrors the 1s budget used by the Go
/// runner so a single slow backend cannot stall auction logging.
pub const DEFAULT_MODULE_TIMEOUT: Duration = Duration::from_secs(1);

/// Runs a collection of analytics modules, fanning each event out to all of
/// them concurrently with a bounded per-module timeout.
#[derive(Clone)]
pub struct AnalyticsRunner {
    modules: Vec<Arc<dyn PbsAnalyticsModule + Send + Sync>>,
    module_timeout: Duration,
}

impl Default for AnalyticsRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalyticsRunner {
    /// Construct a runner with no modules and the default per-module timeout.
    pub fn new() -> Self {
        Self {
            modules: Vec::new(),
            module_timeout: DEFAULT_MODULE_TIMEOUT,
        }
    }

    /// Construct a runner from an existing module list.
    pub fn with_modules(modules: Vec<Arc<dyn PbsAnalyticsModule + Send + Sync>>) -> Self {
        Self {
            modules,
            module_timeout: DEFAULT_MODULE_TIMEOUT,
        }
    }

    /// Override the per-module dispatch timeout.
    pub fn with_timeout(mut self, module_timeout: Duration) -> Self {
        self.module_timeout = module_timeout;
        self
    }

    /// Register an additional module.
    pub fn push(&mut self, module: Arc<dyn PbsAnalyticsModule + Send + Sync>) {
        self.modules.push(module);
    }

    /// Number of registered modules.
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    /// Whether no modules are registered.
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }
}

/// Internal helper: spawn `f(module)` per module with a timeout and swallow
/// errors/panics behind a `tracing::warn!`.
macro_rules! fanout {
    ($self:ident, $evt:ident, $method:ident) => {{
        let timeout_dur = $self.module_timeout;
        let mut handles = Vec::with_capacity($self.modules.len());
        for module in &$self.modules {
            let module = Arc::clone(module);
            let evt = $evt.clone();
            let handle = tokio::spawn(async move {
                let name = module.name().to_string();
                match timeout(timeout_dur, async move { module.$method(&evt).await }).await {
                    Ok(()) => {}
                    Err(_) => {
                        tracing::warn!(
                            module = %name,
                            method = stringify!($method),
                            "analytics module timed out"
                        );
                    }
                }
            });
            handles.push(handle);
        }
        for h in handles {
            if let Err(e) = h.await {
                tracing::warn!(
                    error = %e,
                    method = stringify!($method),
                    "analytics module task panicked"
                );
            }
        }
    }};
}

#[async_trait]
impl PbsAnalyticsModule for AnalyticsRunner {
    fn name(&self) -> &str {
        "runner"
    }

    async fn log_auction_object(&self, evt: &AuctionObject) {
        fanout!(self, evt, log_auction_object);
    }

    async fn log_video_object(&self, evt: &VideoObject) {
        fanout!(self, evt, log_video_object);
    }

    async fn log_cookie_sync_object(&self, evt: &CookieSyncObject) {
        fanout!(self, evt, log_cookie_sync_object);
    }

    async fn log_set_uid_object(&self, evt: &SetUidObject) {
        fanout!(self, evt, log_set_uid_object);
    }

    async fn log_amp_object(&self, evt: &AmpObject) {
        fanout!(self, evt, log_amp_object);
    }

    async fn log_notification_event(&self, evt: &NotificationEvent) {
        fanout!(self, evt, log_notification_event);
    }

    async fn shutdown(&self) {
        let timeout_dur = self.module_timeout;
        let mut handles = Vec::with_capacity(self.modules.len());
        for module in &self.modules {
            let module = Arc::clone(module);
            handles.push(tokio::spawn(async move {
                let name = module.name().to_string();
                if timeout(timeout_dur, async move { module.shutdown().await })
                    .await
                    .is_err()
                {
                    tracing::warn!(module = %name, "analytics module shutdown timed out");
                }
            }));
        }
        for h in handles {
            if let Err(e) = h.await {
                tracing::warn!(error = %e, "analytics module shutdown task panicked");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::AuctionObject;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Counting {
        hits: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl PbsAnalyticsModule for Counting {
        async fn log_auction_object(&self, _evt: &AuctionObject) {
            self.hits.fetch_add(1, Ordering::SeqCst);
        }
        async fn log_video_object(&self, _evt: &VideoObject) {}
        async fn log_cookie_sync_object(&self, _evt: &CookieSyncObject) {}
        async fn log_set_uid_object(&self, _evt: &SetUidObject) {}
        async fn log_amp_object(&self, _evt: &AmpObject) {}
        async fn log_notification_event(&self, _evt: &NotificationEvent) {}
        async fn shutdown(&self) {}
    }

    #[tokio::test]
    async fn fanout_dispatches_to_every_module() {
        let a = Arc::new(AtomicUsize::new(0));
        let b = Arc::new(AtomicUsize::new(0));
        let runner = AnalyticsRunner::with_modules(vec![
            Arc::new(Counting { hits: a.clone() }),
            Arc::new(Counting { hits: b.clone() }),
        ]);

        let evt = AuctionObject::new("r", "acct", 200, serde_json::json!({}));
        runner.log_auction_object(&evt).await;
        assert_eq!(a.load(Ordering::SeqCst), 1);
        assert_eq!(b.load(Ordering::SeqCst), 1);
    }
}
