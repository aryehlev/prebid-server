//! HTTP batching analytics backend loosely modelled on the Go `pubstack`
//! module.
//!
//! Events are serialized into an in-memory buffer and flushed to an HTTP
//! endpoint either when the buffer reaches `batch_size` entries or when the
//! `flush_interval` elapses.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::events::{
    AmpObject, AuctionObject, CookieSyncObject, NotificationEvent, SetUidObject, VideoObject,
};
use crate::module::PbsAnalyticsModule;

/// Configuration for [`PubstackAnalyticsBackend`].
#[derive(Debug, Clone)]
pub struct PubstackConfig {
    /// HTTP endpoint events are POSTed to as JSON.
    pub endpoint: String,
    /// Pubstack scope identifier, forwarded as the `x-scope-id` header.
    pub scope_id: String,
    /// Maximum number of events in a single batch before a forced flush.
    pub batch_size: usize,
    /// Maximum time a batch may sit in memory before a forced flush.
    pub flush_interval: Duration,
}

impl Default for PubstackConfig {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            scope_id: String::new(),
            batch_size: 100,
            flush_interval: Duration::from_secs(60),
        }
    }
}

/// HTTP batching analytics backend.
pub struct PubstackAnalyticsBackend {
    endpoint: String,
    scope_id: String,
    buffer: Arc<Mutex<Vec<Value>>>,
    batch_size: usize,
    flush_interval: Duration,
    client: reqwest::Client,
    flusher: Mutex<Option<JoinHandle<()>>>,
}

impl PubstackAnalyticsBackend {
    /// Construct a new backend from an already built `reqwest::Client`.
    pub fn with_client(config: PubstackConfig, client: reqwest::Client) -> Arc<Self> {
        Arc::new(Self {
            endpoint: config.endpoint,
            scope_id: config.scope_id,
            buffer: Arc::new(Mutex::new(Vec::new())),
            batch_size: config.batch_size,
            flush_interval: config.flush_interval,
            client,
            flusher: Mutex::new(None),
        })
    }

    /// Construct a new backend with a default `reqwest::Client`.
    pub fn new(config: PubstackConfig) -> Arc<Self> {
        Self::with_client(config, reqwest::Client::new())
    }

    /// Configured endpoint.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Configured scope id.
    pub fn scope_id(&self) -> &str {
        &self.scope_id
    }

    /// Spawn the background flush loop. Safe to call once after construction.
    pub async fn start(self: &Arc<Self>) {
        let me = Arc::clone(self);
        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(me.flush_interval);
            ticker.tick().await; // drop the immediate first tick
            loop {
                ticker.tick().await;
                me.flush().await;
            }
        });
        let mut guard = self.flusher.lock().await;
        *guard = Some(handle);
    }

    async fn push<T: Serialize>(&self, kind: &'static str, evt: &T) {
        let value = match serde_json::to_value(evt) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(kind, error = %e, "failed to serialize analytics event");
                return;
            }
        };
        let should_flush = {
            let mut buf = self.buffer.lock().await;
            buf.push(value);
            buf.len() >= self.batch_size
        };
        if should_flush {
            self.flush().await;
        }
    }

    /// Drain the buffer and POST it to the configured endpoint.
    pub async fn flush(&self) {
        let batch: Vec<Value> = {
            let mut buf = self.buffer.lock().await;
            if buf.is_empty() {
                return;
            }
            std::mem::take(&mut *buf)
        };

        if self.endpoint.is_empty() {
            tracing::debug!(
                batch_len = batch.len(),
                "pubstack backend has no endpoint configured; dropping batch"
            );
            return;
        }

        let result = self
            .client
            .post(&self.endpoint)
            .header("x-scope-id", &self.scope_id)
            .json(&batch)
            .send()
            .await;

        match result {
            Ok(resp) if resp.status().is_success() => {
                tracing::debug!(batch_len = batch.len(), "pubstack flushed batch");
            }
            Ok(resp) => {
                tracing::warn!(
                    status = %resp.status(),
                    batch_len = batch.len(),
                    "pubstack flush returned non-success status"
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, batch_len = batch.len(), "pubstack flush failed");
            }
        }
    }
}

#[async_trait]
impl PbsAnalyticsModule for PubstackAnalyticsBackend {
    fn name(&self) -> &str {
        "pubstack"
    }
    async fn log_auction_object(&self, evt: &AuctionObject) {
        self.push("auction", evt).await;
    }
    async fn log_video_object(&self, evt: &VideoObject) {
        self.push("video", evt).await;
    }
    async fn log_cookie_sync_object(&self, evt: &CookieSyncObject) {
        self.push("cookie_sync", evt).await;
    }
    async fn log_set_uid_object(&self, evt: &SetUidObject) {
        self.push("set_uid", evt).await;
    }
    async fn log_amp_object(&self, evt: &AmpObject) {
        self.push("amp", evt).await;
    }
    async fn log_notification_event(&self, evt: &NotificationEvent) {
        self.push("notification", evt).await;
    }
    async fn shutdown(&self) {
        // Cancel the background flusher if any and drain the buffer.
        let handle = { self.flusher.lock().await.take() };
        if let Some(handle) = handle {
            handle.abort();
        }
        self.flush().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn push_accumulates_without_network() {
        let backend = PubstackAnalyticsBackend::new(PubstackConfig {
            endpoint: String::new(),
            scope_id: "scope".into(),
            batch_size: 10,
            flush_interval: Duration::from_secs(60),
        });

        let evt = AuctionObject::new("r", "a", 200, serde_json::json!({}));
        backend.log_auction_object(&evt).await;
        backend.log_auction_object(&evt).await;

        let len = backend.buffer.lock().await.len();
        assert_eq!(len, 2);
    }
}
