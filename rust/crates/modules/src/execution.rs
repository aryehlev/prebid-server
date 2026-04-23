//! Hook execution engine.
//!
//! Mirrors Go `hooks/hookexecution/execution.go` — executes hooks
//! within groups with timeouts, applies mutations, tracks outcomes.

use std::time::{Duration, Instant};
use crate::hookstage::{ChangeSet, HookResult, MutationType};
use crate::plan::{Group, Plan};
use crate::HookID;

// ---------------------------------------------------------------------------
// Outcome tracking — mirrors Go hookexecution types
// ---------------------------------------------------------------------------

/// Action taken by a hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    Update,
    Reject,
}

/// Status of a hook invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Success,
    ExecutionFailure,
    Timeout,
}

/// Outcome of a single hook invocation.
#[derive(Debug, Clone)]
pub struct HookOutcome {
    pub hook_id: HookID,
    pub status: Status,
    pub action: Action,
    pub message: String,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub debug_messages: Vec<String>,
    pub execution_time: Duration,
}

/// Outcome of a group of hooks.
#[derive(Debug, Clone)]
pub struct GroupOutcome {
    pub invocation_results: Vec<HookOutcome>,
    pub execution_time: Duration,
}

/// Outcome of an entire stage.
#[derive(Debug, Clone)]
pub struct StageOutcome {
    pub groups: Vec<GroupOutcome>,
    pub execution_time: Duration,
}

/// Error returned when a hook rejects the request.
#[derive(Debug, Clone)]
pub struct RejectError {
    pub nbr_code: i32,
    pub hook_id: HookID,
    pub message: String,
}

impl std::fmt::Display for RejectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "rejected by {}: {} (nbr={})", self.hook_id, self.message, self.nbr_code)
    }
}

impl std::error::Error for RejectError {}

// ---------------------------------------------------------------------------
// Synchronous execution — applies mutations from hook results
// ---------------------------------------------------------------------------

/// Apply mutations from a hook result to a payload.
/// Returns the updated payload and a HookOutcome.
pub fn apply_hook_mutations<T>(
    payload: T,
    result: HookResult<T>,
    hook_id: HookID,
    execution_time: Duration,
) -> (T, HookOutcome) {
    let mut outcome = HookOutcome {
        hook_id,
        status: Status::Success,
        action: Action::None,
        message: result.message.clone(),
        errors: result.errors,
        warnings: result.warnings,
        debug_messages: result.debug_messages,
        execution_time,
    };

    if result.reject {
        outcome.action = Action::Reject;
        return (payload, outcome);
    }

    let mut change_set = result.change_set;
    if change_set.is_empty() {
        return (payload, outcome);
    }

    outcome.action = Action::Update;
    let mut current = payload;
    let mut success_count = 0;

    for mutation in change_set.drain() {
        let key = mutation.key.join(".");
        let mut_type = mutation.mut_type;
        match mutation.apply(current) {
            Ok(updated) => {
                current = updated;
                success_count += 1;
                outcome.debug_messages.push(format!(
                    "Hook mutation successfully applied, affected key: {}, mutation type: {}",
                    key, mut_type
                ));
            }
            Err((returned_payload, e)) => {
                outcome.warnings.push(format!("failed to apply hook mutation: {}", e));
                current = returned_payload;
                // Continue with remaining mutations
            }
        }
    }

    if success_count == 0 {
        outcome.status = Status::ExecutionFailure;
    }

    (current, outcome)
}

/// Execute a plan synchronously (no async, no goroutines).
/// Hooks in the same group are executed sequentially.
pub fn execute_plan_sync<T, H, F>(
    plan: &Plan<H>,
    mut payload: T,
    handler: F,
) -> (StageOutcome, T, Option<RejectError>)
where
    F: Fn(&H, T) -> (HookResult<T>, T, Duration, HookID),
{
    let mut stage_outcome = StageOutcome {
        groups: Vec::new(),
        execution_time: Duration::ZERO,
    };

    for group in plan {
        let group_start = Instant::now();
        let mut group_outcome = GroupOutcome {
            invocation_results: Vec::new(),
            execution_time: Duration::ZERO,
        };

        for hw in &group.hooks {
            let (result, updated_payload, exec_time, hook_id) = handler(&hw.hook, payload);
            payload = updated_payload;

            if result.reject {
                let reject_err = RejectError {
                    nbr_code: result.nbr_code,
                    hook_id: hook_id.clone(),
                    message: result.message.clone(),
                };
                let outcome = HookOutcome {
                    hook_id,
                    status: Status::Success,
                    action: Action::Reject,
                    message: result.message,
                    errors: result.errors,
                    warnings: result.warnings,
                    debug_messages: result.debug_messages,
                    execution_time: exec_time,
                };
                group_outcome.invocation_results.push(outcome);
                group_outcome.execution_time = group_start.elapsed();
                stage_outcome.groups.push(group_outcome);
                stage_outcome.execution_time += group_start.elapsed();
                return (stage_outcome, payload, Some(reject_err));
            }

            let (updated, outcome) = apply_hook_mutations(payload, result, hook_id, exec_time);
            payload = updated;
            group_outcome.invocation_results.push(outcome);
        }

        group_outcome.execution_time = group_start.elapsed();
        stage_outcome.execution_time += group_outcome.execution_time;
        stage_outcome.groups.push(group_outcome);
    }

    (stage_outcome, payload, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hookstage::HookResult;

    #[test]
    fn test_apply_hook_mutations_no_changes() {
        let result: HookResult<String> = HookResult::default();
        let hook_id = HookID {
            module_code: "test".to_string(),
            hook_impl_code: "hook1".to_string(),
        };
        let (payload, outcome) = apply_hook_mutations("hello".to_string(), result, hook_id, Duration::from_millis(10));
        assert_eq!(payload, "hello");
        assert_eq!(outcome.action, Action::None);
    }

    #[test]
    fn test_apply_hook_mutations_with_changes() {
        let mut result: HookResult<String> = HookResult::default();
        result.change_set.add_mutation(
            Box::new(|s: String| Ok(format!("{}_updated", s))),
            MutationType::Update,
            vec!["field".to_string()],
        );
        let hook_id = HookID {
            module_code: "test".to_string(),
            hook_impl_code: "hook1".to_string(),
        };
        let (payload, outcome) = apply_hook_mutations("hello".to_string(), result, hook_id, Duration::from_millis(10));
        assert_eq!(payload, "hello_updated");
        assert_eq!(outcome.action, Action::Update);
    }

    #[test]
    fn test_apply_hook_mutations_reject() {
        let mut result: HookResult<String> = HookResult::default();
        result.reject = true;
        result.nbr_code = 100;
        let hook_id = HookID {
            module_code: "test".to_string(),
            hook_impl_code: "hook1".to_string(),
        };
        let (_, outcome) = apply_hook_mutations("hello".to_string(), result, hook_id, Duration::from_millis(10));
        assert_eq!(outcome.action, Action::Reject);
    }
}
