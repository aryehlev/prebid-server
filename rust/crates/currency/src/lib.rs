//! Currency rate conversion and fetching.
//!
//! Port of the Go `currency` package from prebid-server. Provides:
//!
//! - [`Rates`]: a map of `from -> to -> rate`, matching the JSON format served
//!   from <https://cdn.jsdelivr.net/gh/prebid/currency-file@1/latest.json>.
//! - [`ConversionRates`]: trait exposing `get_rate(from, to)` for any rate source.
//! - [`ConstantRates`]: a no-op source that only allows identity conversions.
//! - [`RateConverter`]: fetches [`Rates`] from an HTTP URL periodically and
//!   caches them with last-updated timestamps and stale-rate handling.
//! - [`AggregateConversions`]: layers request-level custom rates over a primary
//!   (e.g. fetched) source, prioritising the custom rates.
//!
//! Conversion lookup order in [`Rates::get_rate`]:
//!
//! 1. identity (`from == to`)
//! 2. direct `from -> to`
//! 3. reciprocal `to -> from` (inverted)
//! 4. triangulation via USD (preferred intermediate)
//! 5. triangulation via any other base currency present in the table

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Errors returned by rate lookups and fetching.
#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    /// No conversion (direct, reciprocal, or intermediate) could be found.
    #[error("Currency conversion rate not found: '{from}' => '{to}'")]
    NotFound { from: String, to: String },

    /// The currency code was not a valid ISO 4217 three-letter code.
    #[error("invalid currency code: '{0}'")]
    InvalidCurrency(String),

    /// Rates have not yet been loaded (e.g. first fetch hasn't completed).
    #[error("rates are not available")]
    RatesUnavailable,

    /// An HTTP error was returned while fetching rates.
    #[error("the currency rates request failed with status code {0}")]
    BadServerResponse(u16),

    /// A networking / transport error occurred during fetch.
    #[error("the currency rates request failed: {0}")]
    Transport(String),

    /// Failed to decode the JSON body.
    #[error("the currency rates request failed to parse json: {0}")]
    Parse(String),
}

/// Trait for any source of currency conversion rates.
///
/// Async so implementations may acquire async locks or perform async lookups.
/// For purely synchronous sources, the async blocks are effectively free.
#[async_trait]
pub trait ConversionRates: Send + Sync {
    /// Get the conversion rate from `from` to `to`.
    async fn get_rate(&self, from: &str, to: &str) -> Result<f64, ConversionError>;

    /// Optional: return a snapshot of all known rates. Primarily used for
    /// introspection / diagnostics.
    async fn get_rates(&self) -> Option<HashMap<String, HashMap<String, f64>>> {
        None
    }
}

/// Validate an ISO 4217 currency code (three uppercase ASCII letters).
fn validate_iso(code: &str) -> Result<String, ConversionError> {
    let trimmed = code.trim();
    if trimmed.len() != 3 || !trimmed.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(ConversionError::InvalidCurrency(code.to_string()));
    }
    Ok(trimmed.to_ascii_uppercase())
}

/// Rate table as represented on the prebid currency file CDN.
///
/// Matches:
/// ```json
/// { "conversions": { "USD": { "EUR": 0.85, "GBP": 0.77 }, ... } }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Rates {
    pub conversions: HashMap<String, HashMap<String, f64>>,
}

impl Rates {
    /// Create a new `Rates` from a conversions map.
    pub fn new(conversions: HashMap<String, HashMap<String, f64>>) -> Self {
        Self { conversions }
    }

    /// Synchronous rate lookup.
    ///
    /// Tries: identity, direct, reciprocal, USD triangulation, then any
    /// intermediate base currency.
    pub fn get_rate(&self, from: &str, to: &str) -> Result<f64, ConversionError> {
        let from = validate_iso(from)?;
        let to = validate_iso(to)?;

        if from == to {
            return Ok(1.0);
        }

        // Direct: from -> to
        if let Some(rate) = self.conversions.get(&from).and_then(|m| m.get(&to)) {
            return Ok(*rate);
        }

        // Reciprocal: to -> from
        if let Some(rate) = self.conversions.get(&to).and_then(|m| m.get(&from)) {
            if *rate != 0.0 {
                return Ok(1.0 / rate);
            }
        }

        // Triangulation via USD (preferred pivot)
        if from != "USD" && to != "USD" {
            if let Some(usd) = self.conversions.get("USD") {
                if let (Some(usd_to_from), Some(usd_to_to)) = (usd.get(&from), usd.get(&to)) {
                    if *usd_to_from != 0.0 {
                        return Ok(usd_to_to / usd_to_from);
                    }
                }
            }
        }

        // Triangulation via any intermediate base in the table
        for rates in self.conversions.values() {
            if let (Some(to_rate), Some(from_rate)) = (rates.get(&to), rates.get(&from)) {
                if *from_rate != 0.0 {
                    return Ok(to_rate / from_rate);
                }
            }
        }

        Err(ConversionError::NotFound { from, to })
    }
}

#[async_trait]
impl ConversionRates for Rates {
    async fn get_rate(&self, from: &str, to: &str) -> Result<f64, ConversionError> {
        Rates::get_rate(self, from, to)
    }

    async fn get_rates(&self) -> Option<HashMap<String, HashMap<String, f64>>> {
        Some(self.conversions.clone())
    }
}

/// A no-op converter that only allows same-currency conversions.
///
/// Used as a fallback whenever live rates are unavailable or have been cleared
/// due to staleness.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConstantRates;

impl ConstantRates {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ConversionRates for ConstantRates {
    async fn get_rate(&self, from: &str, to: &str) -> Result<f64, ConversionError> {
        let from = validate_iso(from)?;
        let to = validate_iso(to)?;
        if from == to {
            Ok(1.0)
        } else {
            Err(ConversionError::NotFound { from, to })
        }
    }

    async fn get_rates(&self) -> Option<HashMap<String, HashMap<String, f64>>> {
        None
    }
}

/// Layers request-level custom rates over a primary source, prioritising
/// the custom rates. If the custom source returns [`ConversionError::NotFound`]
/// the primary source is consulted. Other errors propagate immediately.
pub struct AggregateConversions {
    custom: Arc<dyn ConversionRates>,
    primary: Arc<dyn ConversionRates>,
}

impl AggregateConversions {
    pub fn new(custom: Arc<dyn ConversionRates>, primary: Arc<dyn ConversionRates>) -> Self {
        Self { custom, primary }
    }
}

#[async_trait]
impl ConversionRates for AggregateConversions {
    async fn get_rate(&self, from: &str, to: &str) -> Result<f64, ConversionError> {
        match self.custom.get_rate(from, to).await {
            Ok(rate) => Ok(rate),
            Err(ConversionError::NotFound { .. }) => self.primary.get_rate(from, to).await,
            Err(e) => Err(e),
        }
    }

    async fn get_rates(&self) -> Option<HashMap<String, HashMap<String, f64>>> {
        None
    }
}

// ---------------------------------------------------------------------------
// RateConverter - periodic fetch + caching
// ---------------------------------------------------------------------------

/// Configuration for [`RateConverter`].
#[derive(Debug, Clone)]
pub struct RateConverterConfig {
    /// URL to fetch rates JSON from.
    pub sync_source_url: String,
    /// Timeout for each HTTP fetch.
    pub http_timeout: std::time::Duration,
    /// How often to refetch rates.
    pub fetch_interval: std::time::Duration,
    /// If loaded rates are older than this, they are considered stale and
    /// replaced with the [`ConstantRates`] fallback. Zero disables the check.
    pub stale_rates_threshold: std::time::Duration,
}

impl Default for RateConverterConfig {
    fn default() -> Self {
        Self {
            sync_source_url: String::new(),
            http_timeout: std::time::Duration::from_secs(30),
            fetch_interval: std::time::Duration::from_secs(3600),
            stale_rates_threshold: std::time::Duration::ZERO,
        }
    }
}

#[derive(Debug, Default)]
struct RateConverterState {
    rates: Option<Rates>,
    last_updated: Option<DateTime<Utc>>,
}

/// Fetches currency rates from an HTTP URL on an interval and caches them.
///
/// Background refreshes are driven by [`RateConverter::run`]. Rates can be read
/// at any time via [`RateConverter::rates`] or the [`ConversionRates`] impl. On
/// error, a configurable staleness check will clear the cache and cause reads
/// to fall back to [`ConstantRates`].
pub struct RateConverter {
    config: RateConverterConfig,
    client: reqwest::Client,
    state: Arc<tokio::sync::RwLock<RateConverterState>>,
    constant: ConstantRates,
}

impl RateConverter {
    /// Create a new converter using a default reqwest client configured with
    /// the provided timeout.
    pub fn new(config: RateConverterConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(config.http_timeout)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self::with_client(config, client)
    }

    /// Create a new converter with an explicit reqwest client.
    pub fn with_client(config: RateConverterConfig, client: reqwest::Client) -> Self {
        Self {
            config,
            client,
            state: Arc::new(tokio::sync::RwLock::new(RateConverterState::default())),
            constant: ConstantRates,
        }
    }

    /// The URL rates are being fetched from.
    pub fn source(&self) -> &str {
        &self.config.sync_source_url
    }

    /// Timestamp of the last successful fetch, if any.
    pub async fn last_updated(&self) -> Option<DateTime<Utc>> {
        self.state.read().await.last_updated
    }

    /// Return a snapshot of the currently cached [`Rates`], if loaded.
    pub async fn rates(&self) -> Option<Rates> {
        self.state.read().await.rates.clone()
    }

    /// Fetch rates once and update the cache. On failure, check staleness and
    /// optionally clear the cache so readers fall back to [`ConstantRates`].
    pub async fn update(&self) -> Result<(), ConversionError> {
        match self.fetch().await {
            Ok(rates) => {
                let mut state = self.state.write().await;
                state.rates = Some(rates);
                state.last_updated = Some(Utc::now());
                tracing::info!(url = %self.config.sync_source_url, "currency rates updated");
                Ok(())
            }
            Err(e) => {
                let cleared = self.check_and_clear_stale().await;
                if cleared {
                    tracing::error!(
                        error = %e,
                        "error updating conversion rates, falling back to constant rates"
                    );
                } else {
                    tracing::error!(error = %e, "error updating conversion rates");
                }
                Err(e)
            }
        }
    }

    /// Alias for [`RateConverter::update`] matching the Go method name.
    pub async fn run(&self) -> Result<(), ConversionError> {
        self.update().await
    }

    /// Spawn a background task that calls [`RateConverter::update`] on the
    /// configured interval. Returns the join handle; dropping it does NOT
    /// cancel the task, so callers who need cancellation should abort it.
    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let this = Arc::clone(&self);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(this.config.fetch_interval);
            loop {
                ticker.tick().await;
                let _ = this.update().await;
            }
        })
    }

    /// Perform the HTTP fetch and JSON decode.
    async fn fetch(&self) -> Result<Rates, ConversionError> {
        let response = self
            .client
            .get(&self.config.sync_source_url)
            .send()
            .await
            .map_err(|e| ConversionError::Transport(e.to_string()))?;

        let status = response.status();
        if status.as_u16() >= 400 {
            // Drain body so the connection can be reused.
            let _ = response.bytes().await;
            return Err(ConversionError::BadServerResponse(status.as_u16()));
        }

        let body = response
            .bytes()
            .await
            .map_err(|e| ConversionError::Transport(e.to_string()))?;

        serde_json::from_slice::<Rates>(&body).map_err(|e| ConversionError::Parse(e.to_string()))
    }

    /// Clear cached rates if they are older than the configured stale
    /// threshold. Returns true if the cache was cleared.
    async fn check_and_clear_stale(&self) -> bool {
        if self.config.stale_rates_threshold.is_zero() {
            return false;
        }
        let threshold = match chrono::Duration::from_std(self.config.stale_rates_threshold) {
            Ok(d) => d,
            Err(_) => return false,
        };
        let mut state = self.state.write().await;
        if let Some(last) = state.last_updated {
            if Utc::now() - last > threshold {
                state.rates = None;
                return true;
            }
        }
        false
    }
}

#[async_trait]
impl ConversionRates for RateConverter {
    async fn get_rate(&self, from: &str, to: &str) -> Result<f64, ConversionError> {
        let state = self.state.read().await;
        if let Some(ref rates) = state.rates {
            return rates.get_rate(from, to);
        }
        drop(state);
        self.constant.get_rate(from, to).await
    }

    async fn get_rates(&self) -> Option<HashMap<String, HashMap<String, f64>>> {
        self.state
            .read()
            .await
            .rates
            .as_ref()
            .map(|r| r.conversions.clone())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_rates() -> Rates {
        // USD -> EUR=0.85, USD -> GBP=0.77, EUR -> JPY=130
        let mut conversions = HashMap::new();
        let mut usd = HashMap::new();
        usd.insert("EUR".to_string(), 0.85);
        usd.insert("GBP".to_string(), 0.77);
        conversions.insert("USD".to_string(), usd);

        let mut eur = HashMap::new();
        eur.insert("JPY".to_string(), 130.0);
        conversions.insert("EUR".to_string(), eur);

        Rates::new(conversions)
    }

    // ------- validate_iso -------

    #[test]
    fn test_validate_iso_ok() {
        assert_eq!(validate_iso("usd").unwrap(), "USD");
        assert_eq!(validate_iso("EUR").unwrap(), "EUR");
    }

    #[test]
    fn test_validate_iso_bad() {
        assert!(validate_iso("US").is_err());
        assert!(validate_iso("USDD").is_err());
        assert!(validate_iso("12A").is_err());
    }

    // ------- Rates direct/reciprocal -------

    #[test]
    fn test_same_currency() {
        let rates = sample_rates();
        assert_eq!(rates.get_rate("USD", "USD").unwrap(), 1.0);
    }

    #[test]
    fn test_direct_lookup() {
        let rates = sample_rates();
        assert_eq!(rates.get_rate("USD", "EUR").unwrap(), 0.85);
    }

    #[test]
    fn test_reciprocal_lookup() {
        let rates = sample_rates();
        let rate = rates.get_rate("EUR", "USD").unwrap();
        assert!((rate - 1.0 / 0.85).abs() < 1e-10);
    }

    #[test]
    fn test_reciprocal_rate_case_insensitive() {
        let rates = sample_rates();
        let rate = rates.get_rate("eur", "usd").unwrap();
        assert!((rate - 1.0 / 0.85).abs() < 1e-10);
    }

    // ------- Triangulation -------

    #[test]
    fn test_triangulation_via_usd() {
        // GBP -> EUR should resolve as USD->EUR / USD->GBP = 0.85 / 0.77
        let rates = sample_rates();
        let rate = rates.get_rate("GBP", "EUR").unwrap();
        assert!((rate - 0.85 / 0.77).abs() < 1e-10);
    }

    #[test]
    fn test_triangulation_via_non_usd_base() {
        // Build a table where USD does NOT contain both, but EUR does.
        let mut conversions = HashMap::new();
        let mut eur = HashMap::new();
        eur.insert("GBP".to_string(), 0.90);
        eur.insert("JPY".to_string(), 130.0);
        conversions.insert("EUR".to_string(), eur);
        let rates = Rates::new(conversions);

        // GBP -> JPY = 130 / 0.90
        let rate = rates.get_rate("GBP", "JPY").unwrap();
        assert!((rate - 130.0 / 0.90).abs() < 1e-10);
    }

    #[test]
    fn test_not_found() {
        let rates = sample_rates();
        assert!(matches!(
            rates.get_rate("USD", "CHF"),
            Err(ConversionError::NotFound { .. })
        ));
    }

    #[test]
    fn test_empty_rates() {
        let rates = Rates::default();
        assert!(rates.get_rate("USD", "EUR").is_err());
        assert_eq!(rates.get_rate("USD", "USD").unwrap(), 1.0);
    }

    #[test]
    fn test_zero_rate_inverse_safety() {
        let mut conversions = HashMap::new();
        let mut inner = HashMap::new();
        inner.insert("EUR".to_string(), 0.0);
        conversions.insert("USD".to_string(), inner);
        let rates = Rates::new(conversions);
        // Direct lookup returns 0.0
        assert_eq!(rates.get_rate("USD", "EUR").unwrap(), 0.0);
        // Reciprocal should NOT divide by zero
        assert!(rates.get_rate("EUR", "USD").is_err());
    }

    // ------- ConstantRates -------

    #[tokio::test]
    async fn test_constant_rates_same() {
        let c = ConstantRates;
        assert_eq!(c.get_rate("USD", "USD").await.unwrap(), 1.0);
    }

    #[tokio::test]
    async fn test_constant_rates_different() {
        let c = ConstantRates;
        assert!(matches!(
            c.get_rate("USD", "EUR").await,
            Err(ConversionError::NotFound { .. })
        ));
    }

    #[tokio::test]
    async fn test_constant_rates_invalid() {
        let c = ConstantRates;
        assert!(matches!(
            c.get_rate("XX", "EUR").await,
            Err(ConversionError::InvalidCurrency(_))
        ));
    }

    // ------- Rates via trait -------

    #[tokio::test]
    async fn test_rates_trait() {
        let rates: Arc<dyn ConversionRates> = Arc::new(sample_rates());
        let rate = rates.get_rate("USD", "EUR").await.unwrap();
        assert_eq!(rate, 0.85);
    }

    // ------- AggregateConversions -------

    #[tokio::test]
    async fn test_aggregate_custom_overrides_primary() {
        let mut primary_map = HashMap::new();
        let mut inner = HashMap::new();
        inner.insert("EUR".to_string(), 0.85);
        primary_map.insert("USD".to_string(), inner);
        let primary: Arc<dyn ConversionRates> = Arc::new(Rates::new(primary_map));

        let mut custom_map = HashMap::new();
        let mut inner = HashMap::new();
        inner.insert("EUR".to_string(), 0.90);
        custom_map.insert("USD".to_string(), inner);
        let custom: Arc<dyn ConversionRates> = Arc::new(Rates::new(custom_map));

        let agg = AggregateConversions::new(custom, primary);
        assert_eq!(agg.get_rate("USD", "EUR").await.unwrap(), 0.90);
    }

    #[tokio::test]
    async fn test_aggregate_falls_back_to_primary_on_not_found() {
        let mut primary_map = HashMap::new();
        let mut inner = HashMap::new();
        inner.insert("GBP".to_string(), 0.77);
        primary_map.insert("USD".to_string(), inner);
        let primary: Arc<dyn ConversionRates> = Arc::new(Rates::new(primary_map));

        // Custom doesn't have USD->GBP, but primary does
        let custom: Arc<dyn ConversionRates> = Arc::new(Rates::new(HashMap::new()));

        let agg = AggregateConversions::new(custom, primary);
        assert_eq!(agg.get_rate("USD", "GBP").await.unwrap(), 0.77);
    }

    #[tokio::test]
    async fn test_aggregate_same_currency() {
        let primary: Arc<dyn ConversionRates> = Arc::new(ConstantRates);
        let custom: Arc<dyn ConversionRates> = Arc::new(ConstantRates);
        let agg = AggregateConversions::new(custom, primary);
        assert_eq!(agg.get_rate("USD", "USD").await.unwrap(), 1.0);
    }

    // ------- RateConverter (no network) -------

    #[tokio::test]
    async fn test_rate_converter_falls_back_to_constant() {
        let config = RateConverterConfig {
            sync_source_url: "http://localhost:0/missing".into(),
            ..Default::default()
        };
        let rc = RateConverter::new(config);
        // No rates loaded -> same-currency works via constant fallback
        assert_eq!(rc.get_rate("USD", "USD").await.unwrap(), 1.0);
        // Different currency -> NotFound via constant fallback
        assert!(matches!(
            rc.get_rate("USD", "EUR").await,
            Err(ConversionError::NotFound { .. })
        ));
        assert!(rc.last_updated().await.is_none());
        assert!(rc.rates().await.is_none());
    }

    #[tokio::test]
    async fn test_rate_converter_uses_cached_rates() {
        let config = RateConverterConfig {
            sync_source_url: "http://localhost:0/missing".into(),
            ..Default::default()
        };
        let rc = RateConverter::new(config);
        // Manually populate the cache to simulate a successful fetch
        {
            let mut state = rc.state.write().await;
            state.rates = Some(sample_rates());
            state.last_updated = Some(Utc::now());
        }
        assert_eq!(rc.get_rate("USD", "EUR").await.unwrap(), 0.85);
        // Triangulation works too
        let rate = rc.get_rate("GBP", "EUR").await.unwrap();
        assert!((rate - 0.85 / 0.77).abs() < 1e-10);
        assert!(rc.last_updated().await.is_some());
        assert!(rc.rates().await.is_some());
    }

    #[tokio::test]
    async fn test_rate_converter_config_default() {
        let cfg = RateConverterConfig::default();
        assert_eq!(cfg.sync_source_url, "");
        assert!(cfg.stale_rates_threshold.is_zero());
    }

    // ------- JSON deserialization round-trip -------

    #[test]
    fn test_rates_json_roundtrip() {
        let json = r#"{"conversions":{"USD":{"EUR":0.85,"GBP":0.77}}}"#;
        let rates: Rates = serde_json::from_str(json).unwrap();
        assert_eq!(rates.get_rate("USD", "EUR").unwrap(), 0.85);
        let back = serde_json::to_string(&rates).unwrap();
        // Re-parse to confirm structure is valid
        let _: Rates = serde_json::from_str(&back).unwrap();
    }
}
