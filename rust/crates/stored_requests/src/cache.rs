//! A [`CachedFetcher`] wrapper that fronts another [`Fetcher`] with
//! in-process `moka` caches for both stored requests and stored imps.

use async_trait::async_trait;
use moka::future::Cache;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::fetcher::{FetchError, Fetcher};

/// A fetcher that wraps another [`Fetcher`] with in-memory caches.
///
/// Requests and imps each get their own `moka::future::Cache<String, Value>`.
/// On a cache miss, the inner fetcher is consulted and its results are
/// populated back into the cache.
pub struct CachedFetcher<F: Fetcher> {
    /// The underlying fetcher consulted on cache misses.
    pub inner: Arc<F>,
    /// Cache of stored request JSON keyed by request ID.
    pub requests: Cache<String, Value>,
    /// Cache of stored imp JSON keyed by imp ID.
    pub imps: Cache<String, Value>,
}

impl<F: Fetcher> CachedFetcher<F> {
    /// Create a [`CachedFetcher`] with default sizing (10k entries per
    /// cache, no TTL).
    pub fn new(inner: Arc<F>) -> Self {
        Self::with_config(inner, 10_000, None)
    }

    /// Create a [`CachedFetcher`] with a specific max capacity and
    /// optional time-to-live per entry.
    pub fn with_config(inner: Arc<F>, max_capacity: u64, ttl: Option<Duration>) -> Self {
        let build = |cap: u64, ttl: Option<Duration>| {
            let mut b = Cache::builder().max_capacity(cap);
            if let Some(t) = ttl {
                b = b.time_to_live(t);
            }
            b.build()
        };
        Self {
            inner,
            requests: build(max_capacity, ttl),
            imps: build(max_capacity, ttl),
        }
    }
}

async fn drain_cache(cache: &Cache<String, Value>, ids: &[String]) -> (HashMap<String, Value>, Vec<String>) {
    let mut hits = HashMap::with_capacity(ids.len());
    let mut misses = Vec::new();
    for id in ids {
        if let Some(v) = cache.get(id).await {
            hits.insert(id.clone(), v);
        } else {
            misses.push(id.clone());
        }
    }
    (hits, misses)
}

#[async_trait]
impl<F: Fetcher> Fetcher for CachedFetcher<F> {
    async fn fetch_requests(
        &self,
        req_ids: &[String],
        imp_ids: &[String],
    ) -> (
        HashMap<String, Value>,
        HashMap<String, Value>,
        Vec<FetchError>,
    ) {
        let (mut req_hits, req_misses) = drain_cache(&self.requests, req_ids).await;
        let (mut imp_hits, imp_misses) = drain_cache(&self.imps, imp_ids).await;

        if req_misses.is_empty() && imp_misses.is_empty() {
            return (req_hits, imp_hits, Vec::new());
        }

        let (fetched_req, fetched_imp, errs) =
            self.inner.fetch_requests(&req_misses, &imp_misses).await;

        for (k, v) in fetched_req {
            self.requests.insert(k.clone(), v.clone()).await;
            req_hits.insert(k, v);
        }
        for (k, v) in fetched_imp {
            self.imps.insert(k.clone(), v.clone()).await;
            imp_hits.insert(k, v);
        }

        (req_hits, imp_hits, errs)
    }

    async fn fetch_account(&self, account_id: &str) -> Result<Value, FetchError> {
        self.inner.fetch_account(account_id).await
    }

    async fn fetch_categories(
        &self,
        primary_adserver: &str,
        publisher_id: &str,
    ) -> Result<String, FetchError> {
        self.inner
            .fetch_categories(primary_adserver, publisher_id)
            .await
    }

    async fn fetch_responses(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, Value>, FetchError> {
        self.inner.fetch_responses(ids).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingFetcher {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl Fetcher for CountingFetcher {
        async fn fetch_requests(
            &self,
            req_ids: &[String],
            imp_ids: &[String],
        ) -> (
            HashMap<String, Value>,
            HashMap<String, Value>,
            Vec<FetchError>,
        ) {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let mut req = HashMap::new();
            for id in req_ids {
                req.insert(id.clone(), serde_json::json!({ "id": id }));
            }
            let mut imp = HashMap::new();
            for id in imp_ids {
                imp.insert(id.clone(), serde_json::json!({ "imp": id }));
            }
            (req, imp, Vec::new())
        }
        async fn fetch_account(&self, _id: &str) -> Result<Value, FetchError> {
            Err(FetchError::Other("unused".into()))
        }
        async fn fetch_categories(&self, _a: &str, _p: &str) -> Result<String, FetchError> {
            Err(FetchError::Other("unused".into()))
        }
        async fn fetch_responses(
            &self,
            _ids: &[String],
        ) -> Result<HashMap<String, Value>, FetchError> {
            Ok(HashMap::new())
        }
    }

    #[tokio::test]
    async fn second_call_is_cached() {
        let inner = Arc::new(CountingFetcher {
            calls: AtomicUsize::new(0),
        });
        let cached = CachedFetcher::new(inner.clone());

        let ids = vec!["a".to_string()];
        let (req1, _, _) = cached.fetch_requests(&ids, &[]).await;
        assert_eq!(req1.len(), 1);
        assert_eq!(inner.calls.load(Ordering::SeqCst), 1);

        let (req2, _, _) = cached.fetch_requests(&ids, &[]).await;
        assert_eq!(req2.len(), 1);
        // Still 1 — the second call should have been served from cache.
        assert_eq!(inner.calls.load(Ordering::SeqCst), 1);
    }
}
