//! Hook execution engine with stage-specific methods and metrics integration.
//!
//! This module builds on top of [`super::hooks`] to provide convenience
//! stage-execution methods that mirror the Go
//! `hooks/hookexecution/executor.go` surface.  Each stage method knows
//! which entity type is processed and records module-level metrics through
//! a [`MetricsEngine`] reference.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::Mutex;

use super::hooks::{
    ChangeSet, ExecutionPlan, GroupOutcome, Hook, HookError, HookGroup, HookId, HookOutcome,
    HookRepository, HookResult, HookWrapper, InvocationAction, InvocationContext,
    InvocationStatus, ModuleContext, ModuleContextStore, MutationAction, PlanBuilder, RejectError,
    Stage, StageOutcome,
};

// ---------------------------------------------------------------------------
// Entity — the type of object being processed at a stage
// ---------------------------------------------------------------------------

/// Describes the type of object being processed during stage execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Entity {
    HttpRequest,
    AuctionRequest,
    AuctionResponse,
    AllProcessedBidResponses,
}

impl Entity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Entity::HttpRequest => "http-request",
            Entity::AuctionRequest => "auction-request",
            Entity::AuctionResponse => "auction_response",
            Entity::AllProcessedBidResponses => "all_processed_bid_responses",
        }
    }
}

impl fmt::Display for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// ModuleLabels — labels for per-module metrics
// ---------------------------------------------------------------------------

/// Labels used when recording module-level hook execution metrics.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModuleLabels {
    pub module: String,
    pub stage: String,
    pub hook_code: String,
}

// ---------------------------------------------------------------------------
// HookAction — simplified enum for metric recording
// ---------------------------------------------------------------------------

/// Simplified action for metric counting purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookAction {
    /// Hook did nothing (noop).
    None,
    /// Hook rejected the request.
    Reject,
    /// Hook applied mutations.
    Update,
}

impl HookAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookAction::None => "none",
            HookAction::Reject => "reject",
            HookAction::Update => "update",
        }
    }
}

impl From<InvocationAction> for HookAction {
    fn from(action: InvocationAction) -> Self {
        match action {
            InvocationAction::None => HookAction::None,
            InvocationAction::Reject => HookAction::Reject,
            InvocationAction::Update => HookAction::Update,
        }
    }
}

impl fmt::Display for HookAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// AnalyticsTag — analytics data attached to a hook outcome
// ---------------------------------------------------------------------------

/// A single analytics tag produced by a hook.
#[derive(Debug, Clone)]
pub struct AnalyticsTag {
    pub name: String,
    pub value: Value,
}

/// Collection of analytics tags from a hook invocation.
#[derive(Debug, Clone, Default)]
pub struct AnalyticsTags {
    pub tags: Vec<AnalyticsTag>,
}

impl AnalyticsTags {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, name: impl Into<String>, value: Value) {
        self.tags.push(AnalyticsTag {
            name: name.into(),
            value,
        });
    }

    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }
}

// ---------------------------------------------------------------------------
// HookMetricsRecorder — trait for recording hook execution metrics
// ---------------------------------------------------------------------------

/// Trait for recording hook execution metrics.
///
/// Implementations are passed to the stage executor so that every hook
/// invocation is automatically instrumented.
pub trait HookMetricsRecorder: Send + Sync {
    /// Record that a module hook was called.
    fn record_module_called(&self, labels: &ModuleLabels, duration: Duration);

    /// Record that a module hook timed out.
    fn record_module_timeout(&self, labels: &ModuleLabels);

    /// Record that a module hook failed.
    fn record_module_failed(&self, labels: &ModuleLabels);

    /// Record that a module hook encountered an execution error.
    fn record_module_execution_error(&self, labels: &ModuleLabels);

    /// Record that a module hook succeeded with no changes (noop).
    fn record_module_success_noop(&self, labels: &ModuleLabels);

    /// Record that a module hook succeeded and applied updates.
    fn record_module_success_update(&self, labels: &ModuleLabels);

    /// Record that a module hook successfully rejected the request.
    fn record_module_success_reject(&self, labels: &ModuleLabels);
}

/// No-op metrics recorder for use in tests or when metrics are disabled.
pub struct NoopHookMetrics;

impl HookMetricsRecorder for NoopHookMetrics {
    fn record_module_called(&self, _labels: &ModuleLabels, _duration: Duration) {}
    fn record_module_timeout(&self, _labels: &ModuleLabels) {}
    fn record_module_failed(&self, _labels: &ModuleLabels) {}
    fn record_module_execution_error(&self, _labels: &ModuleLabels) {}
    fn record_module_success_noop(&self, _labels: &ModuleLabels) {}
    fn record_module_success_update(&self, _labels: &ModuleLabels) {}
    fn record_module_success_reject(&self, _labels: &ModuleLabels) {}
}

// ---------------------------------------------------------------------------
// HookStageExecutor — stage-specific execution with metrics
// ---------------------------------------------------------------------------

/// Well-known endpoints used in hook execution.
pub const ENDPOINT_AUCTION: &str = "/openrtb2/auction";
pub const ENDPOINT_AMP: &str = "/openrtb2/amp";

/// Executes hooks at each stage of the auction pipeline, recording metrics
/// and propagating rejections.
///
/// This is the primary entry point for hook execution in the exchange.
/// It wraps [`super::hooks::HookExecutor`] and adds:
///
/// - Per-stage convenience methods (`execute_entrypoint_stage`, etc.)
/// - Entity tagging on stage outcomes
/// - Per-hook metric recording
/// - Rejection reason propagation
///
/// Mirrors Go `hookexecution.hookExecutor` with its `StageExecutor` methods.
pub struct HookStageExecutor {
    plan_builder: Arc<PlanBuilder>,
    endpoint: String,
    account_id: String,
    module_contexts: ModuleContextStore,
    stage_outcomes: Arc<Mutex<Vec<StageOutcome>>>,
    metrics: Arc<dyn HookMetricsRecorder>,
}

impl HookStageExecutor {
    /// Create a new executor for the given endpoint.
    pub fn new(
        plan_builder: Arc<PlanBuilder>,
        endpoint: impl Into<String>,
        metrics: Arc<dyn HookMetricsRecorder>,
    ) -> Self {
        Self {
            plan_builder,
            endpoint: endpoint.into(),
            account_id: String::new(),
            module_contexts: ModuleContextStore::new(),
            stage_outcomes: Arc::new(Mutex::new(Vec::new())),
            metrics,
        }
    }

    /// Set the account ID for metric labelling.
    pub fn set_account_id(&mut self, id: impl Into<String>) {
        self.account_id = id.into();
    }

    /// Retrieve all stage outcomes collected so far.
    pub async fn get_outcomes(&self) -> Vec<StageOutcome> {
        self.stage_outcomes.lock().await.clone()
    }

    // -- Stage-specific methods -----------------------------------------------

    /// Execute the **Entrypoint** stage (HTTP-level, before any parsing).
    ///
    /// Mirrors Go `ExecuteEntrypointStage`.
    pub async fn execute_entrypoint_stage(
        &self,
        body: Value,
    ) -> Result<Value, RejectError> {
        let (payload, mut outcome) = self
            .run_stage(Stage::EntrypointRaw, body)
            .await?;
        outcome.entity = Entity::HttpRequest.to_string();
        self.push_outcome(outcome).await;
        Ok(payload)
    }

    /// Execute the **RawAuctionRequest** stage (raw request body, post-account lookup).
    ///
    /// Mirrors Go `ExecuteRawAuctionStage`.
    pub async fn execute_raw_auction_stage(
        &self,
        body: Value,
    ) -> Result<Value, RejectError> {
        let (payload, mut outcome) = self
            .run_stage(Stage::RawAuctionRequest, body)
            .await?;
        outcome.entity = Entity::AuctionRequest.to_string();
        self.push_outcome(outcome).await;
        Ok(payload)
    }

    /// Execute the **ProcessedAuctionRequest** stage (after FPD enrichment).
    ///
    /// Mirrors Go `ExecuteProcessedAuctionStage`.
    pub async fn execute_processed_auction_stage(
        &self,
        request: Value,
    ) -> Result<Value, RejectError> {
        let (payload, mut outcome) = self
            .run_stage(Stage::ProcessedAuctionRequest, request)
            .await?;
        outcome.entity = Entity::AuctionRequest.to_string();
        self.push_outcome(outcome).await;
        Ok(payload)
    }

    /// Execute the **BidderRequest** stage (per-bidder, before sending).
    ///
    /// Mirrors Go `ExecuteBidderRequestStage`.
    pub async fn execute_bidder_request_stage(
        &self,
        request: Value,
        bidder: &str,
    ) -> Result<Value, RejectError> {
        // Attach bidder name as metadata in payload if not present.
        let mut payload = request;
        if let Some(obj) = payload.as_object_mut() {
            obj.entry("__bidder".to_string())
                .or_insert_with(|| Value::String(bidder.to_owned()));
        }
        let (result, mut outcome) = self
            .run_stage(Stage::BidderRequest, payload)
            .await?;
        outcome.entity = Entity::AuctionRequest.to_string();
        self.push_outcome(outcome).await;
        Ok(result)
    }

    /// Execute the **RawBidderResponse** stage (per-bidder, after response).
    ///
    /// Mirrors Go `ExecuteRawBidderResponseStage`.
    pub async fn execute_raw_bidder_response_stage(
        &self,
        response: Value,
        bidder: &str,
    ) -> Result<Value, RejectError> {
        let mut payload = response;
        if let Some(obj) = payload.as_object_mut() {
            obj.entry("__bidder".to_string())
                .or_insert_with(|| Value::String(bidder.to_owned()));
        }
        let (result, mut outcome) = self
            .run_stage(Stage::RawBidderResponse, payload)
            .await?;
        outcome.entity = Entity::AuctionRequest.to_string();
        self.push_outcome(outcome).await;
        Ok(result)
    }

    /// Execute the **AllProcessedBidResponses** stage.
    ///
    /// This stage does **not** support rejection.
    ///
    /// Mirrors Go `ExecuteAllProcessedBidResponsesStage`.
    pub async fn execute_all_processed_bid_responses_stage(
        &self,
        responses: Value,
    ) -> Value {
        match self
            .run_stage(Stage::AllProcessedBidResponses, responses.clone())
            .await
        {
            Ok((payload, mut outcome)) => {
                outcome.entity = Entity::AllProcessedBidResponses.to_string();
                self.push_outcome(outcome).await;
                payload
            }
            Err(_) => {
                // Should never happen — stage is non-rejectable.
                responses
            }
        }
    }

    /// Execute the **AuctionResponse** stage.
    ///
    /// This stage does **not** support rejection.
    ///
    /// Mirrors Go `ExecuteAuctionResponseStage`.
    pub async fn execute_auction_response_stage(
        &self,
        response: Value,
    ) -> Value {
        match self
            .run_stage(Stage::AuctionResponse, response.clone())
            .await
        {
            Ok((payload, mut outcome)) => {
                outcome.entity = Entity::AuctionResponse.to_string();
                self.push_outcome(outcome).await;
                payload
            }
            Err(_) => response,
        }
    }

    /// Execute the **Exitpoint** stage (final HTTP response, after serialization).
    ///
    /// This stage does **not** support rejection.
    ///
    /// Mirrors Go `ExecuteExitpointStage`.
    pub async fn execute_exitpoint_stage(
        &self,
        response: Value,
    ) -> Value {
        // Exitpoint uses the same AuctionResponse stage key but with a
        // separate entity tag. Some hook systems treat it as a distinct
        // stage; here we reuse the AuctionResponse plan.
        match self
            .run_stage(Stage::AuctionResponse, response.clone())
            .await
        {
            Ok((payload, mut outcome)) => {
                outcome.entity = "exitpoint".to_string();
                self.push_outcome(outcome).await;
                payload
            }
            Err(_) => response,
        }
    }

    // -- Internal helpers -----------------------------------------------------

    /// Core execution logic for any stage.
    ///
    /// Returns the (possibly mutated) payload together with the stage outcome
    /// (not yet pushed — the caller sets the entity and pushes).
    ///
    /// On rejection the outcome IS pushed internally before returning `Err`.
    async fn run_stage(
        &self,
        stage: Stage,
        mut payload: Value,
    ) -> Result<(Value, StageOutcome), RejectError> {
        let plan = self.plan_builder.plan_for_stage(stage, &self.endpoint);
        if plan.is_empty() {
            return Ok((
                payload,
                StageOutcome {
                    stage: stage.to_string(),
                    ..Default::default()
                },
            ));
        }

        let mut stage_outcome = StageOutcome {
            stage: stage.to_string(),
            ..Default::default()
        };

        for group in &plan {
            let (group_outcome, new_payload, reject) =
                self.execute_group_with_metrics(stage, group, payload).await;
            stage_outcome.execution_time += group_outcome.execution_time;
            stage_outcome.groups.push(group_outcome);

            if let Some(reject_err) = reject {
                return Err(reject_err);
            }
            payload = new_payload;
        }

        Ok((payload, stage_outcome))
    }

    /// Execute a single group of hooks with per-hook metric recording.
    async fn execute_group_with_metrics(
        &self,
        stage: Stage,
        group: &HookGroup,
        payload: Value,
    ) -> (GroupOutcome, Value, Option<RejectError>) {
        let mut group_outcome = GroupOutcome::default();
        let mut current_payload = payload.clone();

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
                let start = Instant::now();
                let result =
                    tokio::time::timeout(timeout, hook.handle(&inv_ctx, payload_snap)).await;
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

        let mut reject: Option<RejectError> = None;
        for handle in handles {
            let (hook_id, result, elapsed) = match handle.await {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!("hook task panicked: {e}");
                    continue;
                }
            };

            if elapsed > group_outcome.execution_time {
                group_outcome.execution_time = elapsed;
            }

            // Build module labels for metric recording.
            let labels = ModuleLabels {
                module: hook_id.module_code.replace('.', "_").replace('-', "_"),
                stage: stage.to_string(),
                hook_code: hook_id.hook_impl_code.clone(),
            };

            // Always record "called".
            self.metrics.record_module_called(&labels, elapsed);

            let hook_outcome = match result {
                Ok(hr) => {
                    if !hr.module_context.is_empty() {
                        self.module_contexts
                            .put(&hook_id.module_code, hr.module_context.clone())
                            .await;
                    }

                    if hr.reject {
                        if stage.is_rejectable() {
                            self.metrics.record_module_success_reject(&labels);
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
                            self.metrics.record_module_execution_error(&labels);
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
                        self.metrics.record_module_success_update(&labels);
                        let mut successful = 0usize;
                        let mut warnings = hr.warnings.clone();
                        for mutation in hr.change_set.mutations() {
                            match mutation.apply(current_payload.clone()) {
                                Ok(p) => {
                                    current_payload = p;
                                    successful += 1;
                                }
                                Err(e) => {
                                    warnings
                                        .push(format!("failed to apply hook mutation: {}", e));
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
                            debug_messages: hr.debug_messages,
                            execution_time: elapsed,
                        }
                    } else {
                        self.metrics.record_module_success_noop(&labels);
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
                        HookError::Timeout => {
                            self.metrics.record_module_timeout(&labels);
                            (InvocationStatus::Timeout, hook_err.to_string())
                        }
                        HookError::Failure(_) => {
                            self.metrics.record_module_failed(&labels);
                            (InvocationStatus::Failure, hook_err.to_string())
                        }
                        HookError::Internal(_) => {
                            self.metrics.record_module_execution_error(&labels);
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

impl fmt::Debug for HookStageExecutor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HookStageExecutor")
            .field("endpoint", &self.endpoint)
            .field("account_id", &self.account_id)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};

    // -- Counting metrics recorder for tests ----------------------------------

    struct CountingMetrics {
        called: AtomicU64,
        timeout: AtomicU64,
        failed: AtomicU64,
        exec_error: AtomicU64,
        noop: AtomicU64,
        update: AtomicU64,
        reject: AtomicU64,
    }

    impl CountingMetrics {
        fn new() -> Self {
            Self {
                called: AtomicU64::new(0),
                timeout: AtomicU64::new(0),
                failed: AtomicU64::new(0),
                exec_error: AtomicU64::new(0),
                noop: AtomicU64::new(0),
                update: AtomicU64::new(0),
                reject: AtomicU64::new(0),
            }
        }
    }

    impl HookMetricsRecorder for CountingMetrics {
        fn record_module_called(&self, _labels: &ModuleLabels, _duration: Duration) {
            self.called.fetch_add(1, Ordering::Relaxed);
        }
        fn record_module_timeout(&self, _labels: &ModuleLabels) {
            self.timeout.fetch_add(1, Ordering::Relaxed);
        }
        fn record_module_failed(&self, _labels: &ModuleLabels) {
            self.failed.fetch_add(1, Ordering::Relaxed);
        }
        fn record_module_execution_error(&self, _labels: &ModuleLabels) {
            self.exec_error.fetch_add(1, Ordering::Relaxed);
        }
        fn record_module_success_noop(&self, _labels: &ModuleLabels) {
            self.noop.fetch_add(1, Ordering::Relaxed);
        }
        fn record_module_success_update(&self, _labels: &ModuleLabels) {
            self.update.fetch_add(1, Ordering::Relaxed);
        }
        fn record_module_success_reject(&self, _labels: &ModuleLabels) {
            self.reject.fetch_add(1, Ordering::Relaxed);
        }
    }

    // -- Test hooks ------------------------------------------------------------

    struct NoopHook {
        id: String,
        stages: Vec<Stage>,
    }

    #[async_trait]
    impl Hook for NoopHook {
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
            Ok(HookResult::noop())
        }
    }

    struct MutatingHook {
        id: String,
        stages: Vec<Stage>,
    }

    #[async_trait]
    impl Hook for MutatingHook {
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
            use super::super::hooks::Mutation;
            let mut result = HookResult::noop();
            result.change_set.add_mutation(Mutation::new(
                MutationAction::Update,
                vec!["injected".to_string()],
                Box::new(|mut v| {
                    v["injected"] = json!(true);
                    Ok(v)
                }),
            ));
            Ok(result)
        }
    }

    struct RejectingHook;

    #[async_trait]
    impl Hook for RejectingHook {
        fn id(&self) -> &str {
            "vendor.rejecting"
        }
        fn stages(&self) -> &[Stage] {
            &[
                Stage::EntrypointRaw,
                Stage::RawAuctionRequest,
                Stage::BidderRequest,
            ]
        }
        async fn handle(
            &self,
            _ctx: &InvocationContext,
            _payload: Value,
        ) -> Result<HookResult, HookError> {
            Ok(HookResult::rejection(300, "blocked by policy"))
        }
    }

    struct FailingHook;

    #[async_trait]
    impl Hook for FailingHook {
        fn id(&self) -> &str {
            "vendor.failing"
        }
        fn stages(&self) -> &[Stage] {
            &[Stage::RawAuctionRequest]
        }
        async fn handle(
            &self,
            _ctx: &InvocationContext,
            _payload: Value,
        ) -> Result<HookResult, HookError> {
            Err(HookError::Failure("something went wrong".into()))
        }
    }

    struct SlowHook;

    #[async_trait]
    impl Hook for SlowHook {
        fn id(&self) -> &str {
            "vendor.slow"
        }
        fn stages(&self) -> &[Stage] {
            &[Stage::RawAuctionRequest]
        }
        async fn handle(
            &self,
            _ctx: &InvocationContext,
            _payload: Value,
        ) -> Result<HookResult, HookError> {
            tokio::time::sleep(Duration::from_secs(10)).await;
            Ok(HookResult::noop())
        }
    }

    // -- Helpers ---------------------------------------------------------------

    fn make_executor(
        repo: HookRepository,
        metrics: Arc<CountingMetrics>,
    ) -> HookStageExecutor {
        let plan = Arc::new(
            PlanBuilder::new(Arc::new(repo), true)
                .with_default_group_timeout(Duration::from_millis(500)),
        );
        HookStageExecutor::new(plan, ENDPOINT_AUCTION, metrics)
    }

    // -- Tests ----------------------------------------------------------------

    #[test]
    fn entity_display() {
        assert_eq!(Entity::HttpRequest.as_str(), "http-request");
        assert_eq!(Entity::AuctionRequest.as_str(), "auction-request");
        assert_eq!(Entity::AuctionResponse.as_str(), "auction_response");
        assert_eq!(
            Entity::AllProcessedBidResponses.as_str(),
            "all_processed_bid_responses"
        );
    }

    #[test]
    fn hook_action_from_invocation_action() {
        assert_eq!(HookAction::from(InvocationAction::None), HookAction::None);
        assert_eq!(
            HookAction::from(InvocationAction::Reject),
            HookAction::Reject
        );
        assert_eq!(
            HookAction::from(InvocationAction::Update),
            HookAction::Update
        );
    }

    #[test]
    fn hook_action_display() {
        assert_eq!(HookAction::None.to_string(), "none");
        assert_eq!(HookAction::Reject.to_string(), "reject");
        assert_eq!(HookAction::Update.to_string(), "update");
    }

    #[test]
    fn analytics_tags_empty_and_add() {
        let mut tags = AnalyticsTags::new();
        assert!(tags.is_empty());
        tags.add("foo", json!(42));
        assert!(!tags.is_empty());
        assert_eq!(tags.tags.len(), 1);
        assert_eq!(tags.tags[0].name, "foo");
    }

    #[test]
    fn noop_hook_metrics_does_not_panic() {
        let m = NoopHookMetrics;
        let labels = ModuleLabels {
            module: "test".into(),
            stage: "entrypoint".into(),
            hook_code: "code".into(),
        };
        m.record_module_called(&labels, Duration::from_millis(1));
        m.record_module_timeout(&labels);
        m.record_module_failed(&labels);
        m.record_module_execution_error(&labels);
        m.record_module_success_noop(&labels);
        m.record_module_success_update(&labels);
        m.record_module_success_reject(&labels);
    }

    #[tokio::test]
    async fn entrypoint_stage_noop_records_metrics() {
        let metrics = Arc::new(CountingMetrics::new());
        let mut repo = HookRepository::new();
        repo.register(Arc::new(NoopHook {
            id: "vendor.noop".into(),
            stages: vec![Stage::EntrypointRaw],
        }))
        .unwrap();

        let exec = make_executor(repo, Arc::clone(&metrics));
        let result = exec.execute_entrypoint_stage(json!({"a": 1})).await.unwrap();
        assert_eq!(result["a"], 1);
        assert_eq!(metrics.called.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.noop.load(Ordering::Relaxed), 1);

        let outcomes = exec.get_outcomes().await;
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].entity, "http-request");
    }

    #[tokio::test]
    async fn raw_auction_stage_mutation_records_update() {
        let metrics = Arc::new(CountingMetrics::new());
        let mut repo = HookRepository::new();
        repo.register(Arc::new(MutatingHook {
            id: "vendor.mutate".into(),
            stages: vec![Stage::RawAuctionRequest],
        }))
        .unwrap();

        let exec = make_executor(repo, Arc::clone(&metrics));
        let result = exec.execute_raw_auction_stage(json!({"x": 1})).await.unwrap();
        assert_eq!(result["injected"], true);
        assert_eq!(result["x"], 1);
        assert_eq!(metrics.called.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.update.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn bidder_request_stage_reject_propagates() {
        let metrics = Arc::new(CountingMetrics::new());
        let mut repo = HookRepository::new();
        repo.register(Arc::new(RejectingHook)).unwrap();

        let exec = make_executor(repo, Arc::clone(&metrics));
        let err = exec
            .execute_bidder_request_stage(json!({}), "appnexus")
            .await
            .unwrap_err();
        assert_eq!(err.nbr, 300);
        assert_eq!(err.stage, "bidder_request");
        assert_eq!(metrics.reject.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn processed_auction_stage_runs() {
        let metrics = Arc::new(CountingMetrics::new());
        let mut repo = HookRepository::new();
        repo.register(Arc::new(NoopHook {
            id: "vendor.proc".into(),
            stages: vec![Stage::ProcessedAuctionRequest],
        }))
        .unwrap();

        let exec = make_executor(repo, Arc::clone(&metrics));
        let result = exec
            .execute_processed_auction_stage(json!({"imp": []}))
            .await
            .unwrap();
        assert_eq!(result["imp"], json!([]));
        assert_eq!(metrics.noop.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn raw_bidder_response_stage_runs() {
        let metrics = Arc::new(CountingMetrics::new());
        let mut repo = HookRepository::new();
        repo.register(Arc::new(MutatingHook {
            id: "vendor.resp".into(),
            stages: vec![Stage::RawBidderResponse],
        }))
        .unwrap();

        let exec = make_executor(repo, Arc::clone(&metrics));
        let result = exec
            .execute_raw_bidder_response_stage(json!({"seatbid": []}), "rubicon")
            .await
            .unwrap();
        assert_eq!(result["injected"], true);
        assert_eq!(metrics.update.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn all_processed_bid_responses_stage_non_rejectable() {
        let metrics = Arc::new(CountingMetrics::new());
        let mut repo = HookRepository::new();
        // Register a hook that tries to reject but at a non-rejectable stage.
        struct TryRejectHook;
        #[async_trait]
        impl Hook for TryRejectHook {
            fn id(&self) -> &str {
                "vendor.try_reject"
            }
            fn stages(&self) -> &[Stage] {
                &[Stage::AllProcessedBidResponses]
            }
            async fn handle(
                &self,
                _ctx: &InvocationContext,
                _payload: Value,
            ) -> Result<HookResult, HookError> {
                Ok(HookResult::rejection(42, "should not work"))
            }
        }
        repo.register(Arc::new(TryRejectHook)).unwrap();

        let exec = make_executor(repo, Arc::clone(&metrics));
        let result = exec
            .execute_all_processed_bid_responses_stage(json!({"bids": []}))
            .await;
        // Should succeed, not reject.
        assert_eq!(result["bids"], json!([]));
        assert_eq!(metrics.exec_error.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn auction_response_stage_runs() {
        let metrics = Arc::new(CountingMetrics::new());
        let mut repo = HookRepository::new();
        repo.register(Arc::new(NoopHook {
            id: "vendor.auc_resp".into(),
            stages: vec![Stage::AuctionResponse],
        }))
        .unwrap();

        let exec = make_executor(repo, Arc::clone(&metrics));
        let result = exec
            .execute_auction_response_stage(json!({"id": "123"}))
            .await;
        assert_eq!(result["id"], "123");
        assert_eq!(metrics.noop.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn failing_hook_records_failure_metric() {
        let metrics = Arc::new(CountingMetrics::new());
        let mut repo = HookRepository::new();
        repo.register(Arc::new(FailingHook)).unwrap();

        let exec = make_executor(repo, Arc::clone(&metrics));
        let result = exec.execute_raw_auction_stage(json!({})).await.unwrap();
        // Payload unchanged on failure.
        assert_eq!(result, json!({}));
        assert_eq!(metrics.called.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.failed.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn timeout_hook_records_timeout_metric() {
        let metrics = Arc::new(CountingMetrics::new());
        let mut repo = HookRepository::new();
        repo.register(Arc::new(SlowHook)).unwrap();

        let plan = Arc::new(
            PlanBuilder::new(Arc::new(repo), true)
                .with_default_group_timeout(Duration::from_millis(50)),
        );
        let exec = HookStageExecutor::new(plan, ENDPOINT_AUCTION, Arc::clone(&metrics));

        let result = exec.execute_raw_auction_stage(json!({})).await.unwrap();
        assert_eq!(result, json!({}));
        assert_eq!(metrics.timeout.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn empty_plan_no_metrics_recorded() {
        let metrics = Arc::new(CountingMetrics::new());
        let repo = HookRepository::new(); // no hooks registered
        let exec = make_executor(repo, Arc::clone(&metrics));

        let result = exec.execute_entrypoint_stage(json!({"ok": true})).await.unwrap();
        assert_eq!(result["ok"], true);
        assert_eq!(metrics.called.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn set_account_id_propagates() {
        let metrics = Arc::new(CountingMetrics::new());
        let repo = HookRepository::new();
        let mut exec = make_executor(repo, metrics);
        exec.set_account_id("acct-123");
        assert_eq!(exec.account_id, "acct-123");
    }

    #[tokio::test]
    async fn debug_impl_does_not_panic() {
        let metrics = Arc::new(CountingMetrics::new());
        let repo = HookRepository::new();
        let exec = make_executor(repo, metrics);
        let _ = format!("{:?}", exec);
    }

    #[test]
    fn module_labels_equality() {
        let l1 = ModuleLabels {
            module: "a".into(),
            stage: "b".into(),
            hook_code: "c".into(),
        };
        let l2 = l1.clone();
        assert_eq!(l1, l2);
    }
}
