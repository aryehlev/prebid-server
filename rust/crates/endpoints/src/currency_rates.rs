//! Skeleton implementation of the `GET /currency/rates` endpoint.
//!
//! Mirrors `endpoints/currency_rates.go`: reply with a JSON blob describing
//! the status of the currency rate converter and the most recent rates map.
//!
//! This version decouples from the heavier `pbs-currency` crate: the handler
//! reads from a trait object so callers can plug in any rate source.

use axum::{extract::State, response::IntoResponse, Json};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

/// Minimal interface a rate converter must implement for this skeleton.
pub trait RateConverter: Send + Sync + std::fmt::Debug {
    fn source(&self) -> Option<String>;
    fn fetching_interval_ns(&self) -> Option<u128>;
    fn last_updated_rfc3339(&self) -> Option<String>;
    fn rates(&self) -> Option<HashMap<String, HashMap<String, f64>>>;
    fn additional_info(&self) -> Option<serde_json::Value>;
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CurrencyRatesResponse {
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "fetchingIntervalNs")]
    pub fetching_interval_ns: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "lastUpdated")]
    pub last_updated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rates: Option<HashMap<String, HashMap<String, f64>>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "additionalInfo")]
    pub additional_info: Option<serde_json::Value>,
}

/// Shared state for the currency rates endpoint.
#[derive(Clone, Default)]
pub struct CurrencyRatesState {
    pub converter: Option<Arc<dyn RateConverter>>,
}

impl std::fmt::Debug for CurrencyRatesState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CurrencyRatesState")
            .field("converter", &self.converter.is_some())
            .finish()
    }
}

pub type SharedCurrencyRatesState = Arc<CurrencyRatesState>;

/// Axum handler for `GET /currency/rates` (skeleton).
pub async fn currency_rates_handler(
    State(state): State<SharedCurrencyRatesState>,
) -> impl IntoResponse {
    let Some(converter) = state.converter.as_ref() else {
        return Json(CurrencyRatesResponse::default());
    };
    Json(CurrencyRatesResponse {
        active: true,
        source: converter.source(),
        fetching_interval_ns: converter.fetching_interval_ns(),
        last_updated: converter.last_updated_rfc3339(),
        rates: converter.rates(),
        additional_info: converter.additional_info(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct StubConverter;
    impl RateConverter for StubConverter {
        fn source(&self) -> Option<String> {
            Some("stub".into())
        }
        fn fetching_interval_ns(&self) -> Option<u128> {
            Some(1_000_000_000)
        }
        fn last_updated_rfc3339(&self) -> Option<String> {
            Some("1970-01-01T00:00:00Z".into())
        }
        fn rates(&self) -> Option<HashMap<String, HashMap<String, f64>>> {
            Some(HashMap::new())
        }
        fn additional_info(&self) -> Option<serde_json::Value> {
            None
        }
    }

    #[test]
    fn stub_serializes() {
        let state = CurrencyRatesState {
            converter: Some(Arc::new(StubConverter)),
        };
        let s: Arc<dyn RateConverter> = state.converter.clone().unwrap();
        assert_eq!(s.source().as_deref(), Some("stub"));
    }
}
