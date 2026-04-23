//! Stub for the `scope3` Real-Time Data (RTD) hook module.
//!
//! Scope3 provides emissions / sustainability signals for in-flight auctions.
//! This stub gives downstream code a stable async entry point
//! ([`fetch_rtd`]) so it can be wired up ahead of the real integration.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Scope3Error {
    #[error("scope3 rtd fetch failed: {0}")]
    Fetch(String),
    #[error("scope3 config invalid: {0}")]
    Config(String),
}

/// Module configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Scope3Config {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub auth_token: String,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_timeout_ms() -> u64 {
    200
}

/// A minimal request snapshot used by the stub.
#[derive(Debug, Clone, Default)]
pub struct Request {
    pub publisher_id: String,
    pub site_domain: String,
    pub ad_slot: String,
    pub bidders: Vec<String>,
}

/// RTD payload returned to the auction.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RtdData {
    pub emissions_grams_co2: f64,
    pub gpp_category: String,
    pub segments: Vec<String>,
    pub per_bidder_scores: HashMap<String, f64>,
}

/// The module handle.
pub struct Scope3Module {
    pub config: Scope3Config,
}

impl Scope3Module {
    pub fn new(config: Scope3Config) -> Self {
        Self { config }
    }
}

/// Fetch RTD for the given request.
///
/// Stub implementation — returns a deterministic dummy payload so callers can
/// exercise the wiring. The real implementation will issue an HTTP call with
/// a timeout.
pub async fn fetch_rtd(req: &Request) -> Result<RtdData, Scope3Error> {
    let mut per_bidder = HashMap::new();
    for (i, b) in req.bidders.iter().enumerate() {
        per_bidder.insert(b.clone(), (i as f64) * 0.25);
    }
    Ok(RtdData {
        emissions_grams_co2: 1.23,
        gpp_category: "low".to_string(),
        segments: vec!["eco_friendly".to_string()],
        per_bidder_scores: per_bidder,
    })
}

/// An async trait so the module can be plugged into a generic RTD provider registry.
#[async_trait]
pub trait RtdProvider: Send + Sync {
    async fn fetch(&self, req: &Request) -> Result<RtdData, Scope3Error>;
    fn name(&self) -> &str;
}

#[async_trait]
impl RtdProvider for Scope3Module {
    fn name(&self) -> &str {
        "scope3"
    }
    async fn fetch(&self, req: &Request) -> Result<RtdData, Scope3Error> {
        if !self.config.enabled {
            return Ok(RtdData::default());
        }
        fetch_rtd(req).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        // A tiny inline executor so we don't need tokio as a dev-dep.
        // We rely on the futures being ready immediately, which is true for
        // the stub implementations here.
        use std::sync::Arc;
        use std::task::{Context, Poll, Wake, Waker};

        struct NoopWaker;
        impl Wake for NoopWaker {
            fn wake(self: Arc<Self>) {}
        }

        let waker = Waker::from(Arc::new(NoopWaker));
        let mut cx = Context::from_waker(&waker);
        let mut fut = Box::pin(f);
        loop {
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(v) => return v,
                Poll::Pending => continue,
            }
        }
    }

    #[test]
    fn test_fetch_rtd_returns_dummy() {
        let req = Request {
            publisher_id: "pub-1".into(),
            site_domain: "example.com".into(),
            ad_slot: "slot-a".into(),
            bidders: vec!["appnexus".into(), "rubicon".into()],
        };
        let out = block_on(fetch_rtd(&req));
        assert!(out.is_ok());
        let data = out.unwrap();
        assert_eq!(data.gpp_category, "low");
        assert_eq!(data.segments, vec!["eco_friendly".to_string()]);
        assert_eq!(data.per_bidder_scores.len(), 2);
        assert_eq!(data.per_bidder_scores.get("appnexus"), Some(&0.0));
        assert_eq!(data.per_bidder_scores.get("rubicon"), Some(&0.25));
    }

    #[test]
    fn test_disabled_module_returns_empty() {
        let m = Scope3Module::new(Scope3Config::default());
        let out = block_on(m.fetch(&Request::default()));
        let data = out.unwrap();
        assert_eq!(data.emissions_grams_co2, 0.0);
        assert!(data.per_bidder_scores.is_empty());
    }

    #[test]
    fn test_name() {
        let m = Scope3Module::new(Scope3Config::default());
        assert_eq!(m.name(), "scope3");
    }
}
