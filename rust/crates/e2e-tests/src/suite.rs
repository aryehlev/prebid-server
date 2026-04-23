//! Directory-based test suite.

use crate::backend::AuctionBackend;
use crate::error::E2eError;
use crate::runner::{ScenarioResult, ScenarioRunner};
use crate::scenario::Scenario;
use std::path::Path;
use std::sync::Arc;

/// Aggregate outcome of running every scenario in a directory.
#[derive(Debug, Clone, Default)]
pub struct SuiteResult {
    /// Total number of scenarios discovered.
    pub total: usize,
    /// Number of scenarios whose actual response matched their expected
    /// response modulo ignore paths.
    pub passed: usize,
    /// Number of scenarios that mismatched or errored.
    pub failed: usize,
    /// Individual [`ScenarioResult`] entries in discovery order.
    pub results: Vec<ScenarioResult>,
}

/// Loader/runner for a directory of `*.yaml` scenario files.
pub struct TestSuite {
    scenarios: Vec<Scenario>,
}

impl TestSuite {
    /// Discover every `*.yaml` / `*.yml` file under `dir` and parse them.
    ///
    /// Files are loaded in lexicographic order.
    pub fn load_from_dir(dir: &Path) -> Result<Self, E2eError> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)?
            .filter_map(Result::ok)
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|ext| ext == "yaml" || ext == "yml")
                    .unwrap_or(false)
            })
            .collect();
        entries.sort_by_key(|e| e.path());

        let mut scenarios = Vec::with_capacity(entries.len());
        for e in entries {
            scenarios.push(Scenario::load_from_path(&e.path())?);
        }
        Ok(Self { scenarios })
    }

    /// Number of loaded scenarios.
    pub fn len(&self) -> usize {
        self.scenarios.len()
    }

    /// Returns `true` if the suite contains no scenarios.
    pub fn is_empty(&self) -> bool {
        self.scenarios.is_empty()
    }

    /// Run every scenario sequentially against `backend` and tally the
    /// outcomes.
    pub async fn run(&self, backend: Arc<dyn AuctionBackend>) -> Result<SuiteResult, E2eError> {
        let mut out = SuiteResult {
            total: self.scenarios.len(),
            ..Default::default()
        };
        for scen in &self.scenarios {
            let runner = ScenarioRunner::new(scen.clone(), backend.clone());
            let result = runner.run().await?;
            if result.passed {
                out.passed += 1;
            } else {
                out.failed += 1;
            }
            out.results.push(result);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::StubAuctionBackend;
    use std::io::Write;

    fn write_fixture(dir: &Path, name: &str, body: &str) {
        let mut f = std::fs::File::create(dir.join(name)).unwrap();
        f.write_all(body.as_bytes()).unwrap();
    }

    const SIMPLE_BANNER: &str = r#"
name: simple_banner
request:
  id: "req-1"
  imp:
    - id: "imp-1"
      mock_response:
        id: "req-1"
        seatbid:
          - bid:
              - id: "bid-1"
                impid: "imp-1"
                price: 1.5
expected_response:
  id: "req-1"
  seatbid:
    - bid:
        - id: "bid-1"
          impid: "imp-1"
          price: 1.5
"#;

    const MISMATCH: &str = r#"
name: mismatching
request:
  id: "req-2"
  imp:
    - id: "imp-2"
      mock_response:
        id: "req-2"
        seatbid: []
expected_response:
  id: "req-2"
  seatbid:
    - bid:
        - id: "should-not-match"
          price: 9.99
"#;

    #[tokio::test]
    async fn suite_reports_pass_and_fail_counts() {
        let dir = tempfile::tempdir().unwrap();
        write_fixture(dir.path(), "a_simple.yaml", SIMPLE_BANNER);
        write_fixture(dir.path(), "b_mismatch.yaml", MISMATCH);
        // Non-yaml files should be ignored.
        write_fixture(dir.path(), "readme.txt", "ignore me");

        let suite = TestSuite::load_from_dir(dir.path()).expect("load");
        assert_eq!(suite.len(), 2);
        let backend: Arc<dyn AuctionBackend> = Arc::new(StubAuctionBackend::new());
        let result = suite.run(backend).await.expect("run");
        assert_eq!(result.total, 2);
        assert_eq!(result.passed, 1);
        assert_eq!(result.failed, 1);
        assert_eq!(result.results.len(), 2);
        assert!(result.results[0].passed);
        assert!(!result.results[1].passed);
    }
}
