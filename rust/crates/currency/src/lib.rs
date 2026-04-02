use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Error type for currency conversion failures
#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error("conversion rate not found from {from} to {to}")]
    NotFound { from: String, to: String },
    #[error("invalid currency code: {0}")]
    InvalidCurrency(String),
    #[error("rates not available")]
    RatesUnavailable,
}

/// Conversions allows getting a conversion rate between two currencies.
pub trait Conversions: Send + Sync {
    /// Get the conversion rate from one currency to another.
    /// Returns None if the conversion is unknown.
    fn get_rate(&self, from: &str, to: &str) -> Option<f64>;

    /// Get a reference to all current rates, if available.
    fn get_rates(&self) -> Option<HashMap<String, HashMap<String, f64>>>;
}

/// CurrencyRates holds the deserialized rate data from the CDN JSON.
/// Matches the format from https://cdn.jsdelivr.net/gh/prebid/currency-file@1/latest.json
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CurrencyRates {
    pub conversions: HashMap<String, HashMap<String, f64>>,
}

impl CurrencyRates {
    /// Look up a conversion rate. Tries direct lookup, then inverse, then via intermediate.
    pub fn get_rate(&self, from: &str, to: &str) -> Option<f64> {
        if from == to {
            return Some(1.0);
        }

        // Direct lookup
        if let Some(rate) = self.conversions.get(from).and_then(|m| m.get(to)) {
            return Some(*rate);
        }

        // Inverse lookup
        if let Some(rate) = self.conversions.get(to).and_then(|m| m.get(from)) {
            return Some(1.0 / rate);
        }

        // Try intermediate currency conversion
        self.find_intermediate_rate(from, to)
    }

    fn find_intermediate_rate(&self, from: &str, to: &str) -> Option<f64> {
        for (_base, rates) in &self.conversions {
            let to_rate = rates.get(to)?;
            let from_rate = rates.get(from)?;
            if rates.contains_key(to) && rates.contains_key(from) {
                return Some(to_rate / from_rate);
            }
        }
        None
    }
}

/// ConstantRates only allows same-currency conversions (rate = 1.0).
/// Used as a fallback when no rates are available.
#[derive(Debug, Clone, Default)]
pub struct ConstantRates;

impl Conversions for ConstantRates {
    fn get_rate(&self, from: &str, to: &str) -> Option<f64> {
        if from == to {
            Some(1.0)
        } else {
            None
        }
    }

    fn get_rates(&self) -> Option<HashMap<String, HashMap<String, f64>>> {
        None
    }
}

/// StaticRates holds a fixed set of conversion rates (useful for testing).
#[derive(Debug, Clone, Default)]
pub struct StaticRates {
    rates: CurrencyRates,
}

impl StaticRates {
    pub fn new(conversions: HashMap<String, HashMap<String, f64>>) -> Self {
        Self {
            rates: CurrencyRates { conversions },
        }
    }
}

impl Conversions for StaticRates {
    fn get_rate(&self, from: &str, to: &str) -> Option<f64> {
        self.rates.get_rate(from, to)
    }

    fn get_rates(&self) -> Option<HashMap<String, HashMap<String, f64>>> {
        Some(self.rates.conversions.clone())
    }
}

/// RateConverter fetches currency rates from a remote URL and caches them.
/// The rates are refreshed periodically in a background task.
pub struct RateConverter {
    url: String,
    client: reqwest::Client,
    current_rates: Arc<tokio::sync::RwLock<Option<CurrencyRates>>>,
}

impl RateConverter {
    /// Create a new RateConverter that fetches rates from the given URL.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            client: reqwest::Client::new(),
            current_rates: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    /// Fetch and update the current rates from the remote URL.
    pub async fn fetch_and_update(&self) -> Result<()> {
        let response = self.client.get(&self.url).send().await?;
        let rates: CurrencyRates = response.json().await?;
        let mut lock = self.current_rates.write().await;
        *lock = Some(rates);
        tracing::info!("Currency rates updated from {}", self.url);
        Ok(())
    }

    /// Start a background task that refreshes rates on the given interval.
    pub fn start_background_refresh(
        self: Arc<Self>,
        interval: std::time::Duration,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                if let Err(e) = self.fetch_and_update().await {
                    tracing::error!("Failed to update currency rates: {}", e);
                }
            }
        })
    }

    /// Get the current rates snapshot.
    pub async fn rates(&self) -> Option<CurrencyRates> {
        self.current_rates.read().await.clone()
    }
}

impl Conversions for RateConverter {
    fn get_rate(&self, from: &str, to: &str) -> Option<f64> {
        // Synchronous access — try to get a read lock without blocking
        if from == to {
            return Some(1.0);
        }
        // In sync contexts, we fall back to constant rates behavior since
        // RateConverter is primarily meant to be used via async methods.
        // A full implementation would use try_read or a separate Arc<RwLock>.
        None
    }

    fn get_rates(&self) -> Option<HashMap<String, HashMap<String, f64>>> {
        None
    }
}

/// SyncRateConverter is a version of RateConverter suitable for synchronous access.
/// It uses an Arc<RwLock> that can be read synchronously.
#[derive(Clone, Default)]
pub struct SyncRateConverter {
    rates: Arc<std::sync::RwLock<Option<CurrencyRates>>>,
    url: String,
}

impl SyncRateConverter {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            rates: Arc::new(std::sync::RwLock::new(None)),
            url: url.into(),
        }
    }

    pub fn update(&self, rates: CurrencyRates) {
        if let Ok(mut lock) = self.rates.write() {
            *lock = Some(rates);
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }
}

impl Conversions for SyncRateConverter {
    fn get_rate(&self, from: &str, to: &str) -> Option<f64> {
        if from == to {
            return Some(1.0);
        }
        let lock = self.rates.read().ok()?;
        lock.as_ref()?.get_rate(from, to)
    }

    fn get_rates(&self) -> Option<HashMap<String, HashMap<String, f64>>> {
        let lock = self.rates.read().ok()?;
        Some(lock.as_ref()?.conversions.clone())
    }
}
