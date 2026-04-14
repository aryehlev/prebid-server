//! Hooks bridge.
//!
//! Wraps a [`hooks::HookExecutor`] together with a [`hooks::HookExecutionPlan`]
//! into a reusable runner. This lets exchange call sites hand the runner a
//! payload and receive back the possibly-mutated payload plus a stage
//! execution summary, without having to thread the executor and plan
//! independently through their code.
//!
//! The wrapper is generic over the payload type `P` exactly as
//! `hooks::HookExecutor::execute` is.

use hooks::executor::StageExecutionSummary;
use hooks::plan::HookExecutionPlan;
use hooks::reject::HookError;
use hooks::HookExecutor;

/// Result of running an [`AuctionHookRunner`] over a payload. Mirrors the
/// tuple returned by [`HookExecutor::execute`] but exposes it as a named
/// struct so callers can destructure cleanly.
pub struct HookRunOutcome<P> {
    /// The payload after all successful hook mutations have been applied.
    pub payload: P,
    /// Summary describing how many hooks ran / were rejected / timed out.
    pub summary: StageExecutionSummary,
    /// Any fatal `HookError` that short-circuited the plan (e.g. a reject).
    pub error: Option<HookError>,
}

/// A small wrapper that holds both a [`HookExecutor`] and the
/// [`HookExecutionPlan`] it should run. Exchange call sites construct one
/// per stage at request time and then call [`AuctionHookRunner::run`] with
/// the current payload.
pub struct AuctionHookRunner<P>
where
    P: Clone + Send + 'static,
{
    executor: HookExecutor,
    plan: HookExecutionPlan<P>,
}

impl<P> AuctionHookRunner<P>
where
    P: Clone + Send + 'static,
{
    /// Construct a runner from an executor and a plan.
    pub fn new(executor: HookExecutor, plan: HookExecutionPlan<P>) -> Self {
        Self { executor, plan }
    }

    /// Access the underlying plan, mostly for test/debug introspection.
    pub fn plan(&self) -> &HookExecutionPlan<P> {
        &self.plan
    }

    /// Drive the plan to completion against `payload`.
    ///
    /// This is a thin wrapper over [`HookExecutor::execute`] that repacks its
    /// `(payload, summary, error)` triple into a [`HookRunOutcome`].
    pub async fn run(&self, payload: P) -> HookRunOutcome<P> {
        let (payload, summary, error) = self.executor.execute(&self.plan, payload).await;
        HookRunOutcome {
            payload,
            summary,
            error,
        }
    }
}
