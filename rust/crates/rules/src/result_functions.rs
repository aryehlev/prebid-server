//! Built-in result functions. These mutate a [`BidderCtx`] when a leaf node
//! is reached during tree evaluation.
//!
//! Mirrors a small subset of Go `rules/result_functions.go`.

use crate::{ResultFunction, ResultFunctionMeta, RulesError};
use crate::schema_functions::RequestCtx;

/// Output context accumulated by result functions. In production this would
/// be a proper auction/bidder-filter context type; for the port we keep it
/// plain.
#[derive(Debug, Default, Clone)]
pub struct BidderCtx {
    pub excluded_bidders: Vec<String>,
    pub included_bidders: Vec<String>,
    pub rule_fired: String,
}

// ---------------------------------------------------------------------------
// excludeBidders
// ---------------------------------------------------------------------------

/// Appends its configured `bidders` to [`BidderCtx::excluded_bidders`].
pub struct ExcludeBidders {
    pub bidders: Vec<String>,
}

impl ResultFunction<RequestCtx, BidderCtx> for ExcludeBidders {
    fn name(&self) -> &str {
        "excludeBidders"
    }
    fn call(
        &self,
        _payload: &RequestCtx,
        out: &mut BidderCtx,
        meta: &ResultFunctionMeta,
    ) -> Result<(), RulesError> {
        for b in &self.bidders {
            out.excluded_bidders.push(b.clone());
        }
        out.rule_fired = meta.rule_fired.clone();
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// includeBidders
// ---------------------------------------------------------------------------

/// Appends its configured `bidders` to [`BidderCtx::included_bidders`].
pub struct IncludeBidders {
    pub bidders: Vec<String>,
}

impl ResultFunction<RequestCtx, BidderCtx> for IncludeBidders {
    fn name(&self) -> &str {
        "includeBidders"
    }
    fn call(
        &self,
        _payload: &RequestCtx,
        out: &mut BidderCtx,
        meta: &ResultFunctionMeta,
    ) -> Result<(), RulesError> {
        for b in &self.bidders {
            out.included_bidders.push(b.clone());
        }
        out.rule_fired = meta.rule_fired.clone();
        Ok(())
    }
}
