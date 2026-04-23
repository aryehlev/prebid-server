//! Scenario: a single YAML-driven end-to-end test case.

use crate::error::E2eError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// In-memory "setup" section of a scenario file.
///
/// Each map's key is the stored-ID; the value is the raw JSON body that the
/// in-memory fetcher should hand back when that ID is requested.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ScenarioSetup {
    /// Stored request bodies keyed by stored request ID.
    #[serde(default)]
    pub stored_requests: HashMap<String, Value>,
    /// Stored imp bodies keyed by stored imp ID.
    #[serde(default)]
    pub stored_imps: HashMap<String, Value>,
    /// Account configs keyed by account ID.
    #[serde(default)]
    pub accounts: HashMap<String, Value>,
    /// Canned bid responses keyed by bidder name (consumed by a stub
    /// backend).
    #[serde(default)]
    pub bid_responses: HashMap<String, Value>,
}

/// A YAML-deserialized test case.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Scenario {
    /// Human-readable scenario name; used as the `ScenarioResult::name`.
    pub name: String,
    /// Optional prose description of what the scenario is validating.
    #[serde(default)]
    pub description: Option<String>,
    /// The OpenRTB bid request sent to the [`crate::AuctionBackend`].
    pub request: Value,
    /// The expected OpenRTB response; compared against the backend output via
    /// [`parity::diff::diff`].
    pub expected_response: Value,
    /// Warnings the runner is allowed to surface without failing.
    #[serde(default)]
    pub expected_warnings: Vec<String>,
    /// In-memory priming data for the auction pipeline.
    #[serde(default)]
    pub setup: ScenarioSetup,
}

impl Scenario {
    /// Parse a scenario from a YAML string.
    pub fn load_from_yaml(s: &str) -> Result<Self, E2eError> {
        let scenario: Scenario = serde_yaml::from_str(s)?;
        Ok(scenario)
    }

    /// Load a scenario file from disk.
    pub fn load_from_path(p: &Path) -> Result<Self, E2eError> {
        let s = std::fs::read_to_string(p)?;
        Self::load_from_yaml(&s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CANNED: &str = r#"
name: canned
description: "A canned scenario"
request:
  id: "req-1"
  imp:
    - id: "imp-1"
expected_response:
  id: "req-1"
  seatbid: []
expected_warnings: []
setup:
  stored_requests: {}
  stored_imps: {}
  accounts: {}
  bid_responses: {}
"#;

    #[test]
    fn load_from_yaml_parses_canned_string() {
        let s = Scenario::load_from_yaml(CANNED).expect("parse");
        assert_eq!(s.name, "canned");
        assert_eq!(s.description.as_deref(), Some("A canned scenario"));
        assert_eq!(s.request["id"], "req-1");
        assert_eq!(s.expected_response["id"], "req-1");
        assert!(s.expected_warnings.is_empty());
        assert!(s.setup.stored_requests.is_empty());
    }

    #[test]
    fn load_from_yaml_allows_missing_setup_and_description() {
        let s = r#"
name: minimal
request: {id: "r"}
expected_response: {id: "r"}
"#;
        let parsed = Scenario::load_from_yaml(s).expect("parse minimal");
        assert_eq!(parsed.name, "minimal");
        assert!(parsed.description.is_none());
        assert!(parsed.setup.stored_requests.is_empty());
    }
}
