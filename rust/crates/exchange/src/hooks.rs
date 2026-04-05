use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Stage
// ---------------------------------------------------------------------------

/// Identifies a point in the auction pipeline where hooks may execute.
/// Mirrors the Go `hooks.Stage` constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    /// Before any request processing (HTTP-level).
    EntrypointRaw,
    /// After account lookup, before request parsing.
    RawAuctionRequest,
    /// After first-party-data enrichment, before bidder fan-out.
    ProcessedAuctionRequest,
    /// Per-bidder, before the request is sent.
    BidderRequest,
    /// Per-bidder, after the raw response is received.
    RawBidderResponse,
    /// After all bidder responses have been processed.
    AllProcessedBidResponses,
    /// Final response assembly.
    AuctionResponse,
}

impl Stage {
    /// Returns the canonical string name used in configuration and logging.
    pub fn as_str(&self) -> &'static str {
        match self {
            Stage::EntrypointRaw => "entrypoint",
            Stage::RawAuctionRequest => "raw_auction_request",
            Stage::ProcessedAuctionRequest => "processed_auction_request",
            Stage::BidderRequest => "bidder_request",
            Stage::RawBidderResponse => "raw_bidder_response",
            Stage::AllProcessedBidResponses => "all_processed_bid_responses",
            Stage::AuctionResponse => "auction_response",
        }
    }

    /// Whether rejection is permitted at this stage.
    /// `AllProcessedBidResponses` and `AuctionResponse` do not support rejection.
    pub fn is_rejectable(&self) -> bool {
        !matches!(
            self,
            Stage::AllProcessedBidResponses | Stage::AuctionResponse
        )
    }
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Mutation
// ---------------------------------------------------------------------------

/// The kind of change a mutation describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationAction {
    Add,
    Update,
    Delete,
}

impl fmt::Display for MutationAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MutationAction::Add => f.write_str("add"),
            MutationAction::Update => f.write_str("update"),
            MutationAction::Delete => f.write_str("delete"),
        }
    }
}

/// A type-erased mutation function.  Takes the current payload and returns the
/// mutated payload or an error.
pub type MutationFn = Box<dyn Fn(Value) -> Result<Value, String> + Send + Sync>;

/// A single mutation returned by a hook.
///
/// Carries the mutation type, a key path describing which part of the payload
/// is affected, and a closure that applies the change.
pub struct Mutation {
    pub action: MutationAction,
    /// Dot-separated key path (e.g. `["imp", "0", "ext"]`).
    pub key: Vec<String>,
    apply_fn: MutationFn,
}

impl Mutation {
    pub fn new(action: MutationAction, key: Vec<String>, apply_fn: MutationFn) -> Self {
        Self {
            action,
            key,
            apply_fn,
        }
    }

    /// Apply the mutation to `payload`, returning the modified value.
    pub fn apply(&self, payload: Value) -> Result<Value, String> {
        (self.apply_fn)(payload)
    }
}

impl fmt::Debug for Mutation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mutation")
            .field("action", &self.action)
            .field("key", &self.key)
            .finish()
    }
}

/// An ordered collection of mutations (mirrors Go `ChangeSet`).
#[derive(Default)]
pub struct ChangeSet {
    mutations: Vec<Mutation>,
}

impl ChangeSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_mutation(&mut self, mutation: Mutation) -> &mut Self {
        self.mutations.push(mutation);
        self
    }

    pub fn mutations(&self) -> &[Mutation] {
        &self.mutations
    }

    pub fn is_empty(&self) -> bool {
        self.mutations.is_empty()
    }
}

impl fmt::Debug for ChangeSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChangeSet")
            .field("len", &self.mutations.len())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// HookResult
// ---------------------------------------------------------------------------

/// The result returned by a hook after execution.
///
/// Mirrors Go `hookstage.HookResult[T]`.  Because Rust does not allow
/// carrying generic payloads through trait objects easily, the change-set
/// operates on `serde_json::Value`.
#[derive(Debug)]
pub struct HookResult {
    /// If `true`, the current stage is rejected.
    pub reject: bool,
    /// NBR (No-Bid Reason) code — required when `reject` is `true`.
    pub nbr_code: i32,
    /// Arbitrary message attached by the hook.
    pub message: String,
    /// Mutations the hook wants applied to the payload.
    pub change_set: ChangeSet,
    /// Error messages produced during hook execution.
    pub errors: Vec<String>,
    /// Warning messages produced during hook execution.
    pub warnings: Vec<String>,
    /// Debug messages produced during hook execution.
    pub debug_messages: Vec<String>,
    /// Data the module wants to pass to itself at later stages.
    pub module_context: ModuleContext,
}

impl Default for HookResult {
    fn default() -> Self {
        Self {
            reject: false,
            nbr_code: 0,
            message: String::new(),
            change_set: ChangeSet::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
            debug_messages: Vec::new(),
            module_context: ModuleContext::new(),
        }
    }
}

impl HookResult {
    /// Convenience: a result that does nothing.
    pub fn noop() -> Self {
        Self::default()
    }

    /// Convenience: a result that rejects the current stage.
    pub fn rejection(nbr_code: i32, message: impl Into<String>) -> Self {
        Self {
            reject: true,
            nbr_code,
            message: message.into(),
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// ModuleContext / InvocationContext
// ---------------------------------------------------------------------------

/// Arbitrary data a module passes to itself across stages.
pub type ModuleContext = HashMap<String, Value>;

/// Data provided to a hook at invocation time.
///
/// Mirrors Go `hookstage.ModuleInvocationContext`.
#[derive(Debug, Clone, Default)]
pub struct InvocationContext {
    /// The account ID of the current request.
    pub account_id: String,
    /// Account-level module configuration (raw JSON).
    pub account_config: Option<Value>,
    /// The endpoint path (e.g. `/openrtb2/auction`).
    pub endpoint: String,
    /// Data the module stored at a previous stage.
    pub module_context: ModuleContext,
    /// The hook_impl_code for this particular hook registration.
    pub hook_impl_code: String,
}

// ---------------------------------------------------------------------------
// Hook trait
// ---------------------------------------------------------------------------

/// The core trait that modules implement.
///
/// Each module provides one or more `Hook` implementations.  The executor
/// calls `handle` with the current stage payload serialised as
/// `serde_json::Value` and the invocation context.
///
/// Hooks are async so they can perform I/O (HTTP calls, DB lookups, etc.)
/// without blocking the executor.
#[async_trait]
pub trait Hook: Send + Sync {
    /// Unique identifier for this hook (usually `"vendor.module_name"`).
    fn id(&self) -> &str;

    /// The stages this hook should run at.
    fn stages(&self) -> &[Stage];

    /// Execute the hook logic.
    async fn handle(
        &self,
        ctx: &InvocationContext,
        payload: Value,
    ) -> Result<HookResult, HookError>;
}

// ---------------------------------------------------------------------------
// Hook errors
// ---------------------------------------------------------------------------

/// Errors that can occur during hook execution.
#[derive(Debug, thiserror::Error)]
pub enum HookError {
    /// The hook exceeded its allotted time.
    #[error("hook execution timeout")]
    Timeout,
    /// An expected, module-side failure.
    #[error("hook execution failed: {0}")]
    Failure(String),
    /// An unexpected internal error.
    #[error("hook execution error: {0}")]
    Internal(String),
}

/// Error returned when a hook rejects the current stage.
#[derive(Debug, thiserror::Error)]
#[error("module {module} (hook: {hook_impl_code}) rejected request with code {nbr} at {stage} stage")]
pub struct RejectError {
    pub nbr: i32,
    pub module: String,
    pub hook_impl_code: String,
    pub stage: String,
}

// ---------------------------------------------------------------------------
// Execution-outcome types
// ---------------------------------------------------------------------------

/// Status of an individual hook invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvocationStatus {
    Success,
    Timeout,
    Failure,
    ExecutionFailure,
}

impl fmt::Display for InvocationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvocationStatus::Success => f.write_str("success"),
            InvocationStatus::Timeout => f.write_str("timeout"),
            InvocationStatus::Failure => f.write_str("failure"),
            InvocationStatus::ExecutionFailure => f.write_str("execution_failure"),
        }
    }
}

/// What action the hook took.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvocationAction {
    /// Mutations were applied.
    Update,
    /// The stage was rejected.
    Reject,
    /// No action taken (noop).
    None,
}

/// Identity of a hook within an execution plan.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HookId {
    pub module_code: String,
    pub hook_impl_code: String,
}

/// Outcome of a single hook invocation.
#[derive(Debug, Clone)]
pub struct HookOutcome {
    pub hook_id: HookId,
    pub status: InvocationStatus,
    pub action: InvocationAction,
    pub message: String,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub debug_messages: Vec<String>,
    pub execution_time: Duration,
}

/// Outcome of a group of hooks that ran together.
#[derive(Debug, Default, Clone)]
pub struct GroupOutcome {
    pub invocation_results: Vec<HookOutcome>,
    pub execution_time: Duration,
}

/// Outcome of an entire stage.
#[derive(Debug, Default, Clone)]
pub struct StageOutcome {
    pub stage: String,
    pub entity: String,
    pub groups: Vec<GroupOutcome>,
    pub execution_time: Duration,
}

// ---------------------------------------------------------------------------
// HookRepository
// ---------------------------------------------------------------------------

/// Stores hook registrations, keyed by (stage, module id).
///
/// Mirrors Go `hooks.HookRepository`.  We store trait objects so that any
/// module can register hooks without the repository knowing concrete types.
pub struct HookRepository {
    /// stage -> ordered list of (module_code, hook)
    hooks: HashMap<Stage, Vec<(String, Arc<dyn Hook>)>>,
}

impl HookRepository {
    pub fn new() -> Self {
        Self {
            hooks: HashMap::new(),
        }
    }

    /// Register a hook for one or more stages.  The hook's `stages()` method
    /// determines which stages it is added to.
    pub fn register(&mut self, hook: Arc<dyn Hook>) -> Result<(), String> {
        let stages = hook.stages().to_vec();
        if stages.is_empty() {
            return Err(format!(
                "hook \"{}\" does not declare any stages",
                hook.id()
            ));
        }
        let id = hook.id().to_owned();
        for stage in stages {
            let entry = self.hooks.entry(stage).or_default();
            if entry.iter().any(|(existing_id, _)| existing_id == &id) {
                return Err(format!(
                    "hook \"{}\" already registered for stage {}",
                    id, stage
                ));
            }
            entry.push((id.clone(), Arc::clone(&hook)));
        }
        Ok(())
    }

    /// Retrieve all hooks registered for the given stage, in registration order.
    pub fn get_hooks(&self, stage: Stage) -> &[(String, Arc<dyn Hook>)] {
        self.hooks
            .get(&stage)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

impl Default for HookRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for HookRepository {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let summary: HashMap<Stage, Vec<&str>> = self
            .hooks
            .iter()
            .map(|(stage, hooks)| (*stage, hooks.iter().map(|(id, _)| id.as_str()).collect()))
            .collect();
        f.debug_struct("HookRepository")
            .field("hooks", &summary)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Execution plan types
// ---------------------------------------------------------------------------

/// A single hook within an execution group, carrying metadata.
pub struct HookWrapper {
    /// Module name, e.g. `"vendor.module_name"`.
    pub module: String,
    /// Arbitrary code assigned via the execution plan configuration.
    pub code: String,
    /// The hook implementation.
    pub hook: Arc<dyn Hook>,
}

/// A group of hooks that may execute concurrently, with a shared timeout.
pub struct HookGroup {
    /// Maximum time this group is allowed to run.
    pub timeout: Duration,
    /// The hooks in this group.
    pub hooks: Vec<HookWrapper>,
}

/// An ordered list of groups forming the execution plan for one stage.
pub type ExecutionPlan = Vec<HookGroup>;

// ---------------------------------------------------------------------------
// PlanBuilder
// ---------------------------------------------------------------------------

/// Builds execution plans for each stage from configuration and the hook
/// repository.
///
/// Mirrors Go `hooks.PlanBuilder` / `ExecutionPlanBuilder`.
pub struct PlanBuilder {
    repo: Arc<HookRepository>,
    /// Whether hooks are enabled at all.
    enabled: bool,
    /// Default timeout applied per group when not configured explicitly.
    default_group_timeout: Duration,
}

impl PlanBuilder {
    pub fn new(repo: Arc<HookRepository>, enabled: bool) -> Self {
        Self {
            repo,
            enabled,
            default_group_timeout: Duration::from_millis(5_000),
        }
    }

    /// Override the default per-group timeout.
    pub fn with_default_group_timeout(mut self, timeout: Duration) -> Self {
        self.default_group_timeout = timeout;
        self
    }

    /// Build the plan for the given `stage` and `endpoint`.
    ///
    /// When hooks are disabled, an empty plan is returned.
    pub fn plan_for_stage(&self, stage: Stage, _endpoint: &str) -> ExecutionPlan {
        if !self.enabled {
            return Vec::new();
        }

        let hooks = self.repo.get_hooks(stage);
        if hooks.is_empty() {
            return Vec::new();
        }

        // Default strategy: all hooks in a single group with the default
        // timeout.  A richer implementation would read group configuration
        // from the host / account execution plan, exactly like the Go side.
        let wrappers: Vec<HookWrapper> = hooks
            .iter()
            .map(|(id, hook)| HookWrapper {
                module: id.clone(),
                code: String::new(),
                hook: Arc::clone(hook),
            })
            .collect();

        vec![HookGroup {
            timeout: self.default_group_timeout,
            hooks: wrappers,
        }]
    }

    /// Return an empty plan (convenience for disabled / no-op paths).
    pub fn empty_plan() -> ExecutionPlan {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// ModuleContextStore  (thread-safe cross-stage context)
// ---------------------------------------------------------------------------

/// Thread-safe store for module contexts that persist across stages.
///
/// Mirrors Go `moduleContexts` (with `sync.RWMutex`).
#[derive(Debug, Default, Clone)]
pub struct ModuleContextStore {
    inner: Arc<RwLock<HashMap<String, ModuleContext>>>,
}

impl ModuleContextStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get(&self, module: &str) -> Option<ModuleContext> {
        self.inner.read().await.get(module).cloned()
    }

    pub async fn put(&self, module: &str, ctx: ModuleContext) {
        let mut store = self.inner.write().await;
        let entry = store.entry(module.to_owned()).or_default();
        for (k, v) in ctx {
            entry.insert(k, v);
        }
    }
}

// ---------------------------------------------------------------------------
// HookExecutor
// ---------------------------------------------------------------------------

/// Runs registered hooks for each stage.
///
/// Mirrors Go `hookexecution.hookExecutor`.
pub struct HookExecutor {
    plan_builder: Arc<PlanBuilder>,
    endpoint: String,
    account_id: String,
    module_contexts: ModuleContextStore,
    stage_outcomes: Arc<tokio::sync::Mutex<Vec<StageOutcome>>>,
}

impl HookExecutor {
    pub fn new(plan_builder: Arc<PlanBuilder>, endpoint: impl Into<String>) -> Self {
        Self {
            plan_builder,
            endpoint: endpoint.into(),
            account_id: String::new(),
            module_contexts: ModuleContextStore::new(),
            stage_outcomes: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }

    pub fn set_account_id(&mut self, id: impl Into<String>) {
        self.account_id = id.into();
    }

    /// Retrieve all stage outcomes collected so far.
    pub async fn outcomes(&self) -> Vec<StageOutcome> {
        self.stage_outcomes.lock().await.clone()
    }

    /// Execute all hooks for the given `stage` against `payload`.
    ///
    /// Returns the (possibly mutated) payload, or a `RejectError` if a hook
    /// rejected the stage.
    pub async fn execute_stage(
        &self,
        stage: Stage,
        mut payload: Value,
    ) -> Result<Value, RejectError> {
        let plan = self.plan_builder.plan_for_stage(stage, &self.endpoint);
        if plan.is_empty() {
            return Ok(payload);
        }

        let mut stage_outcome = StageOutcome {
            stage: stage.to_string(),
            ..Default::default()
        };

        for group in &plan {
            let (group_outcome, new_payload, reject) =
                self.execute_group(stage, group, payload).await;
            stage_outcome.execution_time += group_outcome.execution_time;
            stage_outcome.groups.push(group_outcome);

            if let Some(reject_err) = reject {
                self.push_outcome(stage_outcome).await;
                return Err(reject_err);
            }
            payload = new_payload;
        }

        self.push_outcome(stage_outcome).await;
        Ok(payload)
    }

    /// Execute a single group of hooks.
    ///
    /// Hooks within a group run concurrently (matching Go behaviour).
    /// The group timeout is enforced per hook.
    async fn execute_group(
        &self,
        stage: Stage,
        group: &HookGroup,
        payload: Value,
    ) -> (GroupOutcome, Value, Option<RejectError>) {
        let mut group_outcome = GroupOutcome::default();
        let mut current_payload = payload.clone();

        // Spawn all hooks concurrently.
        let mut handles = Vec::with_capacity(group.hooks.len());
        for hw in &group.hooks {
            let hook = Arc::clone(&hw.hook);
            let module = hw.module.clone();
            let code = hw.code.clone();
            let payload_snap = payload.clone();
            let timeout = group.timeout;
            let module_ctx = self.module_contexts.get(&module).await.unwrap_or_default();

            let inv_ctx = InvocationContext {
                account_id: self.account_id.clone(),
                account_config: None,
                endpoint: self.endpoint.clone(),
                module_context: module_ctx,
                hook_impl_code: code.clone(),
            };

            let handle = tokio::spawn(async move {
                let start = std::time::Instant::now();
                let result = tokio::time::timeout(
                    timeout,
                    hook.handle(&inv_ctx, payload_snap),
                )
                .await;
                let elapsed = start.elapsed();

                let hook_id = HookId {
                    module_code: module,
                    hook_impl_code: code,
                };

                match result {
                    Ok(Ok(hr)) => (hook_id, Ok(hr), elapsed),
                    Ok(Err(e)) => (hook_id, Err(e), elapsed),
                    Err(_) => (hook_id, Err(HookError::Timeout), elapsed),
                }
            });
            handles.push(handle);
        }

        // Collect results.
        let mut reject: Option<RejectError> = None;
        for handle in handles {
            let (hook_id, result, elapsed) = match handle.await {
                Ok(v) => v,
                Err(e) => {
                    // JoinError — should not happen in normal flow.
                    tracing::error!("hook task panicked: {e}");
                    continue;
                }
            };

            if elapsed > group_outcome.execution_time {
                group_outcome.execution_time = elapsed;
            }

            let hook_outcome = match result {
                Ok(hr) => {
                    // Save module context for later stages.
                    if !hr.module_context.is_empty() {
                        self.module_contexts
                            .put(&hook_id.module_code, hr.module_context.clone())
                            .await;
                    }

                    if hr.reject {
                        if stage.is_rejectable() {
                            reject = Some(RejectError {
                                nbr: hr.nbr_code,
                                module: hook_id.module_code.clone(),
                                hook_impl_code: hook_id.hook_impl_code.clone(),
                                stage: stage.to_string(),
                            });
                            HookOutcome {
                                hook_id,
                                status: InvocationStatus::Success,
                                action: InvocationAction::Reject,
                                message: hr.message,
                                errors: hr.errors,
                                warnings: hr.warnings,
                                debug_messages: hr.debug_messages,
                                execution_time: elapsed,
                            }
                        } else {
                            // Rejection not allowed at this stage.
                            let mut errors = hr.errors;
                            errors.push(format!(
                                "module {} tried to reject at non-rejectable stage {}",
                                hook_id.module_code, stage
                            ));
                            HookOutcome {
                                hook_id,
                                status: InvocationStatus::ExecutionFailure,
                                action: InvocationAction::None,
                                message: hr.message,
                                errors,
                                warnings: hr.warnings,
                                debug_messages: hr.debug_messages,
                                execution_time: elapsed,
                            }
                        }
                    } else if !hr.change_set.is_empty() {
                        // Apply mutations.
                        let mut successful = 0usize;
                        let mut warnings = hr.warnings.clone();
                        let mut debug = hr.debug_messages.clone();
                        for mutation in hr.change_set.mutations() {
                            match mutation.apply(current_payload.clone()) {
                                Ok(p) => {
                                    current_payload = p;
                                    successful += 1;
                                    debug.push(format!(
                                        "mutation applied: key={}, type={}",
                                        mutation.key.join("."),
                                        mutation.action,
                                    ));
                                }
                                Err(e) => {
                                    warnings.push(format!(
                                        "failed to apply hook mutation: {}",
                                        e
                                    ));
                                }
                            }
                        }
                        let status = if successful > 0 {
                            InvocationStatus::Success
                        } else {
                            InvocationStatus::ExecutionFailure
                        };
                        HookOutcome {
                            hook_id,
                            status,
                            action: InvocationAction::Update,
                            message: hr.message,
                            errors: hr.errors,
                            warnings,
                            debug_messages: debug,
                            execution_time: elapsed,
                        }
                    } else {
                        // Noop.
                        HookOutcome {
                            hook_id,
                            status: InvocationStatus::Success,
                            action: InvocationAction::None,
                            message: hr.message,
                            errors: hr.errors,
                            warnings: hr.warnings,
                            debug_messages: hr.debug_messages,
                            execution_time: elapsed,
                        }
                    }
                }
                Err(hook_err) => {
                    let (status, err_msg) = match &hook_err {
                        HookError::Timeout => (InvocationStatus::Timeout, hook_err.to_string()),
                        HookError::Failure(_) => {
                            (InvocationStatus::Failure, hook_err.to_string())
                        }
                        HookError::Internal(_) => {
                            (InvocationStatus::ExecutionFailure, hook_err.to_string())
                        }
                    };
                    HookOutcome {
                        hook_id,
                        status,
                        action: InvocationAction::None,
                        message: String::new(),
                        errors: vec![err_msg],
                        warnings: Vec::new(),
                        debug_messages: Vec::new(),
                        execution_time: elapsed,
                    }
                }
            };

            group_outcome.invocation_results.push(hook_outcome);

            if reject.is_some() {
                break;
            }
        }

        (group_outcome, current_payload, reject)
    }

    async fn push_outcome(&self, outcome: StageOutcome) {
        self.stage_outcomes.lock().await.push(outcome);
    }
}

// ---------------------------------------------------------------------------
// EmptyHookExecutor
// ---------------------------------------------------------------------------

/// A no-op executor that never runs any hooks.
///
/// Useful as a default / placeholder (mirrors Go `EmptyHookExecutor`).
pub struct EmptyHookExecutor;

impl EmptyHookExecutor {
    /// Always returns the payload unchanged.
    pub async fn execute_stage(&self, _stage: Stage, payload: Value) -> Result<Value, RejectError> {
        Ok(payload)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A trivial hook that adds a field to the payload.
    struct TestHook {
        id: String,
        stages: Vec<Stage>,
    }

    #[async_trait]
    impl Hook for TestHook {
        fn id(&self) -> &str {
            &self.id
        }
        fn stages(&self) -> &[Stage] {
            &self.stages
        }
        async fn handle(
            &self,
            _ctx: &InvocationContext,
            _payload: Value,
        ) -> Result<HookResult, HookError> {
            let mut result = HookResult::noop();
            result.change_set.add_mutation(Mutation::new(
                MutationAction::Update,
                vec!["test".to_string()],
                Box::new(|mut v| {
                    v["test"] = json!("modified");
                    Ok(v)
                }),
            ));
            Ok(result)
        }
    }

    /// A hook that rejects the stage.
    struct RejectHook;

    #[async_trait]
    impl Hook for RejectHook {
        fn id(&self) -> &str {
            "vendor.reject_hook"
        }
        fn stages(&self) -> &[Stage] {
            &[Stage::RawAuctionRequest]
        }
        async fn handle(
            &self,
            _ctx: &InvocationContext,
            _payload: Value,
        ) -> Result<HookResult, HookError> {
            Ok(HookResult::rejection(100, "blocked"))
        }
    }

    #[test]
    fn stage_display_and_rejectable() {
        assert_eq!(Stage::EntrypointRaw.as_str(), "entrypoint");
        assert!(Stage::BidderRequest.is_rejectable());
        assert!(!Stage::AllProcessedBidResponses.is_rejectable());
        assert!(!Stage::AuctionResponse.is_rejectable());
    }

    #[test]
    fn mutation_action_display() {
        assert_eq!(MutationAction::Add.to_string(), "add");
        assert_eq!(MutationAction::Update.to_string(), "update");
        assert_eq!(MutationAction::Delete.to_string(), "delete");
    }

    #[test]
    fn change_set_add_and_apply() {
        let mut cs = ChangeSet::new();
        assert!(cs.is_empty());
        cs.add_mutation(Mutation::new(
            MutationAction::Update,
            vec!["key".to_string()],
            Box::new(|mut v| {
                v["key"] = json!(42);
                Ok(v)
            }),
        ));
        assert!(!cs.is_empty());
        let result = cs.mutations()[0].apply(json!({})).unwrap();
        assert_eq!(result["key"], 42);
    }

    #[test]
    fn hook_result_defaults() {
        let hr = HookResult::noop();
        assert!(!hr.reject);
        assert!(hr.change_set.is_empty());

        let hr = HookResult::rejection(5, "no");
        assert!(hr.reject);
        assert_eq!(hr.nbr_code, 5);
    }

    #[test]
    fn hook_repository_register_and_get() {
        let mut repo = HookRepository::new();
        let hook: Arc<dyn Hook> = Arc::new(TestHook {
            id: "vendor.test".to_string(),
            stages: vec![Stage::EntrypointRaw, Stage::RawAuctionRequest],
        });
        repo.register(hook).unwrap();

        assert_eq!(repo.get_hooks(Stage::EntrypointRaw).len(), 1);
        assert_eq!(repo.get_hooks(Stage::RawAuctionRequest).len(), 1);
        assert_eq!(repo.get_hooks(Stage::BidderRequest).len(), 0);
    }

    #[test]
    fn hook_repository_duplicate_rejected() {
        let mut repo = HookRepository::new();
        let hook: Arc<dyn Hook> = Arc::new(TestHook {
            id: "vendor.test".to_string(),
            stages: vec![Stage::EntrypointRaw],
        });
        repo.register(Arc::clone(&hook)).unwrap();
        assert!(repo.register(hook).is_err());
    }

    #[tokio::test]
    async fn executor_runs_hooks_and_applies_mutations() {
        let mut repo = HookRepository::new();
        repo.register(Arc::new(TestHook {
            id: "vendor.test".to_string(),
            stages: vec![Stage::RawAuctionRequest],
        }))
        .unwrap();

        let plan_builder = Arc::new(PlanBuilder::new(Arc::new(repo), true));
        let executor = HookExecutor::new(plan_builder, "/openrtb2/auction");

        let payload = json!({"foo": "bar"});
        let result = executor
            .execute_stage(Stage::RawAuctionRequest, payload)
            .await
            .unwrap();
        assert_eq!(result["test"], "modified");
        assert_eq!(result["foo"], "bar");

        let outcomes = executor.outcomes().await;
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].stage, "raw_auction_request");
    }

    #[tokio::test]
    async fn executor_reject_stops_execution() {
        let mut repo = HookRepository::new();
        repo.register(Arc::new(RejectHook)).unwrap();

        let plan_builder = Arc::new(PlanBuilder::new(Arc::new(repo), true));
        let executor = HookExecutor::new(plan_builder, "/openrtb2/auction");

        let payload = json!({});
        let err = executor
            .execute_stage(Stage::RawAuctionRequest, payload)
            .await
            .unwrap_err();
        assert_eq!(err.nbr, 100);
        assert_eq!(err.stage, "raw_auction_request");
    }

    #[tokio::test]
    async fn executor_disabled_returns_payload_unchanged() {
        let repo = HookRepository::new();
        let plan_builder = Arc::new(PlanBuilder::new(Arc::new(repo), false));
        let executor = HookExecutor::new(plan_builder, "/openrtb2/auction");

        let payload = json!({"a": 1});
        let result = executor
            .execute_stage(Stage::RawAuctionRequest, payload.clone())
            .await
            .unwrap();
        assert_eq!(result, payload);
    }

    #[tokio::test]
    async fn empty_executor_passthrough() {
        let executor = EmptyHookExecutor;
        let payload = json!({"x": true});
        let result = executor
            .execute_stage(Stage::EntrypointRaw, payload.clone())
            .await
            .unwrap();
        assert_eq!(result, payload);
    }

    #[tokio::test]
    async fn module_context_store_put_and_get() {
        let store = ModuleContextStore::new();
        assert!(store.get("mod1").await.is_none());

        let mut ctx = ModuleContext::new();
        ctx.insert("key1".to_string(), json!("val1"));
        store.put("mod1", ctx).await;

        let retrieved = store.get("mod1").await.unwrap();
        assert_eq!(retrieved["key1"], "val1");

        // Merge behaviour: new keys are added, existing keys updated.
        let mut ctx2 = ModuleContext::new();
        ctx2.insert("key2".to_string(), json!("val2"));
        store.put("mod1", ctx2).await;

        let retrieved = store.get("mod1").await.unwrap();
        assert_eq!(retrieved["key1"], "val1");
        assert_eq!(retrieved["key2"], "val2");
    }

    #[tokio::test]
    async fn reject_at_non_rejectable_stage_becomes_failure() {
        // A hook that tries to reject at AuctionResponse (non-rejectable).
        struct BadRejectHook;

        #[async_trait]
        impl Hook for BadRejectHook {
            fn id(&self) -> &str {
                "vendor.bad_reject"
            }
            fn stages(&self) -> &[Stage] {
                &[Stage::AuctionResponse]
            }
            async fn handle(
                &self,
                _ctx: &InvocationContext,
                _payload: Value,
            ) -> Result<HookResult, HookError> {
                Ok(HookResult::rejection(99, "should not work"))
            }
        }

        let mut repo = HookRepository::new();
        repo.register(Arc::new(BadRejectHook)).unwrap();

        let plan_builder = Arc::new(PlanBuilder::new(Arc::new(repo), true));
        let executor = HookExecutor::new(plan_builder, "/openrtb2/auction");

        // Should succeed (rejection is ignored at non-rejectable stage).
        let result = executor
            .execute_stage(Stage::AuctionResponse, json!({}))
            .await;
        assert!(result.is_ok());

        let outcomes = executor.outcomes().await;
        let hook_outcome = &outcomes[0].groups[0].invocation_results[0];
        assert_eq!(hook_outcome.status, InvocationStatus::ExecutionFailure);
    }
}
