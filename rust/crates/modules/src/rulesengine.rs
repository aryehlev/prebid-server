//! Stub for the `rulesengine` hook module.
//!
//! Reads a JSON rules configuration and applies it to an in-flight auction.
//! This file deliberately does *not* depend on the (separate) `rules` crate —
//! instead it inlines a small stub mirror of the core abstractions so it can
//! be iterated on independently.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RulesEngineError {
    #[error("invalid rules config: {0}")]
    InvalidConfig(String),
    #[error("rule evaluation failed: {0}")]
    Evaluation(String),
}

/// Top-level config — mirrors the JSON you'd load from `host.yaml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RulesEngineConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub rulesets: Vec<RuleSet>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuleSet {
    pub stage: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub model_groups: Vec<ModelGroup>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelGroup {
    #[serde(default = "default_weight")]
    pub weight: u32,
    #[serde(default)]
    pub analytics_key: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub schema: Vec<SchemaEntry>,
    #[serde(default)]
    pub rules: Vec<RuleEntry>,
    #[serde(default)]
    pub default: Vec<ResultEntry>,
}

fn default_weight() -> u32 {
    100
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchemaEntry {
    pub function: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuleEntry {
    #[serde(default)]
    pub conditions: Vec<String>,
    #[serde(default)]
    pub results: Vec<ResultEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResultEntry {
    pub function: String,
    #[serde(default)]
    pub args: Value,
}

// ---------------------------------------------------------------------------
// Minimal in-crate stub of a decision tree
// ---------------------------------------------------------------------------

/// A trivial wrapper that remembers the parsed config and exposes an
/// `apply_to_auction` entry point. In the fully-wired version this would
/// build a real `rules::Tree` and evaluate it against the auction context.
pub struct RulesEngineModule {
    pub config: RulesEngineConfig,
}

/// Minimal auction-side snapshot used by the stub rules evaluation.
#[derive(Debug, Default, Clone)]
pub struct AuctionSnapshot {
    pub device_country: String,
    pub channel: String,
    pub bidders: Vec<String>,
}

/// Result of applying the rules engine to an auction.
#[derive(Debug, Default, Clone)]
pub struct RulesEngineOutcome {
    pub excluded_bidders: Vec<String>,
    pub included_bidders: Vec<String>,
    pub rules_fired: Vec<String>,
}

impl RulesEngineModule {
    pub fn from_json(raw: Value) -> Result<Self, RulesEngineError> {
        let config: RulesEngineConfig =
            serde_json::from_value(raw).map_err(|e| RulesEngineError::InvalidConfig(e.to_string()))?;
        Ok(Self { config })
    }

    pub fn new(config: RulesEngineConfig) -> Self {
        Self { config }
    }

    /// Apply the engine to the given auction snapshot. This stub walks the
    /// `rulesets` and runs a simple string-matching pass over their default
    /// result functions. It is intentionally simple — real evaluation will
    /// route through the `rules` crate.
    pub fn apply_to_auction(
        &self,
        snapshot: &AuctionSnapshot,
    ) -> Result<RulesEngineOutcome, RulesEngineError> {
        let mut outcome = RulesEngineOutcome::default();
        if !self.config.enabled {
            return Ok(outcome);
        }
        for rs in &self.config.rulesets {
            for group in &rs.model_groups {
                for entry in &group.default {
                    apply_result_entry(entry, snapshot, &mut outcome);
                }
                outcome.rules_fired.push(format!("{}:{}", rs.name, group.version));
            }
        }
        Ok(outcome)
    }
}

fn apply_result_entry(
    entry: &ResultEntry,
    _snapshot: &AuctionSnapshot,
    outcome: &mut RulesEngineOutcome,
) {
    match entry.function.as_str() {
        "excludeBidders" => {
            if let Some(arr) = entry.args.get("bidders").and_then(|v| v.as_array()) {
                for b in arr {
                    if let Some(s) = b.as_str() {
                        outcome.excluded_bidders.push(s.to_string());
                    }
                }
            }
        }
        "includeBidders" => {
            if let Some(arr) = entry.args.get("bidders").and_then(|v| v.as_array()) {
                for b in arr {
                    if let Some(s) = b.as_str() {
                        outcome.included_bidders.push(s.to_string());
                    }
                }
            }
        }
        other => {
            tracing::debug!("rulesengine: unknown result function {}", other);
        }
    }
}

/// Builder wiring so the host can construct the module from a generic JSON blob.
pub fn build(config: Value) -> Result<RulesEngineModule, String> {
    RulesEngineModule::from_json(config).map_err(|e| e.to_string())
}

/// Exposed for tests and diagnostics — returns the known result-function names.
pub fn known_result_functions() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    m.insert("excludeBidders", "exclude a list of bidders from the auction");
    m.insert("includeBidders", "restrict the auction to a list of bidders");
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_build_disabled_by_default() {
        let m = RulesEngineModule::from_json(json!({})).unwrap();
        assert!(!m.config.enabled);
        let outcome = m
            .apply_to_auction(&AuctionSnapshot::default())
            .unwrap();
        assert!(outcome.excluded_bidders.is_empty());
        assert!(outcome.rules_fired.is_empty());
    }

    #[test]
    fn test_apply_exclude_bidders() {
        let cfg = json!({
            "enabled": true,
            "rulesets": [{
                "stage": "processed_auction_request",
                "name": "demo",
                "version": "1",
                "model_groups": [{
                    "weight": 100,
                    "analytics_key": "k",
                    "version": "v1",
                    "schema": [],
                    "rules": [],
                    "default": [{
                        "function": "excludeBidders",
                        "args": {"bidders": ["appnexus", "rubicon"]}
                    }]
                }]
            }]
        });
        let m = RulesEngineModule::from_json(cfg).unwrap();
        let outcome = m
            .apply_to_auction(&AuctionSnapshot::default())
            .unwrap();
        assert_eq!(outcome.excluded_bidders, vec!["appnexus".to_string(), "rubicon".to_string()]);
        assert_eq!(outcome.rules_fired, vec!["demo:v1".to_string()]);
    }

    #[test]
    fn test_known_result_functions() {
        let m = known_result_functions();
        assert!(m.contains_key("excludeBidders"));
        assert!(m.contains_key("includeBidders"));
    }
}
