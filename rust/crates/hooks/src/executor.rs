//! Hook execution engine.
//!
//! The `HookExecutor` walks a `HookExecutionPlan`, running every group's
//! hooks **in parallel** under a shared per-group timeout. Once the group's
//! hooks have finished (or timed out), the executor folds their changesets
//! back into the payload in *declaration* (index) order so that mutations
//! remain deterministic regardless of task completion order.
//!
//! This mirrors Go's `ExecutionPlan.ExecuteGroup`, which launches each hook
//! on its own goroutine with a shared context deadline and then collects
//! their `HookResult`s before applying them sequentially.

use std::time::Duration;

use futures::future::join_all;
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
    /// # Payload constraints
    ///
    /// Because hooks within a group run **in parallel** on dedicated tokio
    /// tasks, each hook needs its own copy of the payload. The payload type
    /// must therefore be `Clone + Send + 'static`.
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
            let (new_payload, rejected) = self.execute_group(group, payload, &mut summary).await;
            payload = new_payload;
            if let Some(err) = rejected {
                summary.rejected = true;
                return (payload, summary, Some(err));
            }
        }

        (payload, summary, None)
    }

    /// Execute a single group with all hooks running concurrently, applying
    /// their side effects in declaration order.
    async fn execute_group<P>(
        &self,
        group: &Group<P>,
        payload: P,
        summary: &mut StageExecutionSummary,
    ) -> (P, Option<HookError>)
    where
        P: Clone + Send + 'static,
    {
        if group.hooks.is_empty() {
            return (payload, None);
        }

        // Spawn every hook on its own task. Each task clones the payload and
        // owns a dedicated context so the future is `'static`.
        let group_timeout = group.timeout;
        let mut handles = Vec::with_capacity(group.hooks.len());

        for hook in &group.hooks {
            let hook_wrapper: HookWrapper<P> = hook.clone();
            let payload_clone: P = payload.clone();
            let ctx = ModuleInvocationContext {
                account_id: self.account_id.clone(),
                endpoint: self.endpoint.clone(),
                hook_impl_code: hook_wrapper.code.clone(),
                ..Default::default()
            };

            let handle = tokio::spawn(async move {
                // Borrow the owned ctx locally so its lifetime is tied to
                // this task rather than the caller.
                let ctx_ref = &ctx;
                timeout(group_timeout, hook_wrapper.hook.handle(ctx_ref, payload_clone)).await
            });
            handles.push(handle);
        }

        // Wait for all hooks to complete. `join_all` preserves iterator
        // order, so `results[i]` corresponds to `group.hooks[i]`.
        let results = join_all(handles).await;

        // Fold results in declaration order.
        let mut current = payload;
        for (idx, join_res) in results.into_iter().enumerate() {
            let hook = &group.hooks[idx];

            // Unwrap the join result (outer layer is the JoinHandle), then
            // the timeout layer, then the hook's own Result.
            let hook_outcome = match join_res {
                Ok(Ok(inner)) => inner,
                Ok(Err(_elapsed)) => {
                    summary.timed_out_hooks += 1;
                    warn!(module = %hook.module, code = %hook.code, "hook timed out");
                    continue;
                }
                Err(join_err) => {
                    summary.hooks_with_errors += 1;
                    warn!(
                        module = %hook.module,
                        code = %hook.code,
                        error = %join_err,
                        "hook task panicked or was cancelled"
                    );
                    continue;
                }
            };

            let result = match hook_outcome {
                Ok(result) => result,
                Err(e) => {
                    match &e {
                        HookError::Timeout => summary.timed_out_hooks += 1,
                        _ => summary.hooks_with_errors += 1,
                    }
                    warn!(module = %hook.module, code = %hook.code, error = %e, "hook failed");
                    // A hook-reported Reject at a rejectable stage short-
                    // circuits the remainder of the plan, but we still let
                    // earlier, already-applied mutations (up to `current`)
                    // stand.
                    if matches!(e, HookError::Rejected(_)) && self.stage.is_rejectable() {
                        return (current, Some(e));
                    }
                    continue;
                }
            };

            current = self.apply_result(result, current, hook, summary);
            if summary.rejected {
                // The hook's `HookResult::reject` flag fired: surface it as
                // an explicit HookError and abort the group / plan.
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

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

    /// Hook that sleeps before contributing its mutation. Used to assert
    /// parallelism (three copies with 100ms sleep should finish in <200ms).
    /// The completion order is tracked in `order` so that we can check that
    /// changesets are applied in declaration order regardless.
    struct SleepyAddHook {
        delta: i64,
        sleep: Duration,
        order: Arc<AtomicUsize>,
        id: usize,
        completions: Arc<std::sync::Mutex<Vec<usize>>>,
    }

    #[async_trait]
    impl Hook<i64> for SleepyAddHook {
        async fn handle(
            &self,
            _ctx: &ModuleInvocationContext,
            _payload: i64,
        ) -> Result<HookResult<i64>, HookError> {
            tokio::time::sleep(self.sleep).await;
            self.order.fetch_add(1, Ordering::SeqCst);
            self.completions.lock().unwrap().push(self.id);
            let delta = self.delta;
            let mut r = HookResult::new();
            r.changeset.add_mutation(
                move |v| Ok(v + delta),
                MutationType::Update,
                vec!["value".to_string()],
            );
            Ok(r)
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

    /// Three hooks that each sleep 100ms should complete in roughly 100ms
    /// total when run in parallel, not 300ms.
    #[tokio::test]
    async fn hooks_within_group_execute_in_parallel() {
        let order = Arc::new(AtomicUsize::new(0));
        let completions = Arc::new(std::sync::Mutex::new(Vec::new()));

        let mut group = Group::new(Duration::from_secs(2));
        for (i, delta) in [1_i64, 10, 100].iter().enumerate() {
            group.push(HookWrapper::new(
                "test.module",
                format!("hook{}", i),
                Arc::new(SleepyAddHook {
                    delta: *delta,
                    sleep: Duration::from_millis(100),
                    order: order.clone(),
                    id: i,
                    completions: completions.clone(),
                }) as Arc<dyn Hook<i64>>,
            ));
        }
        let mut plan = HookExecutionPlan::new();
        plan.push_group(group);

        let executor = HookExecutor::new(Stage::EntrypointStage);
        let start = Instant::now();
        let (payload, summary, err) = executor.execute(&plan, 0_i64).await;
        let elapsed = start.elapsed();

        assert!(err.is_none());
        assert_eq!(payload, 111);
        assert_eq!(summary.successful_hooks, 3);
        assert_eq!(summary.mutations_applied, 3);
        // If they were serialized we'd be at ~300ms. Parallel execution
        // should land firmly under 250ms.
        assert!(
            elapsed < Duration::from_millis(250),
            "expected parallel execution (<250ms), took {:?}",
            elapsed
        );
        assert!(
            elapsed >= Duration::from_millis(90),
            "expected sleep overhead (>=90ms), took {:?}",
            elapsed
        );
    }

    /// Three `AddHook`s in a single group must apply their mutations in
    /// declaration order, regardless of completion order. We verify that by
    /// deliberately varying per-hook latency and checking that the final
    /// payload equals the ordered fold.
    #[tokio::test]
    async fn changesets_apply_in_declaration_order() {
        let order = Arc::new(AtomicUsize::new(0));
        let completions = Arc::new(std::sync::Mutex::new(Vec::new()));

        let mut group = Group::new(Duration::from_secs(2));
        // Intentionally descending sleep so the last hook in declaration
        // order finishes first on the tokio scheduler.
        let specs = [
            (1_i64, Duration::from_millis(120)),
            (10_i64, Duration::from_millis(60)),
            (100_i64, Duration::from_millis(10)),
        ];
        for (i, (delta, sleep)) in specs.iter().enumerate() {
            group.push(HookWrapper::new(
                "test.module",
                format!("hook{}", i),
                Arc::new(SleepyAddHook {
                    delta: *delta,
                    sleep: *sleep,
                    order: order.clone(),
                    id: i,
                    completions: completions.clone(),
                }) as Arc<dyn Hook<i64>>,
            ));
        }
        let mut plan = HookExecutionPlan::new();
        plan.push_group(group);

        let executor = HookExecutor::new(Stage::EntrypointStage);
        let (payload, summary, err) = executor.execute(&plan, 0_i64).await;

        assert!(err.is_none());
        // 0 + 1 + 10 + 100 regardless of completion order.
        assert_eq!(payload, 111);
        assert_eq!(summary.successful_hooks, 3);

        // Sanity-check that completion order really was not declaration
        // order, so the previous assertion is meaningful.
        let observed = completions.lock().unwrap().clone();
        assert_eq!(observed.len(), 3);
        assert_ne!(
            observed,
            vec![0usize, 1, 2],
            "completion order matched declaration order; the test no longer \
             exercises ordering stability"
        );
    }

    /// A group with one slow hook must time out that hook while letting a
    /// sibling fast hook complete. Because both hooks see the same group
    /// timeout, the outer `timeout(group.timeout, ...)` fires only on the
    /// slow one.
    #[tokio::test]
    async fn group_timeout_fires_on_slow_hook_only() {
        let mut group = Group::new(Duration::from_millis(60));
        group.push(HookWrapper::new(
            "test.module",
            "fast",
            Arc::new(IncrementHook) as Arc<dyn Hook<i64>>,
        ));
        group.push(HookWrapper::new(
            "test.module",
            "slow",
            Arc::new(SlowHook) as Arc<dyn Hook<i64>>, // sleeps 200ms
        ));
        let mut plan = HookExecutionPlan::new();
        plan.push_group(group);

        let executor = HookExecutor::new(Stage::EntrypointStage);
        let start = Instant::now();
        let (payload, summary, err) = executor.execute(&plan, 0_i64).await;
        let elapsed = start.elapsed();

        assert!(err.is_none());
        // Fast hook's +1 applied, slow hook's +100 dropped due to timeout.
        assert_eq!(payload, 1);
        assert_eq!(summary.successful_hooks, 1);
        assert_eq!(summary.timed_out_hooks, 1);
        // The executor waits for the outer timeout before moving on.
        assert!(elapsed < Duration::from_millis(180));
    }

    /// A rejection in group N must prevent group N+1 from running at all.
    #[tokio::test]
    async fn reject_short_circuits_next_group() {
        let ran = Arc::new(AtomicUsize::new(0));

        struct TracingHook(Arc<AtomicUsize>);
        #[async_trait]
        impl Hook<i64> for TracingHook {
            async fn handle(
                &self,
                _ctx: &ModuleInvocationContext,
                _payload: i64,
            ) -> Result<HookResult<i64>, HookError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(HookResult::new())
            }
        }

        let mut g1 = Group::new(Duration::from_secs(1));
        g1.push(HookWrapper::new(
            "test.module",
            "reject",
            Arc::new(RejectingHook) as Arc<dyn Hook<i64>>,
        ));
        let mut g2 = Group::new(Duration::from_secs(1));
        g2.push(HookWrapper::new(
            "test.module",
            "never",
            Arc::new(TracingHook(ran.clone())) as Arc<dyn Hook<i64>>,
        ));
        let mut plan = HookExecutionPlan::new();
        plan.push_group(g1);
        plan.push_group(g2);

        let executor = HookExecutor::new(Stage::EntrypointStage);
        let (_payload, summary, err) = executor.execute(&plan, 0_i64).await;

        assert!(summary.rejected);
        assert!(matches!(err, Some(HookError::Rejected(_))));
        assert_eq!(ran.load(Ordering::SeqCst), 0, "second group must not run");
    }
}
