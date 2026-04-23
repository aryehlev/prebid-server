//! HTTP-backed fetcher for dynamic price-floor payloads.
//!
//! Maintains an in-memory [`moka`] cache keyed by endpoint URL. Callers hand
//! out an endpoint string; the fetcher returns either a cached
//! [`FetchedFloors`] (if present and still fresh) or performs a GET, parses
//! the JSON body into a [`PriceFloorRules`], stores it in the cache, and
//! returns the new entry.
//!
//! The fetcher also exposes a `refresh_all` method intended to be spawned on
//! a background tokio task; it walks every currently-cached endpoint and
//! re-fetches it, replacing the cached entry on success.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::types::PriceFloorRules;

/// Fetch status values corresponding to the Go constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchStatus {
    Success,
    Timeout,
    Error,
    InProgress,
    None,
}

impl FetchStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Timeout => "timeout",
            Self::Error => "error",
            Self::InProgress => "inprogress",
            Self::None => "none",
        }
    }
}

/// Minimal configuration required to fetch floors for an account.
#[derive(Debug, Clone, Default)]
pub struct FloorFetcherConfig {
    pub enabled: bool,
    pub url: String,
    pub account_id: String,
    pub timeout_ms: u64,
    pub max_file_size_kb: u64,
    pub max_age_sec: u64,
    pub period_sec: u64,
    pub max_schema_dims: usize,
}

/// Result of a fetch attempt. Retained for backwards compatibility with the
/// original stub API; richer error information is available via
/// [`FloorFetcher::fetch`] which returns a typed [`FetchError`].
#[derive(Debug, Clone, Default)]
pub struct FetchResult {
    pub rules: Option<PriceFloorRules>,
    pub status: Option<FetchStatusCode>,
}

#[derive(Debug, Clone, Copy)]
pub struct FetchStatusCode(pub FetchStatus);

impl Default for FetchStatusCode {
    fn default() -> Self {
        FetchStatusCode(FetchStatus::None)
    }
}

/// An errored fetch attempt.
#[derive(Debug, Error)]
pub enum FetchError {
    #[error("http transport error: {0}")]
    Http(String),
    #[error("bad http status: {0}")]
    BadStatus(u16),
    #[error("payload parse error: {0}")]
    Parse(String),
    #[error("request timed out")]
    Timeout,
}

impl FetchError {
    pub fn status(&self) -> FetchStatus {
        match self {
            FetchError::Timeout => FetchStatus::Timeout,
            _ => FetchStatus::Error,
        }
    }
}

/// A cached floors payload plus the metadata required to decide whether it is
/// still fresh.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchedFloors {
    pub endpoint: String,
    pub fetched_at: DateTime<Utc>,
    pub rules: PriceFloorRules,
}

/// HTTP-backed floors fetcher with an in-memory cache.
#[derive(Clone)]
pub struct FloorFetcher {
    client: reqwest::Client,
    refresh_period: Duration,
    max_age: Duration,
    cache: Cache<String, Arc<FetchedFloors>>,
}

impl FloorFetcher {
    /// Construct a new fetcher from the given configuration.
    pub fn new(config: FloorFetcherConfig) -> Self {
        let timeout = if config.timeout_ms == 0 {
            Duration::from_secs(5)
        } else {
            Duration::from_millis(config.timeout_ms)
        };
        let refresh_period = if config.period_sec == 0 {
            Duration::from_secs(300)
        } else {
            Duration::from_secs(config.period_sec)
        };
        let max_age = if config.max_age_sec == 0 {
            Duration::from_secs(600)
        } else {
            Duration::from_secs(config.max_age_sec)
        };

        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let cache = Cache::builder()
            .max_capacity(1024)
            .time_to_live(max_age * 2)
            .build();

        Self {
            client,
            refresh_period,
            max_age,
            cache,
        }
    }

    /// Refresh period used by the background poller.
    pub fn refresh_period(&self) -> Duration {
        self.refresh_period
    }

    /// Fetch the floors payload for `endpoint`, returning a cached entry if
    /// one exists and is still fresh.
    pub async fn fetch(&self, endpoint: &str) -> Result<Arc<FetchedFloors>, FetchError> {
        if let Some(existing) = self.cache.get(endpoint).await {
            if !self.is_stale(&existing) {
                return Ok(existing);
            }
        }
        let fetched = self.fetch_uncached(endpoint).await?;
        let arc = Arc::new(fetched);
        self.cache.insert(endpoint.to_string(), arc.clone()).await;
        Ok(arc)
    }

    /// Force a fetch from the remote endpoint (bypassing the cache) and
    /// update the cached entry. Returns the newly fetched value.
    pub async fn refresh(&self, endpoint: &str) -> Result<Arc<FetchedFloors>, FetchError> {
        let fetched = self.fetch_uncached(endpoint).await?;
        let arc = Arc::new(fetched);
        self.cache.insert(endpoint.to_string(), arc.clone()).await;
        Ok(arc)
    }

    /// Iterate over every currently-cached endpoint and refetch each.
    /// Failures are logged but do not abort the walk.
    pub async fn refresh_all(&self) {
        let endpoints: Vec<String> = self
            .cache
            .iter()
            .map(|(k, _)| (*k).clone())
            .collect();
        for ep in endpoints {
            match self.refresh(&ep).await {
                Ok(_) => tracing::debug!(endpoint = %ep, "refreshed floors"),
                Err(e) => tracing::warn!(endpoint = %ep, error = %e, "floors refresh failed"),
            }
        }
    }

    /// Return a clone of the cached entry for `endpoint`, if any.
    pub async fn cached(&self, endpoint: &str) -> Option<Arc<FetchedFloors>> {
        self.cache.get(endpoint).await
    }

    fn is_stale(&self, entry: &FetchedFloors) -> bool {
        let age = Utc::now().signed_duration_since(entry.fetched_at);
        match age.to_std() {
            Ok(d) => d > self.max_age,
            Err(_) => false, // fetched in the future; treat as fresh
        }
    }

    async fn fetch_uncached(&self, endpoint: &str) -> Result<FetchedFloors, FetchError> {
        let resp = self.client.get(endpoint).send().await.map_err(|e| {
            if e.is_timeout() {
                FetchError::Timeout
            } else {
                FetchError::Http(e.to_string())
            }
        })?;
        let status = resp.status();
        if !status.is_success() {
            return Err(FetchError::BadStatus(status.as_u16()));
        }
        let body = resp
            .bytes()
            .await
            .map_err(|e| FetchError::Http(e.to_string()))?;
        let rules: PriceFloorRules =
            serde_json::from_slice(&body).map_err(|e| FetchError::Parse(e.to_string()))?;
        Ok(FetchedFloors {
            endpoint: endpoint.to_string(),
            fetched_at: Utc::now(),
            rules,
        })
    }
}

/// Legacy free-standing entry point. Preserves the original signature so
/// existing callers keep compiling; when a [`FloorFetcher`] is available the
/// caller should delegate directly via [`FloorFetcher::fetch`] using
/// [`fetch_price_floor_rules_with`].
pub async fn fetch_price_floor_rules(cfg: &FloorFetcherConfig) -> FetchResult {
    if !cfg.enabled || cfg.url.is_empty() {
        return FetchResult {
            rules: None,
            status: Some(FetchStatusCode(FetchStatus::None)),
        };
    }
    let fetcher = FloorFetcher::new(cfg.clone());
    fetch_price_floor_rules_with(&fetcher, &cfg.url).await
}

/// Delegating helper: run a fetch through an existing [`FloorFetcher`] and
/// translate the typed result into the legacy [`FetchResult`] shape.
pub async fn fetch_price_floor_rules_with(
    fetcher: &FloorFetcher,
    endpoint: &str,
) -> FetchResult {
    match fetcher.fetch(endpoint).await {
        Ok(fetched) => FetchResult {
            rules: Some(fetched.rules.clone()),
            status: Some(FetchStatusCode(FetchStatus::Success)),
        },
        Err(err) => {
            tracing::warn!(endpoint = %endpoint, error = %err, "fetch_price_floor_rules failed");
            FetchResult {
                rules: None,
                status: Some(FetchStatusCode(err.status())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn fetch_status_str() {
        assert_eq!(FetchStatus::Success.as_str(), "success");
        assert_eq!(FetchStatus::None.as_str(), "none");
    }

    const CANNED_JSON: &str = r#"{
        "floormin": 1.5,
        "floormincur": "USD",
        "enabled": true,
        "data": {
            "currency": "USD",
            "modelgroups": [
                {
                    "currency": "USD",
                    "schema": {"fields": ["mediaType"], "delimiter": "|"},
                    "values": {"banner": 0.5}
                }
            ]
        }
    }"#;

    /// Spawn a tiny tokio TCP listener that serves the canned payload on
    /// every accepted connection and increments a shared counter. Returns the
    /// bound URL and the counter so tests can assert hit counts.
    async fn start_mock_server() -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{}/floors.json", addr);
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(x) => x,
                    Err(_) => break,
                };
                let counter = counter_clone.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    // Read the request line+headers (best effort).
                    let _ = sock.read(&mut buf).await;
                    counter.fetch_add(1, Ordering::SeqCst);
                    let body = CANNED_JSON;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                    let _ = sock.shutdown().await;
                });
            }
        });
        (url, counter)
    }

    #[tokio::test]
    async fn fetcher_caches_second_call() {
        let (url, counter) = start_mock_server().await;
        let fetcher = FloorFetcher::new(FloorFetcherConfig {
            enabled: true,
            url: url.clone(),
            timeout_ms: 2_000,
            max_age_sec: 60,
            period_sec: 60,
            ..Default::default()
        });

        let first = fetcher.fetch(&url).await.expect("first fetch");
        assert_eq!(first.rules.floor_min, 1.5);
        assert_eq!(first.endpoint, url);

        let second = fetcher.fetch(&url).await.expect("second fetch");
        // Same Arc pointer => served from cache.
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(counter.load(Ordering::SeqCst), 1, "server hit once");
    }

    #[tokio::test]
    async fn fetcher_bad_status_is_error() {
        // Listener that always responds 500.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{}/err", addr);
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(x) => x,
                    Err(_) => break,
                };
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = sock.read(&mut buf).await;
                    let body = "nope";
                    let resp = format!(
                        "HTTP/1.1 500 Internal Server Error\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                    let _ = sock.shutdown().await;
                });
            }
        });
        let fetcher = FloorFetcher::new(FloorFetcherConfig {
            enabled: true,
            url: url.clone(),
            timeout_ms: 2_000,
            ..Default::default()
        });
        let err = fetcher.fetch(&url).await.unwrap_err();
        assert!(matches!(err, FetchError::BadStatus(500)));
    }

    #[tokio::test]
    async fn fetch_price_floor_rules_with_delegates() {
        let (url, _) = start_mock_server().await;
        let fetcher = FloorFetcher::new(FloorFetcherConfig {
            enabled: true,
            url: url.clone(),
            timeout_ms: 2_000,
            ..Default::default()
        });
        let res = fetch_price_floor_rules_with(&fetcher, &url).await;
        assert!(res.rules.is_some());
        assert!(matches!(
            res.status.map(|c| c.0),
            Some(FetchStatus::Success)
        ));
    }

    #[tokio::test]
    async fn legacy_fetch_returns_none_when_disabled() {
        let cfg = FloorFetcherConfig::default();
        let res = fetch_price_floor_rules(&cfg).await;
        assert!(res.rules.is_none());
        assert!(matches!(res.status.map(|c| c.0), Some(FetchStatus::None)));
    }
}
