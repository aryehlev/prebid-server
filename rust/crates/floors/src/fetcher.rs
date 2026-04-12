//! Async stub for dynamic price-floor fetching.
//!
//! The real Go implementation maintains a worker pool, an in-memory
//! LRU cache and a priority queue of URLs to refresh; this skeleton
//! records the public shape needed to plug a fetcher into the
//! exchange pipeline while leaving the implementation for later.

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

/// Result of a fetch attempt.
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

/// Stub async fetcher. Returns `FetchStatus::None` until a real
/// implementation is wired in. Kept `async` so the signature does not
/// change when an HTTP client is added later.
pub async fn fetch_price_floor_rules(_cfg: &FloorFetcherConfig) -> FetchResult {
    tracing::debug!("fetch_price_floor_rules stub invoked");
    FetchResult {
        rules: None,
        status: Some(FetchStatusCode(FetchStatus::None)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_status_str() {
        assert_eq!(FetchStatus::Success.as_str(), "success");
        assert_eq!(FetchStatus::None.as_str(), "none");
    }

    #[test]
    fn fetch_stub_returns_none() {
        let cfg = FloorFetcherConfig::default();
        // Use a minimal executor to drive the future.
        let result = futures_lite_block_on(fetch_price_floor_rules(&cfg));
        assert!(result.rules.is_none());
    }

    // Tiny block-on to avoid pulling tokio into this crate's deps.
    fn futures_lite_block_on<F: std::future::Future>(mut fut: F) -> F::Output {
        use std::pin::Pin;
        use std::task::{Context, Poll, Waker, RawWaker, RawWakerVTable};
        fn noop(_: *const ()) {}
        fn clone(_: *const ()) -> RawWaker {
            RawWaker::new(std::ptr::null(), &VTABLE)
        }
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
        let raw = RawWaker::new(std::ptr::null(), &VTABLE);
        let waker = unsafe { Waker::from_raw(raw) };
        let mut cx = Context::from_waker(&waker);
        // Safety: we don't move `fut` after this point.
        let mut fut = unsafe { Pin::new_unchecked(&mut fut) };
        loop {
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(v) => return v,
                Poll::Pending => continue,
            }
        }
    }
}
