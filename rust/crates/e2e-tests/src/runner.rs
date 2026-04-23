//! Runner that drives a [`Scenario`] through an [`AuctionBackend`] and diffs
//! the result.

use crate::backend::AuctionBackend;
use crate::error::E2eError;
use crate::scenario::Scenario;
use async_trait::async_trait;
use parity::diff::{diff, DiffEntry};
use parity::options::DiffOptions;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use stored_requests::fetcher::{FetchError, Fetcher};

/// Trivial in-memory [`Fetcher`] used to prime the auction pipeline with a
/// scenario's setup section.
#[derive(Debug, Clone, Default)]
pub struct InMemoryFetcher {
    /// Stored requests keyed by ID.
    pub stored_requests: HashMap<String, Value>,
    /// Stored imps keyed by ID.
    pub stored_imps: HashMap<String, Value>,
    /// Accounts keyed by ID.
    pub accounts: HashMap<String, Value>,
    /// Stored responses keyed by ID.
    pub stored_responses: HashMap<String, Value>,
}

impl InMemoryFetcher {
    /// Build a fetcher from a scenario's setup.
    pub fn from_scenario(scenario: &Scenario) -> Self {
        Self {
            stored_requests: scenario.setup.stored_requests.clone(),
            stored_imps: scenario.setup.stored_imps.clone(),
            accounts: scenario.setup.accounts.clone(),
            stored_responses: HashMap::new(),
        }
    }
}

#[async_trait]
impl Fetcher for InMemoryFetcher {
    async fn fetch_requests(
        &self,
        req_ids: &[String],
        imp_ids: &[String],
    ) -> (
        HashMap<String, Value>,
        HashMap<String, Value>,
        Vec<FetchError>,
    ) {
        let mut reqs = HashMap::new();
        let mut imps = HashMap::new();
        let mut errs = Vec::new();
        for id in req_ids {
            match self.stored_requests.get(id) {
                Some(v) => {
                    reqs.insert(id.clone(), v.clone());
                }
                None => errs.push(FetchError::not_found(id.clone(), "Request")),
            }
        }
        for id in imp_ids {
            match self.stored_imps.get(id) {
                Some(v) => {
                    imps.insert(id.clone(), v.clone());
                }
                None => errs.push(FetchError::not_found(id.clone(), "Imp")),
            }
        }
        (reqs, imps, errs)
    }

    async fn fetch_account(&self, account_id: &str) -> Result<Value, FetchError> {
        self.accounts
            .get(account_id)
            .cloned()
            .ok_or_else(|| FetchError::not_found(account_id, "Account"))
    }

    async fn fetch_categories(
        &self,
        _primary_adserver: &str,
        _publisher_id: &str,
    ) -> Result<String, FetchError> {
        Ok(String::new())
    }

    async fn fetch_responses(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, Value>, FetchError> {
        let mut out = HashMap::new();
        for id in ids {
            if let Some(v) = self.stored_responses.get(id) {
                out.insert(id.clone(), v.clone());
            }
        }
        Ok(out)
    }
}

/// Outcome of running a single [`Scenario`].
#[derive(Debug, Clone)]
pub struct ScenarioResult {
    /// Scenario name from the YAML document.
    pub name: String,
    /// `true` if the actual response matched `expected_response` modulo the
    /// configured ignore paths.
    pub passed: bool,
    /// All diff entries produced by [`parity::diff::diff`].
    pub diffs: Vec<DiffEntry>,
    /// Warnings surfaced by the runner.
    pub warnings: Vec<String>,
}

/// Runner that drives a [`Scenario`] through an [`AuctionBackend`].
pub struct ScenarioRunner {
    scenario: Scenario,
    backend: Arc<dyn AuctionBackend>,
    ignore_paths: Vec<String>,
}

impl ScenarioRunner {
    /// Build a runner for the given scenario with the default ignore list
    /// (`ext.responsetimemillis`, `id`).
    pub fn new(scenario: Scenario, backend: Arc<dyn AuctionBackend>) -> Self {
        Self {
            scenario,
            backend,
            ignore_paths: vec!["ext.responsetimemillis".to_string(), "id".to_string()],
        }
    }

    /// Override the default ignore-paths list.
    pub fn with_ignore_paths(mut self, paths: Vec<String>) -> Self {
        self.ignore_paths = paths;
        self
    }

    /// Expose the in-memory fetcher primed from the scenario's setup.
    pub fn fetcher(&self) -> InMemoryFetcher {
        InMemoryFetcher::from_scenario(&self.scenario)
    }

    /// Run the scenario and return its [`ScenarioResult`].
    pub async fn run(&self) -> Result<ScenarioResult, E2eError> {
        // 1) Prime the in-memory fetcher with the setup data. We do not wire
        //    it into the stub backend, but exposing it keeps the contract
        //    with real backends simple.
        let _fetcher = self.fetcher();

        // 2) Call the backend.
        let actual = self
            .backend
            .run_auction(&self.scenario.request)
            .await
            .map_err(|e| E2eError::Backend(e.to_string()))?;

        // 3) Diff the result against the expected response.
        let opts = DiffOptions {
            ignore_paths: self.ignore_paths.clone(),
            ..Default::default()
        };
        let diffs = diff(&self.scenario.expected_response, &actual, &opts);
        let passed = diffs.is_empty();

        Ok(ScenarioResult {
            name: self.scenario.name.clone(),
            passed,
            diffs,
            warnings: self.scenario.expected_warnings.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::StubAuctionBackend;
    use serde_json::json;

    fn simple_banner_scenario() -> Scenario {
        Scenario {
            name: "simple_banner".to_string(),
            description: Some("stub simple banner".to_string()),
            request: json!({
                "id": "req-1",
                "imp": [{
                    "id": "imp-1",
                    "mock_response": {
                        "id": "req-1",
                        "seatbid": [{
                            "bid": [{"id": "bid-1", "impid": "imp-1", "price": 1.5}]
                        }]
                    }
                }]
            }),
            expected_response: json!({
                "id": "req-1",
                "seatbid": [{
                    "bid": [{"id": "bid-1", "impid": "imp-1", "price": 1.5}]
                }]
            }),
            expected_warnings: vec![],
            setup: Default::default(),
        }
    }

    #[tokio::test]
    async fn runner_with_stub_passes_simple_banner() {
        let scen = simple_banner_scenario();
        let backend: Arc<dyn AuctionBackend> = Arc::new(StubAuctionBackend::new());
        let runner = ScenarioRunner::new(scen, backend);
        let result = runner.run().await.expect("run");
        assert!(
            result.passed,
            "scenario should pass; diffs={:?}",
            result.diffs
        );
        assert_eq!(result.name, "simple_banner");
    }

    #[tokio::test]
    async fn diff_with_ignore_paths_skips_ignored_field() {
        // Expected response has a different `ext.responsetimemillis` and a
        // different `id`, both of which should be ignored.
        let mut scen = simple_banner_scenario();
        scen.expected_response = json!({
            "id": "any-id",
            "seatbid": [{
                "bid": [{"id": "bid-1", "impid": "imp-1", "price": 1.5}]
            }],
            "ext": {"responsetimemillis": {"stub": 42}}
        });
        // Embed a matching-modulo-ignored-fields response in the request.
        scen.request = json!({
            "id": "req-1",
            "mock_response": {
                "id": "req-1",
                "seatbid": [{
                    "bid": [{"id": "bid-1", "impid": "imp-1", "price": 1.5}]
                }],
                "ext": {"responsetimemillis": {"stub": 9999}}
            }
        });
        let backend: Arc<dyn AuctionBackend> = Arc::new(StubAuctionBackend::new());
        let runner = ScenarioRunner::new(scen, backend);
        let result = runner.run().await.expect("run");
        assert!(
            result.passed,
            "ignored paths should be suppressed; diffs={:?}",
            result.diffs
        );
    }

    #[tokio::test]
    async fn fetcher_returns_stored_request_setup() {
        let mut scen = simple_banner_scenario();
        scen.setup.stored_requests.insert("sr-1".to_string(), json!({"tmax": 123}));
        let backend: Arc<dyn AuctionBackend> = Arc::new(StubAuctionBackend::new());
        let runner = ScenarioRunner::new(scen, backend);
        let fetcher = runner.fetcher();
        let (reqs, _imps, errs) = fetcher
            .fetch_requests(&["sr-1".to_string()], &[])
            .await;
        assert!(errs.is_empty());
        assert_eq!(reqs["sr-1"]["tmax"], 123);
    }
}
