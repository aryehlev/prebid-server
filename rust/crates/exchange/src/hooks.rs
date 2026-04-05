use std::sync::Arc;
use serde_json::Value;

/// Hook stage identifiers (mirrors Go hookstage package)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HookStage {
    EntrypointRequest,        // Before auction request processing
    RawAuctionRequest,        // After parsing, before bidder calls
    ProcessedAuctionRequest,  // After FPD, before bidder calls
    BidderRequest,            // Per-bidder request
    RawBidderResponse,        // Per-bidder raw response
    AllProcessedBidResponses, // After all bidder responses
    AuctionResponse,          // Final response
}

/// Hook execution outcome
#[derive(Debug, Clone)]
pub enum HookOutcome {
    Continue,
    Reject { reason: String },
    Modify { payload: Value },
}

/// A single hook implementation
pub trait Hook: Send + Sync {
    fn stage(&self) -> HookStage;
    fn execute(&self, payload: &Value) -> HookOutcome;
}

/// Hook execution plan — ordered list of hooks per stage
pub struct HookExecutionPlan {
    hooks: Vec<Arc<dyn Hook>>,
}

impl HookExecutionPlan {
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }
    pub fn empty() -> Self {
        Self::new()
    }

    pub fn add_hook(&mut self, hook: Arc<dyn Hook>) {
        self.hooks.push(hook);
    }

    /// Execute all hooks for a given stage, returning final outcome
    pub fn execute_stage(&self, stage: &HookStage, payload: &Value) -> HookOutcome {
        let mut current = payload.clone();
        for hook in self.hooks.iter().filter(|h| &h.stage() == stage) {
            match hook.execute(&current) {
                HookOutcome::Continue => {}
                HookOutcome::Reject { reason } => return HookOutcome::Reject { reason },
                HookOutcome::Modify { payload } => current = payload,
            }
        }
        HookOutcome::Continue
    }
}

impl Default for HookExecutionPlan {
    fn default() -> Self {
        Self::new()
    }
}
