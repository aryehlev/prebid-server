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
}
