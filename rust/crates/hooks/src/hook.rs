//! The core [`Hook`] trait and associated types.

use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;

use crate::analytics::AnalyticsTags;
use crate::changeset::ChangeSet;
use crate::reject::HookError;

/// Arbitrary data passed between a module's hooks across different stages.
pub type ModuleContext = HashMap<String, Value>;

/// Invocation context passed to every hook.
#[derive(Debug, Clone, Default)]
pub struct ModuleInvocationContext {
    /// The account identifier, if known.
    pub account_id: Option<String>,
    /// Account-level module configuration, as raw JSON.
    pub account_config: Option<Value>,
    /// The endpoint path that originated this invocation (e.g. `/openrtb2/auction`).
    pub endpoint: String,
    /// Inter-stage module state.
    pub module_context: ModuleContext,
    /// The identifier distinguishing between multiple hooks from the same module.
    pub hook_impl_code: String,
}

/// The result of a single hook invocation.
///
/// Mirrors `hooks/hookstage/invocation.go::HookResult` from the Go code.
#[derive(Debug)]
pub struct HookResult<T> {
    /// Whether the hook requests that the request be rejected at this stage.
    pub reject: bool,
    /// If `reject` is true, the NBR code to report back to the client.
    pub nbr_code: i32,
    /// Free-form message supplied by the hook.
    pub message: String,
    /// Mutations the hook wants applied to the payload.
    pub changeset: ChangeSet<T>,
    /// Any errors produced.
    pub errors: Vec<String>,
    /// Any warnings produced.
    pub warnings: Vec<String>,
    /// Debug messages for diagnostics.
    pub debug_messages: Vec<String>,
    /// Analytics tags emitted by the hook.
    pub analytics_tags: AnalyticsTags,
    /// State the hook wants to carry over to later stages.
    pub module_context: ModuleContext,
}

impl<T> Default for HookResult<T> {
    fn default() -> Self {
        Self {
            reject: false,
            nbr_code: 0,
            message: String::new(),
            changeset: ChangeSet::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
            debug_messages: Vec::new(),
            analytics_tags: AnalyticsTags::default(),
            module_context: ModuleContext::new(),
        }
    }
}

impl<T> HookResult<T> {
    /// Construct a new empty result.
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a `HookResult` that rejects the request with the given NBR.
    pub fn rejected(nbr_code: i32, message: impl Into<String>) -> Self {
        let mut r = Self::new();
        r.reject = true;
        r.nbr_code = nbr_code;
        r.message = message.into();
        r
    }
}

/// The core asynchronous hook trait.
///
/// Implementors operate on a specific payload type `PayloadT`. Compared to
/// the Go version, the different per-stage interfaces (`Entrypoint`,
/// `RawAuctionRequest`, ...) all collapse into this single generic trait.
#[async_trait]
pub trait Hook<PayloadT>: Send + Sync
where
    PayloadT: Send + 'static,
{
    /// Handle a single hook invocation.
    async fn handle(
        &self,
        ctx: &ModuleInvocationContext,
        payload: PayloadT,
    ) -> Result<HookResult<PayloadT>, HookError>;
}

/// Boxed, dynamically dispatched handle to a [`Hook`].
pub type HookHandle<P> = std::sync::Arc<dyn Hook<P>>;

/// Wraps a hook with the module metadata that identifies it within a plan.
#[derive(Clone)]
pub struct HookWrapper<P>
where
    P: Send + 'static,
{
    /// The module name (e.g. `"vendor.module_name"`).
    pub module: String,
    /// The hook implementation code used for metrics and logging.
    pub code: String,
    /// The hook instance itself.
    pub hook: HookHandle<P>,
}

impl<P: Send + 'static> HookWrapper<P> {
    /// Construct a new wrapper.
    pub fn new(module: impl Into<String>, code: impl Into<String>, hook: HookHandle<P>) -> Self {
        Self {
            module: module.into(),
            code: code.into(),
            hook,
        }
    }
}
