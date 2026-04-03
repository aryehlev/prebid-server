use std::collections::HashMap;

/// Simple currency converter using a rates table
pub struct CurrencyConverter {
    /// rates[from][to] = multiplier
    rates: HashMap<String, HashMap<String, f64>>,
}

impl CurrencyConverter {
    pub fn new(rates: HashMap<String, HashMap<String, f64>>) -> Self {
        Self { rates }
    }

    pub fn empty() -> Self {
        Self { rates: HashMap::new() }
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
