use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    #[error("stale rates cleared")]
    StaleRatesCleared,
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
            if *rate != 0.0 {
                return Some(1.0 / rate);
            }
        }

        // Try intermediate currency conversion
        self.find_intermediate_rate(from, to)
    }

    fn find_intermediate_rate(&self, from: &str, to: &str) -> Option<f64> {
        for (_base, rates) in &self.conversions {
            if let (Some(to_rate), Some(from_rate)) = (rates.get(to), rates.get(from)) {
                if *from_rate != 0.0 {
                    return Some(to_rate / from_rate);
                }
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

// ---------------------------------------------------------------------------
// RateFetcher - periodically fetches currency rates from a remote URL
// ---------------------------------------------------------------------------

/// Configuration for the rate fetcher.
#[derive(Debug, Clone)]
pub struct RateFetcherConfig {
    /// URL to fetch currency rates from.
    pub fetch_url: String,
    /// How often to re-fetch rates.
    pub fetch_interval: Duration,
    /// If rates are older than this, they are considered stale and cleared.
    /// Set to Duration::ZERO to disable stale-rate checking.
    pub stale_after: Duration,
}

/// Internal state shared between the fetcher task and readers.
#[derive(Debug)]
pub struct RateFetcherInner {
    rates: Option<CurrencyRates>,
    last_updated: Option<Instant>,
}

/// RateFetcher fetches currency rates from a remote URL and stores them.
/// It spawns a background tokio task that refreshes rates periodically and
/// clears them if they become stale.
pub struct RateFetcher {
    config: RateFetcherConfig,
    client: reqwest::Client,
    inner: Arc<tokio::sync::RwLock<RateFetcherInner>>,
}

impl RateFetcher {
    /// Create a new RateFetcher with the given configuration.
    pub fn new(config: RateFetcherConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            inner: Arc::new(tokio::sync::RwLock::new(RateFetcherInner {
                rates: None,
                last_updated: None,
            })),
        }
    }

    /// Create with a custom reqwest client (useful for testing / proxies).
    pub fn with_client(config: RateFetcherConfig, client: reqwest::Client) -> Self {
        Self {
            config,
            client,
            inner: Arc::new(tokio::sync::RwLock::new(RateFetcherInner {
                rates: None,
                last_updated: None,
            })),
        }
    }

    /// Fetch rates from the remote URL and update internal state.
    pub async fn fetch_and_update(&self) -> Result<()> {
        let response = self
            .client
            .get(&self.config.fetch_url)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!(
                "currency rates request failed with status code {}",
                status.as_u16()
            );
        }

        let rates: CurrencyRates = response.json().await?;
        let mut lock = self.inner.write().await;
        lock.rates = Some(rates);
        lock.last_updated = Some(Instant::now());
        tracing::info!("Currency rates updated from {}", self.config.fetch_url);
        Ok(())
    }

    /// Check whether rates are stale and clear them if so.
    async fn check_and_clear_stale(&self) {
        if self.config.stale_after.is_zero() {
            return;
        }
        let mut lock = self.inner.write().await;
        if let Some(last) = lock.last_updated {
            if last.elapsed() > self.config.stale_after {
                tracing::warn!("Currency rates are stale, clearing");
                lock.rates = None;
            }
        }
    }

    /// Spawn a background tokio task that fetches rates on the configured interval.
    /// Returns the JoinHandle so callers can abort it on shutdown.
    pub fn start_fetcher(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(self.config.fetch_interval);
            loop {
                ticker.tick().await;
                if let Err(e) = self.fetch_and_update().await {
                    tracing::error!("Failed to update currency rates: {}", e);
                    self.check_and_clear_stale().await;
                }
            }
        })
    }

    /// Get a snapshot of the current rates, if available.
    pub async fn rates(&self) -> Option<CurrencyRates> {
        self.inner.read().await.rates.clone()
    }

    /// Get a handle that implements `Conversions` for synchronous access.
    pub fn sync_handle(&self) -> SyncRateConverter {
        SyncRateConverter {
            rates: Arc::new(std::sync::RwLock::new(
                // Snapshot current state
                None,
            )),
            url: self.config.fetch_url.clone(),
        }
    }

    /// Returns the shared inner lock (for building a SyncRateConverter that
    /// shares state with this fetcher).
    pub fn shared_inner(&self) -> Arc<tokio::sync::RwLock<RateFetcherInner>> {
        Arc::clone(&self.inner)
    }
}

// ---------------------------------------------------------------------------
// AggregateConversions - combines multiple rate sources
// ---------------------------------------------------------------------------

/// AggregateConversions overlays request-level custom rates on top of
/// fetched/primary rates. Request-level rates take priority.
pub struct AggregateConversions {
    /// Primary rates (e.g. from the periodic fetcher).
    primary: Box<dyn Conversions>,
    /// Request-level custom rates that override the primary.
    custom: Option<CurrencyRates>,
}

impl AggregateConversions {
    /// Create a new aggregate with primary rates and optional custom overrides.
    pub fn new(primary: Box<dyn Conversions>, custom: Option<CurrencyRates>) -> Self {
        Self { primary, custom }
    }
}

impl Conversions for AggregateConversions {
    fn get_rate(&self, from: &str, to: &str) -> Option<f64> {
        if from == to {
            return Some(1.0);
        }
        // Custom rates take priority
        if let Some(ref custom) = self.custom {
            if let Some(rate) = custom.get_rate(from, to) {
                return Some(rate);
            }
        }
        // Fall back to primary
        self.primary.get_rate(from, to)
    }

    fn get_rates(&self) -> Option<HashMap<String, HashMap<String, f64>>> {
        // Merge: start with primary, overlay custom
        let mut merged = self.primary.get_rates().unwrap_or_default();
        if let Some(ref custom) = self.custom {
            for (from, targets) in &custom.conversions {
                let entry = merged.entry(from.clone()).or_default();
                for (to, rate) in targets {
                    entry.insert(to.clone(), *rate);
                }
            }
        }
        Some(merged)
    }
}

// ---------------------------------------------------------------------------
// RateConverter - existing basic converter (kept for backward compat)
// ---------------------------------------------------------------------------

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

    /// Convert an amount between currencies using the currently loaded rates.
    pub async fn convert(&self, amount: f64, from: &str, to: &str) -> Result<f64, ConversionError> {
        if from == to {
            return Ok(amount);
        }
        let lock = self.current_rates.read().await;
        let rates = lock.as_ref().ok_or(ConversionError::RatesUnavailable)?;
        let rate = rates
            .get_rate(from, to)
            .ok_or_else(|| ConversionError::NotFound {
                from: from.to_string(),
                to: to.to_string(),
            })?;
        Ok(amount * rate)
    }

    /// Convert using specific custom rates (request-level overrides).
    pub async fn convert_with_rates(
        &self,
        amount: f64,
        from: &str,
        to: &str,
        custom_rates: &CurrencyRates,
    ) -> Result<f64, ConversionError> {
        if from == to {
            return Ok(amount);
        }
        // Try custom rates first
        if let Some(rate) = custom_rates.get_rate(from, to) {
            return Ok(amount * rate);
        }
        // Fall back to fetched rates
        self.convert(amount, from, to).await
    }

    /// Get the conversion rate between two currencies from the currently loaded
    /// rates, optionally overlaying custom request-level rates.
    pub async fn get_rate(
        &self,
        from: &str,
        to: &str,
        custom_rates: Option<&CurrencyRates>,
    ) -> Option<f64> {
        if from == to {
            return Some(1.0);
        }
        // Custom rates take priority
        if let Some(custom) = custom_rates {
            if let Some(rate) = custom.get_rate(from, to) {
                return Some(rate);
            }
        }
        // Fall back to fetched rates
        let lock = self.current_rates.read().await;
        lock.as_ref()?.get_rate(from, to)
    }
}

impl Conversions for RateConverter {
    fn get_rate(&self, from: &str, to: &str) -> Option<f64> {
        if from == to {
            return Some(1.0);
        }
        // Synchronous access via try_read
        let lock = self.current_rates.try_read().ok()?;
        lock.as_ref()?.get_rate(from, to)
    }

    fn get_rates(&self) -> Option<HashMap<String, HashMap<String, f64>>> {
        let lock = self.current_rates.try_read().ok()?;
        Some(lock.as_ref()?.conversions.clone())
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

    /// Convert an amount synchronously using the currently loaded rates.
    pub fn convert(&self, amount: f64, from: &str, to: &str) -> Result<f64, ConversionError> {
        if from == to {
            return Ok(amount);
        }
        let lock = self
            .rates
            .read()
            .map_err(|_| ConversionError::RatesUnavailable)?;
        let rates = lock.as_ref().ok_or(ConversionError::RatesUnavailable)?;
        let rate = rates
            .get_rate(from, to)
            .ok_or_else(|| ConversionError::NotFound {
                from: from.to_string(),
                to: to.to_string(),
            })?;
        Ok(amount * rate)
    }

    /// Convert using specific custom rates that override the stored rates.
    pub fn convert_with_rates(
        &self,
        amount: f64,
        from: &str,
        to: &str,
        custom_rates: &CurrencyRates,
    ) -> Result<f64, ConversionError> {
        if from == to {
            return Ok(amount);
        }
        // Custom rates first
        if let Some(rate) = custom_rates.get_rate(from, to) {
            return Ok(amount * rate);
        }
        // Fall back to stored rates
        self.convert(amount, from, to)
    }

    /// Get rate synchronously, with optional custom rates overlay.
    pub fn get_rate_with_custom(
        &self,
        from: &str,
        to: &str,
        custom_rates: Option<&CurrencyRates>,
    ) -> Option<f64> {
        if from == to {
            return Some(1.0);
        }
        if let Some(custom) = custom_rates {
            if let Some(rate) = custom.get_rate(from, to) {
                return Some(rate);
            }
        }
        let lock = self.rates.read().ok()?;
        lock.as_ref()?.get_rate(from, to)
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_rates() -> CurrencyRates {
        let mut conversions = HashMap::new();
        let mut usd = HashMap::new();
        usd.insert("EUR".to_string(), 0.85);
        usd.insert("GBP".to_string(), 0.77);
        conversions.insert("USD".to_string(), usd);

        let mut eur = HashMap::new();
        eur.insert("JPY".to_string(), 130.0);
        conversions.insert("EUR".to_string(), eur);

        CurrencyRates { conversions }
    }

    // -- CurrencyRates tests --

    #[test]
    fn test_same_currency() {
        let rates = sample_rates();
        assert_eq!(rates.get_rate("USD", "USD"), Some(1.0));
    }

    #[test]
    fn test_direct_lookup() {
        let rates = sample_rates();
        assert_eq!(rates.get_rate("USD", "EUR"), Some(0.85));
    }

    #[test]
    fn test_inverse_lookup() {
        let rates = sample_rates();
        let rate = rates.get_rate("EUR", "USD").unwrap();
        assert!((rate - 1.0 / 0.85).abs() < 1e-10);
    }

    #[test]
    fn test_intermediate_lookup() {
        // USD->EUR=0.85, USD->GBP=0.77, so GBP->EUR = 0.85/0.77
        let rates = sample_rates();
        let rate = rates.get_rate("GBP", "EUR").unwrap();
        assert!((rate - 0.85 / 0.77).abs() < 1e-10);
    }

    #[test]
    fn test_not_found() {
        let rates = sample_rates();
        assert_eq!(rates.get_rate("USD", "CHF"), None);
    }

    // -- ConstantRates tests --

    #[test]
    fn test_constant_rates_same() {
        let cr = ConstantRates;
        assert_eq!(cr.get_rate("USD", "USD"), Some(1.0));
    }

    #[test]
    fn test_constant_rates_different() {
        let cr = ConstantRates;
        assert_eq!(cr.get_rate("USD", "EUR"), None);
    }

    // -- StaticRates tests --

    #[test]
    fn test_static_rates() {
        let mut m = HashMap::new();
        let mut inner = HashMap::new();
        inner.insert("GBP".to_string(), 0.77);
        m.insert("USD".to_string(), inner);
        let sr = StaticRates::new(m);
        assert_eq!(sr.get_rate("USD", "GBP"), Some(0.77));
        assert!(sr.get_rates().is_some());
    }

    // -- AggregateConversions tests --

    #[test]
    fn test_aggregate_custom_overrides_primary() {
        let primary = Box::new(StaticRates::new({
            let mut m = HashMap::new();
            let mut inner = HashMap::new();
            inner.insert("EUR".to_string(), 0.85);
            m.insert("USD".to_string(), inner);
            m
        }));

        let mut custom_map = HashMap::new();
        let mut inner = HashMap::new();
        inner.insert("EUR".to_string(), 0.90);
        custom_map.insert("USD".to_string(), inner);
        let custom = CurrencyRates {
            conversions: custom_map,
        };

        let agg = AggregateConversions::new(primary, Some(custom));
        // Custom rate should win
        assert_eq!(agg.get_rate("USD", "EUR"), Some(0.90));
    }

    #[test]
    fn test_aggregate_falls_back_to_primary() {
        let primary = Box::new(StaticRates::new({
            let mut m = HashMap::new();
            let mut inner = HashMap::new();
            inner.insert("EUR".to_string(), 0.85);
            m.insert("USD".to_string(), inner);
            m
        }));

        let agg = AggregateConversions::new(primary, None);
        assert_eq!(agg.get_rate("USD", "EUR"), Some(0.85));
    }

    #[test]
    fn test_aggregate_same_currency() {
        let primary = Box::new(ConstantRates);
        let agg = AggregateConversions::new(primary, None);
        assert_eq!(agg.get_rate("USD", "USD"), Some(1.0));
    }

    #[test]
    fn test_aggregate_get_rates_merges() {
        let primary = Box::new(StaticRates::new({
            let mut m = HashMap::new();
            let mut inner = HashMap::new();
            inner.insert("EUR".to_string(), 0.85);
            m.insert("USD".to_string(), inner);
            m
        }));

        let mut custom_map = HashMap::new();
        let mut inner = HashMap::new();
        inner.insert("GBP".to_string(), 0.77);
        custom_map.insert("USD".to_string(), inner);
        let custom = CurrencyRates {
            conversions: custom_map,
        };

        let agg = AggregateConversions::new(primary, Some(custom));
        let merged = agg.get_rates().unwrap();
        let usd = merged.get("USD").unwrap();
        assert_eq!(usd.get("EUR"), Some(&0.85));
        assert_eq!(usd.get("GBP"), Some(&0.77));
    }

    // -- SyncRateConverter tests --

    #[test]
    fn test_sync_converter_no_rates() {
        let conv = SyncRateConverter::new("http://example.com");
        assert_eq!(conv.get_rate("USD", "EUR"), None);
        assert_eq!(conv.get_rate("USD", "USD"), Some(1.0));
    }

    #[test]
    fn test_sync_converter_with_rates() {
        let conv = SyncRateConverter::new("http://example.com");
        conv.update(sample_rates());
        assert_eq!(conv.get_rate("USD", "EUR"), Some(0.85));
    }

    #[test]
    fn test_sync_converter_convert() {
        let conv = SyncRateConverter::new("http://example.com");
        conv.update(sample_rates());
        let result = conv.convert(100.0, "USD", "EUR").unwrap();
        assert!((result - 85.0).abs() < 1e-10);
    }

    #[test]
    fn test_sync_converter_convert_same() {
        let conv = SyncRateConverter::new("http://example.com");
        let result = conv.convert(100.0, "USD", "USD").unwrap();
        assert!((result - 100.0).abs() < 1e-10);
    }

    #[test]
    fn test_sync_converter_convert_no_rates() {
        let conv = SyncRateConverter::new("http://example.com");
        let result = conv.convert(100.0, "USD", "EUR");
        assert!(result.is_err());
    }

    #[test]
    fn test_sync_converter_convert_with_custom_rates() {
        let conv = SyncRateConverter::new("http://example.com");
        conv.update(sample_rates());

        let mut custom_map = HashMap::new();
        let mut inner = HashMap::new();
        inner.insert("EUR".to_string(), 0.95);
        custom_map.insert("USD".to_string(), inner);
        let custom = CurrencyRates {
            conversions: custom_map,
        };

        // Custom rate should override stored rate
        let result = conv.convert_with_rates(100.0, "USD", "EUR", &custom).unwrap();
        assert!((result - 95.0).abs() < 1e-10);

        // For a pair not in custom rates, falls back to stored
        let result = conv.convert_with_rates(100.0, "USD", "GBP", &custom).unwrap();
        assert!((result - 77.0).abs() < 1e-10);
    }

    #[test]
    fn test_sync_converter_get_rate_with_custom() {
        let conv = SyncRateConverter::new("http://example.com");
        conv.update(sample_rates());

        let mut custom_map = HashMap::new();
        let mut inner = HashMap::new();
        inner.insert("EUR".to_string(), 0.95);
        custom_map.insert("USD".to_string(), inner);
        let custom = CurrencyRates {
            conversions: custom_map,
        };

        assert_eq!(
            conv.get_rate_with_custom("USD", "EUR", Some(&custom)),
            Some(0.95)
        );
        assert_eq!(
            conv.get_rate_with_custom("USD", "GBP", Some(&custom)),
            Some(0.77)
        );
        assert_eq!(
            conv.get_rate_with_custom("USD", "GBP", None),
            Some(0.77)
        );
    }

    // -- RateFetcher config test --

    #[test]
    fn test_rate_fetcher_config() {
        let config = RateFetcherConfig {
            fetch_url: "https://cdn.jsdelivr.net/rates.json".to_string(),
            fetch_interval: Duration::from_secs(300),
            stale_after: Duration::from_secs(3600),
        };
        assert_eq!(config.fetch_interval.as_secs(), 300);
        assert_eq!(config.stale_after.as_secs(), 3600);
    }

    // -- CurrencyRates edge cases --

    #[test]
    fn test_empty_rates() {
        let rates = CurrencyRates::default();
        assert_eq!(rates.get_rate("USD", "EUR"), None);
        assert_eq!(rates.get_rate("USD", "USD"), Some(1.0));
    }

    #[test]
    fn test_zero_rate_inverse_safety() {
        let mut conversions = HashMap::new();
        let mut inner = HashMap::new();
        inner.insert("EUR".to_string(), 0.0);
        conversions.insert("USD".to_string(), inner);
        let rates = CurrencyRates { conversions };
        // Direct lookup returns 0.0
        assert_eq!(rates.get_rate("USD", "EUR"), Some(0.0));
        // Inverse would be 1/0 - should return None
        assert_eq!(rates.get_rate("EUR", "USD"), None);
    }
}
