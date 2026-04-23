//! Rules engine hook module — wires the standalone `rules` crate into the
//! module pipeline.
//!
//! A [`RulesEngineModule`] owns a [`rules::Rules`] built from JSON
//! configuration. When [`apply_to_auction`] is invoked the module converts
//! an [`AuctionSnapshot`] into the `rules` crate's [`RequestCtx`], runs the
//! tree, and lifts the resulting [`BidderCtx`] back into a
//! [`RulesEngineOutcome`] consumable by the rest of the server.
//!
//! Configuration format (JSON):
//!
//! ```json
//! {
//!   "enabled": true,
//!   "rulesets": [{
//!     "stage": "processed_auction_request",
//!     "name": "demo",
//!     "version": "v1",
//!     "model_groups": [{
//!       "analytics_key": "k",
//!       "version": "v1",
//!       "schema": [{"function": "deviceCountry"}],
//!       "rules": [
//!         {
//!           "conditions": ["US"],
//!           "results": [{"function": "excludeBidders", "args": {"bidders": ["appnexus"]}}]
//!         }
//!       ],
//!       "default": [{"function": "includeBidders", "args": {"bidders": ["rubicon"]}}]
//!     }]
//!   }]
//! }
//! ```

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use rules::result_functions::{BidderCtx, ExcludeBidders, IncludeBidders};
use rules::schema_functions::{Channel, DeviceCountry, DeviceType, RequestCtx};
use rules::{Node, ResultFunction, Rules, SchemaFunction, Tree};

// ---------------------------------------------------------------------------
// Public errors & config structs (compatible with the prior stub API)
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum RulesEngineError {
    #[error("invalid rules config: {0}")]
    InvalidConfig(String),
    #[error("rule evaluation failed: {0}")]
    Evaluation(String),
}

/// Legacy alias so callers referring to `ModuleError` also resolve.
pub type ModuleError = RulesEngineError;

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
    /// Ordered list of values that correspond to the `schema` of the owning
    /// model group: `conditions[i]` is the value the `schema[i]` function is
    /// expected to return for this rule to fire. The special value `"*"`
    /// acts as a wildcard.
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

/// Minimal auction-side snapshot.
#[derive(Debug, Default, Clone)]
pub struct AuctionSnapshot {
    pub device_country: String,
    pub channel: String,
    pub bidders: Vec<String>,
    pub device_type: String,
}

/// Result of applying the rules engine to an auction.
#[derive(Debug, Default, Clone)]
pub struct RulesEngineOutcome {
    pub excluded_bidders: Vec<String>,
    pub included_bidders: Vec<String>,
    pub rules_fired: Vec<String>,
}

// ---------------------------------------------------------------------------
// Compiled ruleset
// ---------------------------------------------------------------------------

struct CompiledRuleset {
    name: String,
    version: String,
    /// One [`Rules`] tree per model group. In this port we just iterate them
    /// all rather than performing weighted selection.
    groups: Vec<Rules<RequestCtx, BidderCtx>>,
}

pub struct RulesEngineModule {
    pub config: RulesEngineConfig,
    rulesets: Vec<CompiledRuleset>,
}

impl RulesEngineModule {
    /// Parse a JSON blob into a configured module.
    pub fn from_json(raw: Value) -> Result<Self, RulesEngineError> {
        let config: RulesEngineConfig = serde_json::from_value(raw)
            .map_err(|e| RulesEngineError::InvalidConfig(e.to_string()))?;
        Self::from_config(config)
    }

    /// Parse a JSON string into a configured module.
    pub fn from_json_str(raw: &str) -> Result<Self, RulesEngineError> {
        let v: Value = serde_json::from_str(raw)
            .map_err(|e| RulesEngineError::InvalidConfig(e.to_string()))?;
        Self::from_json(v)
    }

    /// Construct from an already-parsed [`RulesEngineConfig`].
    pub fn from_config(config: RulesEngineConfig) -> Result<Self, RulesEngineError> {
        let mut rulesets = Vec::with_capacity(config.rulesets.len());
        for rs in &config.rulesets {
            let mut groups = Vec::with_capacity(rs.model_groups.len());
            for mg in &rs.model_groups {
                let tree = build_tree(mg)?;
                groups.push(Rules::new(tree));
            }
            rulesets.push(CompiledRuleset {
                name: rs.name.clone(),
                version: rs.version.clone(),
                groups,
            });
        }
        Ok(Self { config, rulesets })
    }

    /// Construct directly from a pre-built [`Rules`] tree — primarily useful
    /// for tests.
    pub fn from_rules(rules: Rules<RequestCtx, BidderCtx>) -> Self {
        let compiled = CompiledRuleset {
            name: "inline".to_string(),
            version: "inline".to_string(),
            groups: vec![rules],
        };
        Self {
            config: RulesEngineConfig {
                enabled: true,
                ..Default::default()
            },
            rulesets: vec![compiled],
        }
    }

    pub fn new(config: RulesEngineConfig) -> Self {
        Self::from_config(config).unwrap_or_else(|_| Self {
            config: RulesEngineConfig::default(),
            rulesets: Vec::new(),
        })
    }

    /// Evaluate every compiled ruleset against `snapshot` and return the
    /// accumulated outcome.
    pub fn apply_to_auction(
        &self,
        snapshot: &AuctionSnapshot,
    ) -> Result<RulesEngineOutcome, RulesEngineError> {
        let mut outcome = RulesEngineOutcome::default();
        if !self.config.enabled {
            return Ok(outcome);
        }
        let ctx = RequestCtx {
            device_country: snapshot.device_country.clone(),
            channel: snapshot.channel.clone(),
            device_type: snapshot.device_type.clone(),
        };
        for compiled in &self.rulesets {
            for rules in &compiled.groups {
                let mut out = BidderCtx::default();
                let meta = rules
                    .evaluate(&ctx, &mut out)
                    .map_err(|e| RulesEngineError::Evaluation(e.to_string()))?;
                outcome.excluded_bidders.extend(out.excluded_bidders);
                outcome.included_bidders.extend(out.included_bidders);
                outcome.rules_fired.push(format!(
                    "{}:{}:{}",
                    compiled.name, compiled.version, meta.rule_fired
                ));
            }
        }
        Ok(outcome)
    }
}

// ---------------------------------------------------------------------------
// Tree builder
// ---------------------------------------------------------------------------

fn build_tree(group: &ModelGroup) -> Result<Tree<RequestCtx, BidderCtx>, RulesEngineError> {
    // Collect the schema functions for this model group.
    let schema_fns: Vec<Box<dyn SchemaFunction<RequestCtx>>> = group
        .schema
        .iter()
        .map(|e| build_schema_function(&e.function))
        .collect::<Result<_, _>>()?;

    // Default result functions (used when no rule matches).
    let default_fns: Vec<Box<dyn ResultFunction<RequestCtx, BidderCtx>>> = group
        .default
        .iter()
        .map(build_result_function)
        .collect::<Result<_, _>>()?;

    // If there is no schema we still need a trivial leaf that executes the
    // default functions.
    if schema_fns.is_empty() {
        let mut tree = Tree::new();
        tree.analytics_key = group.analytics_key.clone();
        tree.model_version = group.version.clone();
        tree.default_functions = default_fns;
        tree.root = Some(Node::new());
        return Ok(tree);
    }

    // Build an internal-node-only root, plus leaves for each configured rule.
    let mut root: Node<RequestCtx, BidderCtx> = Node::new();
    root.schema_function = Some(take_boxed(&schema_fns, 0));

    for rule in &group.rules {
        if rule.conditions.len() != schema_fns.len() {
            return Err(RulesEngineError::InvalidConfig(format!(
                "rule conditions length ({}) does not match schema length ({})",
                rule.conditions.len(),
                schema_fns.len()
            )));
        }
        let leaf_funcs: Vec<Box<dyn ResultFunction<RequestCtx, BidderCtx>>> = rule
            .results
            .iter()
            .map(build_result_function)
            .collect::<Result<_, _>>()?;
        insert_rule(&mut root, &schema_fns, &rule.conditions, 0, leaf_funcs)?;
    }

    // If no rules were inserted, collapse the root into a leaf so evaluation
    // falls through to the default functions.
    if root.children.is_empty() {
        root = Node::new();
    }

    let mut tree = Tree::new();
    tree.root = Some(root);
    tree.default_functions = default_fns;
    tree.analytics_key = group.analytics_key.clone();
    tree.model_version = group.version.clone();
    tree.validate()
        .map_err(|e| RulesEngineError::InvalidConfig(e.to_string()))?;
    Ok(tree)
}

/// Return a freshly-boxed schema function for the index-th schema slot.
/// We cannot clone the boxed trait objects, so we re-construct them on demand.
fn take_boxed(
    schema_fns: &[Box<dyn SchemaFunction<RequestCtx>>],
    idx: usize,
) -> Box<dyn SchemaFunction<RequestCtx>> {
    build_schema_function(schema_fns[idx].name())
        .expect("schema function name came from build_schema_function")
}

fn insert_rule(
    node: &mut Node<RequestCtx, BidderCtx>,
    schema_fns: &[Box<dyn SchemaFunction<RequestCtx>>],
    conditions: &[String],
    depth: usize,
    leaf_funcs: Vec<Box<dyn ResultFunction<RequestCtx, BidderCtx>>>,
) -> Result<(), RulesEngineError> {
    if node.schema_function.is_none() {
        node.schema_function = Some(take_boxed(schema_fns, depth));
    }
    let key = conditions[depth].clone();
    let is_last = depth + 1 == conditions.len();
    let child = node.children.entry(key).or_insert_with(Node::new);
    if is_last {
        child.result_functions = leaf_funcs;
        Ok(())
    } else {
        insert_rule(child, schema_fns, conditions, depth + 1, leaf_funcs)
    }
}

fn build_schema_function(
    name: &str,
) -> Result<Box<dyn SchemaFunction<RequestCtx>>, RulesEngineError> {
    match name {
        "deviceCountry" => Ok(Box::new(DeviceCountry)),
        "channel" => Ok(Box::new(Channel)),
        "deviceType" => Ok(Box::new(DeviceType)),
        other => Err(RulesEngineError::InvalidConfig(format!(
            "unknown schema function: {other}"
        ))),
    }
}

fn build_result_function(
    entry: &ResultEntry,
) -> Result<Box<dyn ResultFunction<RequestCtx, BidderCtx>>, RulesEngineError> {
    let bidders = entry
        .args
        .get("bidders")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    match entry.function.as_str() {
        "excludeBidders" => Ok(Box::new(ExcludeBidders { bidders })),
        "includeBidders" => Ok(Box::new(IncludeBidders { bidders })),
        other => Err(RulesEngineError::InvalidConfig(format!(
            "unknown result function: {other}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Builder wiring
// ---------------------------------------------------------------------------

/// Construct a module from a JSON blob, returning a stringy error so the
/// host loader can propagate it without depending on this crate's error type.
pub fn build(config: Value) -> Result<RulesEngineModule, String> {
    RulesEngineModule::from_json(config).map_err(|e| e.to_string())
}

/// Exposed for tests and diagnostics.
pub fn known_result_functions() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    m.insert("excludeBidders", "exclude a list of bidders from the auction");
    m.insert("includeBidders", "restrict the auction to a list of bidders");
    m
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_known_result_functions() {
        let m = known_result_functions();
        assert!(m.contains_key("excludeBidders"));
        assert!(m.contains_key("includeBidders"));
    }

    #[test]
    fn test_disabled_by_default() {
        let m = RulesEngineModule::from_json(json!({})).unwrap();
        assert!(!m.config.enabled);
        let outcome = m.apply_to_auction(&AuctionSnapshot::default()).unwrap();
        assert!(outcome.excluded_bidders.is_empty());
        assert!(outcome.rules_fired.is_empty());
    }

    #[test]
    fn test_from_json_builds_tree_and_matches_country() {
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
                    "schema": [{"function": "deviceCountry"}],
                    "rules": [
                        {
                            "conditions": ["US"],
                            "results": [{"function": "excludeBidders", "args": {"bidders": ["appnexus"]}}]
                        },
                        {
                            "conditions": ["*"],
                            "results": [{"function": "includeBidders", "args": {"bidders": ["rubicon"]}}]
                        }
                    ],
                    "default": []
                }]
            }]
        });
        let m = RulesEngineModule::from_json(cfg).unwrap();

        // US -> appnexus excluded.
        let us = AuctionSnapshot {
            device_country: "US".into(),
            ..Default::default()
        };
        let outcome = m.apply_to_auction(&us).unwrap();
        assert_eq!(outcome.excluded_bidders, vec!["appnexus".to_string()]);
        assert!(outcome.included_bidders.is_empty());
        assert_eq!(outcome.rules_fired, vec!["demo:1:US".to_string()]);

        // FR -> wildcard -> rubicon included.
        let fr = AuctionSnapshot {
            device_country: "FR".into(),
            ..Default::default()
        };
        let outcome = m.apply_to_auction(&fr).unwrap();
        assert_eq!(outcome.included_bidders, vec!["rubicon".to_string()]);
        assert_eq!(outcome.rules_fired, vec!["demo:1:*".to_string()]);
    }

    #[test]
    fn test_manual_tree_via_from_rules() {
        // Build a trivial tree by hand:
        //   root [channel] -> { "web" => exclude(appnexus), "*" => include(rubicon) }
        let mut leaf_web: Node<RequestCtx, BidderCtx> = Node::new();
        leaf_web.result_functions.push(Box::new(ExcludeBidders {
            bidders: vec!["appnexus".into()],
        }));
        let mut leaf_other: Node<RequestCtx, BidderCtx> = Node::new();
        leaf_other.result_functions.push(Box::new(IncludeBidders {
            bidders: vec!["rubicon".into()],
        }));

        let mut root: Node<RequestCtx, BidderCtx> = Node::new();
        root.schema_function = Some(Box::new(Channel));
        root.children.insert("web".into(), leaf_web);
        root.children.insert("*".into(), leaf_other);

        let mut tree = Tree::new();
        tree.root = Some(root);
        tree.analytics_key = "inline".into();
        tree.model_version = "v-test".into();
        tree.validate().unwrap();

        let module = RulesEngineModule::from_rules(Rules::new(tree));

        let web = AuctionSnapshot {
            channel: "web".into(),
            ..Default::default()
        };
        let outcome = module.apply_to_auction(&web).unwrap();
        assert_eq!(outcome.excluded_bidders, vec!["appnexus".to_string()]);
        assert_eq!(outcome.rules_fired, vec!["inline:inline:web".to_string()]);

        let app = AuctionSnapshot {
            channel: "app".into(),
            ..Default::default()
        };
        let outcome = module.apply_to_auction(&app).unwrap();
        assert_eq!(outcome.included_bidders, vec!["rubicon".to_string()]);
    }

    #[test]
    fn test_default_functions_fire_when_no_rules_match() {
        let cfg = json!({
            "enabled": true,
            "rulesets": [{
                "stage": "processed_auction_request",
                "name": "defaults",
                "version": "v1",
                "model_groups": [{
                    "weight": 100,
                    "analytics_key": "k",
                    "version": "v1",
                    "schema": [{"function": "channel"}],
                    "rules": [
                        {
                            "conditions": ["web"],
                            "results": [{"function": "excludeBidders", "args": {"bidders": ["adx"]}}]
                        }
                    ],
                    "default": [{"function": "includeBidders", "args": {"bidders": ["fallback"]}}]
                }]
            }]
        });
        let m = RulesEngineModule::from_json(cfg).unwrap();
        let app = AuctionSnapshot {
            channel: "app".into(),
            ..Default::default()
        };
        let outcome = m.apply_to_auction(&app).unwrap();
        assert_eq!(outcome.included_bidders, vec!["fallback".to_string()]);
        assert_eq!(outcome.excluded_bidders, Vec::<String>::new());
        assert_eq!(outcome.rules_fired, vec!["defaults:v1:default".to_string()]);
    }

    #[test]
    fn test_invalid_schema_function_rejected() {
        let cfg = json!({
            "enabled": true,
            "rulesets": [{
                "stage": "s",
                "name": "n",
                "version": "v",
                "model_groups": [{
                    "schema": [{"function": "notAThing"}],
                    "rules": [{"conditions": ["x"], "results": []}]
                }]
            }]
        });
        match RulesEngineModule::from_json(cfg) {
            Err(RulesEngineError::InvalidConfig(_)) => {}
            Err(other) => panic!("unexpected error: {other:?}"),
            Ok(_) => panic!("expected config rejection"),
        }
    }
}
