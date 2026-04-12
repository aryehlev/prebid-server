//! Hook rejection and execution error types.

use thiserror::Error;

use crate::stage::Stage;

/// Identifies the hook that produced a particular outcome.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HookId {
    /// The module that owns the hook (e.g. `"vendor.module_name"`).
    pub module_code: String,
    /// Arbitrary hook implementation identifier used for logging/metrics.
    pub hook_impl_code: String,
}

/// A request-rejection signal emitted by a hook.
///
/// Mirrors the Go `RejectError` type.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("module {} (hook {}) rejected the request at stage {stage} with nbr {nbr}", hook.module_code, hook.hook_impl_code)]
pub struct Reject {
    /// The stage at which the rejection happened.
    pub stage: Stage,
    /// The hook that requested the rejection.
    pub hook: HookId,
    /// NBR (no bid reason) code supplied by the hook.
    pub nbr: i32,
    /// Optional human-readable message.
    pub message: String,
}

impl Reject {
    /// Construct a new `Reject` for the given hook, stage and NBR code.
    pub fn new(stage: Stage, hook: HookId, nbr: i32) -> Self {
        Self {
            stage,
            hook,
            nbr,
            message: String::new(),
        }
    }

    /// Attach a human-readable message to the rejection.
    pub fn with_message(mut self, msg: impl Into<String>) -> Self {
        self.message = msg.into();
        self
    }
}

/// The primary error type returned by hook execution.
#[derive(Debug, Error)]
pub enum HookError {
    /// A hook explicitly rejected the request.
    #[error(transparent)]
    Rejected(#[from] Reject),

    /// A hook timed out.
    #[error("hook timed out")]
    Timeout,

    /// A hook returned a generic failure.
    #[error("hook failed: {0}")]
    Failure(String),

    /// Any other execution error.
    #[error("hook execution error: {0}")]
    Other(String),
}

impl HookError {
    /// Whether this error represents an explicit reject.
    pub fn is_reject(&self) -> bool {
        matches!(self, HookError::Rejected(_))
    }
}
