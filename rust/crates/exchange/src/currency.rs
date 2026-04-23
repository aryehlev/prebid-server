use std::collections::HashMap;

/// Simple currency converter using a rates table
pub struct CurrencyConverter {
    /// rates[from][to] = multiplier
    rates: HashMap<String, HashMap<String, f64>>,
    /// Optional source URL from which rates were fetched.
    source: Option<String>,
    /// Timestamp of last successful rate update (Unix seconds).
    last_updated: Option<i64>,
}

impl CurrencyConverter {
    pub fn new(rates: HashMap<String, HashMap<String, f64>>) -> Self {
        Self { rates, source: None, last_updated: None }
    }

    pub fn empty() -> Self {
        Self { rates: HashMap::new(), source: None, last_updated: None }
    }

    /// Create a converter with metadata about the rate source.
    pub fn with_info(
        rates: HashMap<String, HashMap<String, f64>>,
        source: Option<String>,
        last_updated: Option<i64>,
    ) -> Self {
        Self { rates, source, last_updated }
    }

    /// Return a reference to the raw rates table.
    pub fn rates(&self) -> &HashMap<String, HashMap<String, f64>> {
        &self.rates
    }

    /// Return the source URL from which rates were fetched.
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// Return the timestamp of the last successful rate update (Unix seconds).
    pub fn last_updated(&self) -> Option<i64> {
        self.last_updated
    }

    /// Convert amount from `from` currency to `to` currency
    /// Returns None if conversion not available
    pub fn convert(&self, amount: f64, from: &str, to: &str) -> Option<f64> {
        if from == to { return Some(amount); }
        // Direct rate
        if let Some(rate) = self.rates.get(from).and_then(|m| m.get(to)) {
            return Some(amount * rate);
        }
        // Inverse rate
        if let Some(rate) = self.rates.get(to).and_then(|m| m.get(from)) {
            return Some(amount / rate);
        }
        // Via USD
        if from != "USD" && to != "USD" {
            let to_usd = self.rates.get(from).and_then(|m| m.get("USD"))?;
            let from_usd = self.rates.get("USD").and_then(|m| m.get(to))?;
            return Some(amount * to_usd * from_usd);
        }
        None
    }

    /// Convert using request-level custom rates with this converter as fallback.
    ///
    /// Follows the Go PBS logic: request-level rates take priority, then the
    /// server-fetched rates are used as fallback.
    pub fn convert_with_overrides(
        &self,
        amount: f64,
        from: &str,
        to: &str,
        request_rates: &HashMap<String, HashMap<String, f64>>,
    ) -> Option<f64> {
        if from == to {
            return Some(amount);
        }
        // Try request-level rates first (direct)
        if let Some(rate) = request_rates.get(from).and_then(|m| m.get(to)) {
            return Some(amount * rate);
        }
        // Try request-level rates (inverse)
        if let Some(rate) = request_rates.get(to).and_then(|m| m.get(from)) {
            return Some(amount / rate);
        }
        // Fall back to server rates
        self.convert(amount, from, to)
    }

    /// Get the conversion rate between two currencies (without multiplying by amount).
    pub fn get_rate(&self, from: &str, to: &str) -> Option<f64> {
        self.convert(1.0, from, to)
    }

    /// Merge additional rates into this converter (request-level rates override).
    pub fn with_additional_rates(
        &self,
        additional: &HashMap<String, HashMap<String, f64>>,
    ) -> Self {
        let mut rates = self.rates.clone();
        for (from_currency, to_map) in additional {
            let entry = rates.entry(from_currency.clone()).or_default();
            for (to_currency, rate) in to_map {
                entry.insert(to_currency.clone(), *rate);
            }
        }
        Self {
            rates,
            source: self.source.clone(),
            last_updated: self.last_updated,
        }
    }
}

/// Extract request-level currency conversion rates from ext.prebid.currency.
///
/// Format: `{ "ext": { "prebid": { "currency": { "rates": { "EUR": { "USD": 1.13 } } } } } }`
pub fn extract_request_rates(
    bid_request: &openrtb::BidRequest,
) -> HashMap<String, HashMap<String, f64>> {
    bid_request
        .ext
        .as_ref()
        .and_then(|e| e.get("prebid"))
        .and_then(|p| p.get("currency"))
        .and_then(|c| c.get("rates"))
        .and_then(|r| serde_json::from_value(r.clone()).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rates() -> CurrencyConverter {
        let mut rates = HashMap::new();
        let mut usd_to = HashMap::new();
        usd_to.insert("EUR".to_string(), 0.85);
        usd_to.insert("GBP".to_string(), 0.73);
        rates.insert("USD".to_string(), usd_to);
        let mut eur_to = HashMap::new();
        eur_to.insert("USD".to_string(), 1.18);
        rates.insert("EUR".to_string(), eur_to);
        CurrencyConverter::new(rates)
    }

    #[test]
    fn test_same_currency() {
        let c = make_rates();
        assert_eq!(c.convert(10.0, "USD", "USD"), Some(10.0));
    }

    #[test]
    fn test_direct_conversion() {
        let c = make_rates();
        let result = c.convert(10.0, "USD", "EUR").unwrap();
        assert!((result - 8.5).abs() < 0.001);
    }

    #[test]
    fn test_inverse_conversion() {
        let c = make_rates();
        // GBP->USD: inverse of USD->GBP (0.73)
        let result = c.convert(10.0, "GBP", "USD").unwrap();
        assert!((result - 10.0 / 0.73).abs() < 0.01);
    }

    #[test]
    fn test_unknown_currency() {
        let c = make_rates();
        assert!(c.convert(10.0, "USD", "JPY").is_none());
    }

    #[test]
    fn test_get_rate() {
        let c = make_rates();
        assert_eq!(c.get_rate("USD", "EUR"), Some(0.85));
    }

    #[test]
    fn test_convert_with_overrides() {
        let c = make_rates();
        let mut overrides = HashMap::new();
        let mut usd_custom = HashMap::new();
        usd_custom.insert("EUR".to_string(), 0.90);
        overrides.insert("USD".to_string(), usd_custom);

        // Override rate should be used
        let result = c.convert_with_overrides(10.0, "USD", "EUR", &overrides).unwrap();
        assert!((result - 9.0).abs() < 0.001);
    }

    #[test]
    fn test_convert_with_overrides_fallback() {
        let c = make_rates();
        let overrides = HashMap::new(); // empty overrides

        // Should fall back to server rates
        let result = c.convert_with_overrides(10.0, "USD", "EUR", &overrides).unwrap();
        assert!((result - 8.5).abs() < 0.001);
    }

    #[test]
    fn test_with_additional_rates() {
        let c = make_rates();
        let mut additional = HashMap::new();
        let mut usd_extra = HashMap::new();
        usd_extra.insert("JPY".to_string(), 110.0);
        additional.insert("USD".to_string(), usd_extra);

        let merged = c.with_additional_rates(&additional);
        // Original rate still works
        assert!(merged.convert(10.0, "USD", "EUR").is_some());
        // New rate works too
        let result = merged.convert(10.0, "USD", "JPY").unwrap();
        assert!((result - 1100.0).abs() < 0.01);
    }

    #[test]
    fn test_extract_request_rates_empty() {
        let req = openrtb::BidRequest::default();
        let rates = extract_request_rates(&req);
        assert!(rates.is_empty());
    }

    #[test]
    fn test_extract_request_rates() {
        let mut req = openrtb::BidRequest::default();
        req.ext = Some(serde_json::json!({
            "prebid": {
                "currency": {
                    "rates": {
                        "USD": { "EUR": 0.90 }
                    }
                }
            }
        }));
        let rates = extract_request_rates(&req);
        assert_eq!(rates["USD"]["EUR"], 0.90);
    }

    #[test]
    fn test_with_info() {
        let c = CurrencyConverter::with_info(
            HashMap::new(),
            Some("https://cdn.jsdelivr.net/gh/prebid/currency-file@1/latest.json".to_string()),
            Some(1700000000),
        );
        assert_eq!(c.source(), Some("https://cdn.jsdelivr.net/gh/prebid/currency-file@1/latest.json"));
        assert_eq!(c.last_updated(), Some(1700000000));
    }
}
