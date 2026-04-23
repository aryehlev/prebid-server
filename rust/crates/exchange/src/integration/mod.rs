//! Integration bridges wiring the standalone `analytics`, `hooks`, `metrics`,
//! and `rules` crates into the `pbs-exchange` auction pipeline.
//!
//! The bridges here are strictly additive: they expose thin, opt-in helpers
//! that existing exchange code can call to emit events, execute hook plans,
//! apply rules, or record metrics without any of the pipeline code needing to
//! know the concrete types in those upstream crates. None of the existing
//! auction/amp/cookie-sync/setuid files are modified — callers wire this in
//! at the boundary as they see fit.

pub mod analytics_bridge;
pub mod hooks_bridge;
pub mod metrics_bridge;
pub mod rules_bridge;

pub use analytics_bridge::{
    emit_amp_event, emit_auction_event, emit_cookie_sync_event, emit_notification_event,
    emit_setuid_event,
};
pub use hooks_bridge::AuctionHookRunner;
pub use metrics_bridge::{
    record_adapter_call, record_auction_duration, record_auction_request, record_cache_op,
};
pub use rules_bridge::evaluate_request_rules;

#[cfg(test)]
mod tests;
