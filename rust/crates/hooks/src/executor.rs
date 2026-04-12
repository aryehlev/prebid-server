//! Hook execution engine.
//!
//! The `HookExecutor` is a small asynchronous runner that walks a
//! `HookExecutionPlan`, invoking every hook in each group and feeding the
//! returned mutations back into the payload before advancing to the next
//! group. It implements timeouts on a per-hook basis and surfaces the first
//! rejection encountered in a group.

use std::time::Duration;

use tokio::time::timeout;
use tracing::warn;

use crate::hook::{HookResult, HookWrapper, ModuleInvocationContext};
use crate::plan::{Group, HookExecutionPlan};
use crate::reject::{HookError, HookId, Reject};
use crate::stage::Stage;

/// High-level summary of a stage execution.
#[derive(Debug, Default, Clone)]
pub struct StageExecutionSummary {
    /// Number of hooks that ran to completion successfully.
    pub successful_hooks: usize,
    /// Number of hooks that reported warnings.
    pub hooks_with_warnings: usize,
    /// Number of hooks that reported errors.
    pub hooks_with_errors: usize,
    /// Number of hooks that timed out.
    pub timed_out_hooks: usize,
    /// Number of mutations applied to the payload.
    pub mutations_applied: usize,
    /// Whether the pipeline was ultimately rejected at this stage.
    pub rejected: bool,
}

/// A `HookExecutor` runs a `HookExecutionPlan` against a payload.
pub struct HookExecutor {
    stage: Stage,
    account_id: Option<String>,
    endpoint: String,
}

impl HookExecutor {
    /// Construct a new executor for a particular stage.
    pub fn new(stage: Stage) -> Self {
        Self {
            stage,
            account_id: None,
            endpoint: String::new(),
        }
    }

    /// Attach an account identifier to the executor.
    pub fn with_account_id(mut self, account_id: impl Into<String>) -> Self {
        self.account_id = Some(account_id.into());
        self
    }

    /// Attach the endpoint path that this execution belongs to.
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    /// Run a plan to completion on the given payload.
    ///
    /// Returns the (possibly mutated) payload alongside a summary of the
    /// execution. Any rejection (supplied by a hook that ran to completion
    /// at a rejectable stage) is returned as `HookError::Rejected`.
    pub async fn execute<P>(
        &self,
        plan: &HookExecutionPlan<P>,
        mut payload: P,
    ) -> (P, StageExecutionSummary, Option<HookError>)
    where
        P: Clone + Send + 'static,
    {
        let mut summary = StageExecutionSummary::default();

        for group in &plan.groups {
            let (new_payload, rejected) = self
                .execute_group(group, payload, &mut summary)
                .await;
            payload = new_payload;
            if let Some(err) = rejected {
                summary.rejected = true;
                return (payload, summary, Some(err));
            }
        }

        (payload, summary, None)
    }

    async fn execute_group<P>(
        &self,
        group: &Group<P>,
        payload: P,
        summary: &mut StageExecutionSummary,
    ) -> (P, Option<HookError>)
    where
        P: Clone + Send + 'static,
    {
        let mut current = payload;

        for hook in &group.hooks {
            let ctx = ModuleInvocationContext {
                account_id: self.account_id.clone(),
                endpoint: self.endpoint.clone(),
                hook_impl_code: hook.code.clone(),
                ..Default::default()
            };

            let fut = hook.hook.handle(&ctx, current.clone());
            let result = match timeout(group.timeout, fut).await {
                Ok(Ok(result)) => result,
                Ok(Err(e)) => {
                    match &e {
                        HookError::Timeout => summary.timed_out_hooks += 1,
                        _ => summary.hooks_with_errors += 1,
                    }
                    warn!(module = %hook.module, code = %hook.code, error = %e, "hook failed");
                    // If the hook returned a Reject error at a rejectable stage, honour it.
                    if matches!(e, HookError::Rejected(_)) && self.stage.is_rejectable() {
                        return (current, Some(e));
                    }
                    continue;
                }
                Err(_) => {
                    summary.timed_out_hooks += 1;
                    warn!(module = %hook.module, code = %hook.code, "hook timed out");
                    continue;
                }
            };

            current = self.apply_result(result, current, hook, summary);
            if summary.rejected {
                // Rejection was surfaced by the result; stop processing the group.
                let reject = Reject::new(
                    self.stage,
                    HookId {
                        module_code: hook.module.clone(),
                        hook_impl_code: hook.code.clone(),
                    },
                    0,
                );
                return (current, Some(HookError::Rejected(reject)));
            }
        }

        (current, None)
    }

    fn apply_result<P>(
        &self,
        result: HookResult<P>,
        payload: P,
        hook: &HookWrapper<P>,
        summary: &mut StageExecutionSummary,
    ) -> P
    where
        P: Clone + Send + 'static,
    {
        if !result.errors.is_empty() {
            summary.hooks_with_errors += 1;
        }
        if !result.warnings.is_empty() {
            summary.hooks_with_warnings += 1;
        }

        // A hook cannot reject at a non-rejectable stage. If it tries, we
        // downgrade the reject into a warning, mirroring the Go executor.
        if result.reject {
            if self.stage.is_rejectable() {
                summary.rejected = true;
                return payload;
            } else {
                warn!(
                    module = %hook.module,
                    code = %hook.code,
                    stage = %self.stage,
                    "hook attempted to reject at non-rejectable stage; ignoring"
                );
            }
        }

        let (updated, errs) = result.changeset.apply_all(payload);
        summary.mutations_applied += result.changeset.len().saturating_sub(errs.len());
        if !errs.is_empty() {
            summary.hooks_with_errors += 1;
        } else {
            summary.successful_hooks += 1;
        }
        updated
    }

    /// Convenience method to explicitly create a rejection tied to this
    /// executor's current stage.
    pub fn reject(&self, hook: HookId, nbr: i32) -> Reject {
        Reject::new(self.stage, hook, nbr)
    }

    /// Duration representing "no timeout"; useful for tests.
    pub const fn no_timeout() -> Duration {
        Duration::from_secs(24 * 60 * 60)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use async_trait::async_trait;

    use super::*;
    use crate::changeset::{ChangeSet, MutationType};
    use crate::hook::{Hook, HookResult, HookWrapper, ModuleInvocationContext};
    use crate::plan::{Group, HookExecutionPlan};

    struct IncrementHook;

    #[async_trait]
    impl Hook<i64> for IncrementHook {
        async fn handle(
            &self,
            _ctx: &ModuleInvocationContext,
            payload: i64,
        ) -> Result<HookResult<i64>, HookError> {
            let mut result = HookResult::new();
            result.changeset.add_mutation(
                |v| Ok(v + 1),
                MutationType::Update,
                vec!["value".to_string()],
            );
            let _ = payload;
            Ok(result)
        }
    }

    struct RejectingHook;

    #[async_trait]
    impl Hook<i64> for RejectingHook {
        async fn handle(
            &self,
            _ctx: &ModuleInvocationContext,
            _payload: i64,
        ) -> Result<HookResult<i64>, HookError> {
            Ok(HookResult::rejected(42, "not allowed"))
        }
    }

    struct SlowHook;

    #[async_trait]
    impl Hook<i64> for SlowHook {
        async fn handle(
            &self,
            _ctx: &ModuleInvocationContext,
            payload: i64,
        ) -> Result<HookResult<i64>, HookError> {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let mut r = HookResult::new();
            let _ = payload;
            r.changeset.add_mutation(
                |v| Ok(v + 100),
                MutationType::Update,
                vec!["value".to_string()],
            );
            Ok(r)
        }
    }

    fn single_group_plan(hook: Arc<dyn Hook<i64>>, timeout: Duration) -> HookExecutionPlan<i64> {
        let mut group = Group::new(timeout);
        group.push(HookWrapper::new("test.module", "hook1", hook));
        let mut plan = HookExecutionPlan::new();
        plan.push_group(group);
        plan
    }

    #[tokio::test]
    async fn simple_increment_hook() {
        let plan = single_group_plan(Arc::new(IncrementHook), Duration::from_secs(1));
        let executor = HookExecutor::new(Stage::EntrypointStage);
        let (payload, summary, err) = executor.execute(&plan, 41_i64).await;
        assert!(err.is_none());
        assert_eq!(payload, 42);
        assert_eq!(summary.successful_hooks, 1);
        assert_eq!(summary.mutations_applied, 1);
        assert!(!summary.rejected);
    }

    #[tokio::test]
    async fn hook_rejects_at_rejectable_stage() {
        let plan = single_group_plan(Arc::new(RejectingHook), Duration::from_secs(1));
        let executor = HookExecutor::new(Stage::EntrypointStage);
        let (_payload, summary, err) = executor.execute(&plan, 1_i64).await;
        assert!(summary.rejected);
        assert!(matches!(err, Some(HookError::Rejected(_))));
    }

    #[tokio::test]
    async fn hook_reject_ignored_at_non_rejectable_stage() {
        let plan = single_group_plan(Arc::new(RejectingHook), Duration::from_secs(1));
        let executor = HookExecutor::new(Stage::AuctionResponseStage);
        let (_payload, summary, err) = executor.execute(&plan, 1_i64).await;
        assert!(!summary.rejected);
        assert!(err.is_none());
    }

    #[tokio::test]
    async fn slow_hook_times_out() {
        let plan = single_group_plan(Arc::new(SlowHook), Duration::from_millis(20));
        let executor = HookExecutor::new(Stage::EntrypointStage);
        let (payload, summary, err) = executor.execute(&plan, 5_i64).await;
        // Timed out hook must not apply its mutation.
        assert_eq!(payload, 5);
        assert_eq!(summary.timed_out_hooks, 1);
        assert!(err.is_none());
    }

    #[tokio::test]
    async fn empty_changeset_produces_no_mutations() {
        let mut cs: ChangeSet<i64> = ChangeSet::new();
        // No mutations added.
        let (v, errs) = cs.apply_all(10);
        assert_eq!(v, 10);
        assert!(errs.is_empty());
        let _ = cs.add_mutation(|v| Ok(v), MutationType::Update, vec![]);
        assert_eq!(cs.len(), 1);
    }
}
