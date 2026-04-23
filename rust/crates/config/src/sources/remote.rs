//! Periodically refetch a JSON blob from a remote URL and broadcast it.
//!
//! Consumers clone the watch receiver via [`RemoteConfigSource::subscribe`]
//! and observe changes via `tokio::sync::watch::Receiver::changed`.
//!
//! The source is deliberately generic over the payload type `T: DeserializeOwned
//! + Clone + Send + Sync` so the same machinery can be reused for bidder info,
//! account overrides, floor rules, etc.

use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use thiserror::Error;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::{debug, warn};

/// Errors returned when constructing / fetching a [`RemoteConfigSource`].
#[derive(Debug, Error)]
pub enum RemoteSourceError {
    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("json deserialization failed: {0}")]
    Json(#[from] serde_json::Error),
}

/// A remote HTTP source that periodically refetches a JSON payload.
pub struct RemoteConfigSource<T: Clone + Send + Sync + 'static> {
    url: Arc<String>,
    client: reqwest::Client,
    tx: watch::Sender<T>,
    rx: watch::Receiver<T>,
    refresh_every: Duration,
    task: Option<JoinHandle<()>>,
}

impl<T> RemoteConfigSource<T>
where
    T: Clone + Send + Sync + DeserializeOwned + 'static,
{
    /// Build a source with an initial value. Call [`Self::start`] afterwards
    /// to kick off the refresh loop. The returned receiver always yields the
    /// `initial` value first.
    pub fn new(url: impl Into<String>, refresh_every: Duration, initial: T) -> Self {
        let (tx, rx) = watch::channel(initial);
        Self {
            url: Arc::new(url.into()),
            client: reqwest::Client::new(),
            tx,
            rx,
            refresh_every,
            task: None,
        }
    }

    /// Clone-able watch receiver for downstream consumers.
    pub fn subscribe(&self) -> watch::Receiver<T> {
        self.rx.clone()
    }

    /// URL being polled.
    pub fn url(&self) -> &str {
        self.url.as_str()
    }

    /// Current cached value (a convenience over the watch receiver).
    pub fn current(&self) -> T {
        self.rx.borrow().clone()
    }

    /// Perform a single fetch + broadcast cycle. Exposed so callers can
    /// prime the channel synchronously before starting the loop.
    pub async fn refresh_once(&self) -> Result<(), RemoteSourceError> {
        let bytes = self
            .client
            .get(self.url.as_str())
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let parsed: T = serde_json::from_slice(&bytes)?;
        // `send` only errors if all receivers are dropped — we hold one, so
        // it cannot fail here in practice.
        let _ = self.tx.send(parsed);
        Ok(())
    }

    /// Start the background refresh loop. Panics if called twice.
    pub fn start(&mut self) {
        assert!(self.task.is_none(), "RemoteConfigSource already started");
        let url = self.url.clone();
        let client = self.client.clone();
        let tx = self.tx.clone();
        let every = self.refresh_every;

        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(every);
            // Skip the immediate first tick so we honour the cadence.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                match fetch_once::<T>(&client, url.as_str()).await {
                    Ok(v) => {
                        debug!(url = %url, "remote config refreshed");
                        let _ = tx.send(v);
                    }
                    Err(e) => {
                        warn!(url = %url, error = %e, "remote config refresh failed");
                    }
                }
            }
        });
        self.task = Some(handle);
    }

    /// Stop the background refresh loop (if any).
    pub fn stop(&mut self) {
        if let Some(h) = self.task.take() {
            h.abort();
        }
    }
}

impl<T: Clone + Send + Sync + 'static> Drop for RemoteConfigSource<T> {
    fn drop(&mut self) {
        if let Some(h) = self.task.take() {
            h.abort();
        }
    }
}

async fn fetch_once<T: DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
) -> Result<T, RemoteSourceError> {
    let bytes = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    Ok(serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Clone, Default, Deserialize)]
    struct Blob {
        #[allow(dead_code)]
        version: u32,
    }

    #[test]
    fn construct_and_initial_value_visible() {
        let src = RemoteConfigSource::<Blob>::new(
            "https://example.com/config.json",
            Duration::from_secs(60),
            Blob { version: 7 },
        );
        let rx = src.subscribe();
        assert_eq!(rx.borrow().version, 7);
        assert_eq!(src.current().version, 7);
        assert_eq!(src.url(), "https://example.com/config.json");
    }
}
