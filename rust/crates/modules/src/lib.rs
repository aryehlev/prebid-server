//! Module and hook execution system for Prebid Server.
//!
//! Mirrors Go `hooks/` + `modules/` packages — provides hook stage definitions,
//! execution plans, mutation system, module builder, and the ortb2blocking module.

pub mod hookstage;
pub mod plan;
pub mod execution;
pub mod ortb2blocking;
pub mod rulesengine;
pub mod fiftyonedegrees;
pub mod scope3;

use std::collections::HashMap;
use serde_json::Value;

// ---------------------------------------------------------------------------
// Stage names — mirrors Go hooks/plan.go
// ---------------------------------------------------------------------------

/// Names of the available hook stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    Entrypoint,
    RawAuctionRequest,
    ProcessedAuctionRequest,
    BidderRequest,
    RawBidderResponse,
    AllProcessedBidResponses,
    AuctionResponse,
    Exitpoint,
}

impl Stage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Stage::Entrypoint => "entrypoint",
            Stage::RawAuctionRequest => "raw_auction_request",
            Stage::ProcessedAuctionRequest => "processed_auction_request",
            Stage::BidderRequest => "bidder_request",
            Stage::RawBidderResponse => "raw_bidder_response",
            Stage::AllProcessedBidResponses => "all_processed_bid_responses",
            Stage::AuctionResponse => "auction_response",
            Stage::Exitpoint => "exitpoint",
        }
    }

    /// Whether this stage can reject a request.
    pub fn is_rejectable(&self) -> bool {
        !matches!(
            self,
            Stage::AllProcessedBidResponses | Stage::AuctionResponse | Stage::Exitpoint
        )
    }
}

impl std::fmt::Display for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Module builder — mirrors Go modules/modules.go
// ---------------------------------------------------------------------------

/// A module builder function creates a module instance from its JSON config.
pub type ModuleBuilderFn = Box<dyn Fn(Value) -> Result<Box<dyn Module>, String> + Send + Sync>;

/// Module is the base trait that all hook modules must implement.
/// Concrete modules also implement one or more hook stage traits.
pub trait Module: Send + Sync {
    /// Return the stages this module provides hooks for.
    fn stages(&self) -> Vec<Stage>;
}

/// ModuleBuilders maps vendor.module names to their builder functions.
pub type ModuleBuilders = HashMap<String, HashMap<String, ModuleBuilderFn>>;

/// Build all enabled modules from configuration.
///
/// Mirrors Go `builder.Build()`.
pub fn build_modules(
    cfg: &HashMap<String, HashMap<String, Value>>,
    builders: &ModuleBuilders,
) -> Result<(HashMap<String, Box<dyn Module>>, HashMap<String, Vec<String>>), String> {
    let mut modules: HashMap<String, Box<dyn Module>> = HashMap::new();
    let mut stage_map: HashMap<String, Vec<String>> = HashMap::new();

    for (vendor, module_builders) in builders {
        for (module_name, builder) in module_builders {
            let id = format!("{}.{}", vendor, module_name);

            // Get config for this module
            let module_cfg = cfg
                .get(vendor)
                .and_then(|v| v.get(module_name))
                .cloned()
                .unwrap_or(Value::Null);

            // Check if enabled
            let is_enabled = module_cfg
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if !is_enabled {
                tracing::info!("Skip {} module, disabled.", id);
                continue;
            }

            let module = builder(module_cfg).map_err(|e| format!("failed to init \"{}\": {}", id, e))?;

            // Record which stages this module provides hooks for
            let stages: Vec<String> = module.stages().iter().map(|s| s.to_string()).collect();
            stage_map.insert(id.clone(), stages);

            modules.insert(id, module);
        }
    }

    Ok((modules, stage_map))
}

// ---------------------------------------------------------------------------
// HookID — identifies a specific hook invocation
// ---------------------------------------------------------------------------

/// Identifies a specific hook within a module.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HookID {
    pub module_code: String,
    pub hook_impl_code: String,
}

impl std::fmt::Display for HookID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.module_code, self.hook_impl_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stage_names() {
        assert_eq!(Stage::Entrypoint.as_str(), "entrypoint");
        assert_eq!(Stage::BidderRequest.as_str(), "bidder_request");
        assert_eq!(Stage::Exitpoint.as_str(), "exitpoint");
    }

    #[test]
    fn test_stage_rejectable() {
        assert!(Stage::Entrypoint.is_rejectable());
        assert!(Stage::BidderRequest.is_rejectable());
        assert!(!Stage::AllProcessedBidResponses.is_rejectable());
        assert!(!Stage::AuctionResponse.is_rejectable());
        assert!(!Stage::Exitpoint.is_rejectable());
    }

    #[test]
    fn test_build_modules_disabled() {
        let cfg: HashMap<String, HashMap<String, Value>> = HashMap::new();
        let builders: ModuleBuilders = HashMap::new();
        let (modules, _) = build_modules(&cfg, &builders).unwrap();
        assert!(modules.is_empty());
    }

    #[test]
    fn test_hook_id_display() {
        let id = HookID {
            module_code: "prebid.ortb2blocking".to_string(),
            hook_impl_code: "block-badv".to_string(),
        };
        assert_eq!(id.to_string(), "prebid.ortb2blocking.block-badv");
    }
}
