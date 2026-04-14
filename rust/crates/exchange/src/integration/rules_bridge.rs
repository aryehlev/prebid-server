//! Rules bridge.
//!
//! Runs a `rules::Rules<RequestCtx, BidderCtx>` against a request and returns
//! the mutated output context. This is deliberately pinned to the canonical
//! `(RequestCtx, BidderCtx)` type pair exposed by the `rules` crate because
//! that is what exchange's existing bidder-filtering code path already
//! understands.

use rules::result_functions::BidderCtx;
use rules::schema_functions::RequestCtx;
use rules::{ResultFunctionMeta, Rules, RulesError};

/// Evaluate `rules` against `ctx` and return the resulting [`BidderCtx`]
/// together with the analytics metadata emitted during traversal.
///
/// On any error the original `BidderCtx::default()` is returned alongside a
/// `RulesError`, so callers can choose whether to surface the error or
/// transparently fall back to a no-op outcome.
pub fn evaluate_request_rules(
    rules: &Rules<RequestCtx, BidderCtx>,
    ctx: &RequestCtx,
) -> Result<(BidderCtx, ResultFunctionMeta), RulesError> {
    let mut out = BidderCtx::default();
    let meta = rules.evaluate(ctx, &mut out)?;
    Ok((out, meta))
}
