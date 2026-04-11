//! Execution plan builder for hook stages.
//!
//! Mirrors Go `hooks/plan.go` — builds ordered execution plans from
//! host and account-level hook configuration.

use std::time::Duration;
use crate::Stage;

/// A hook wrapper with module name and hook code.
#[derive(Debug, Clone)]
pub struct HookWrapper<T> {
    pub module: String,
    pub code: String,
    pub hook: T,
}

/// A group of hooks to execute concurrently with a shared timeout.
#[derive(Debug, Clone)]
pub struct Group<T> {
    pub timeout: Duration,
    pub hooks: Vec<HookWrapper<T>>,
}

/// An execution plan is a sequence of groups.
pub type Plan<T> = Vec<Group<T>>;

/// Configuration for a single hook in the execution plan.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct HookSequenceConfig {
    #[serde(default, rename = "module_code")]
    pub module_code: String,
    #[serde(default, rename = "hook_impl_code")]
    pub hook_impl_code: String,
}

/// Configuration for a group of hooks.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct HookExecutionGroup {
    #[serde(default)]
    pub timeout: u64,
    #[serde(default, rename = "hook_sequence")]
    pub hook_sequence: Vec<HookSequenceConfig>,
}

/// Configuration for hooks at a specific stage.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct StageConfig {
    #[serde(default)]
    pub groups: Vec<HookExecutionGroup>,
}

/// Endpoint-level hook execution plan.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct EndpointConfig {
    #[serde(default)]
    pub stages: std::collections::HashMap<String, StageConfig>,
}

/// Full hook execution plan configuration.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct HookExecutionPlan {
    #[serde(default)]
    pub endpoints: std::collections::HashMap<String, EndpointConfig>,
}

/// Hooks configuration (host-level).
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct HooksConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub host_execution_plan: HookExecutionPlan,
    #[serde(default)]
    pub default_account_execution_plan: HookExecutionPlan,
}

/// Build an execution plan for a given stage and endpoint.
pub fn get_plan_for_stage<T: Clone>(
    endpoint: &str,
    stage: Stage,
    plan_cfg: &HookExecutionPlan,
    get_hook: &dyn Fn(&str) -> Option<T>,
) -> Plan<T> {
    let stage_name = stage.as_str();
    let endpoint_cfg = match plan_cfg.endpoints.get(endpoint) {
        Some(ec) => ec,
        None => return Vec::new(),
    };
    let stage_cfg = match endpoint_cfg.stages.get(stage_name) {
        Some(sc) => sc,
        None => return Vec::new(),
    };

    let mut plan = Vec::new();
    for group_cfg in &stage_cfg.groups {
        let timeout = Duration::from_millis(group_cfg.timeout);
        let mut hooks = Vec::new();

        for hook_cfg in &group_cfg.hook_sequence {
            if let Some(hook) = get_hook(&hook_cfg.module_code) {
                hooks.push(HookWrapper {
                    module: hook_cfg.module_code.clone(),
                    code: hook_cfg.hook_impl_code.clone(),
                    hook,
                });
            } else {
                tracing::warn!(
                    "Not found hook while building execution plan: {} {}",
                    hook_cfg.module_code,
                    hook_cfg.hook_impl_code
                );
            }
        }

        if !hooks.is_empty() {
            plan.push(Group { timeout, hooks });
        }
    }

    plan
}

/// Build a merged plan from host and account execution plans.
pub fn get_merged_plan<T: Clone>(
    endpoint: &str,
    stage: Stage,
    host_plan: &HookExecutionPlan,
    account_plan: &HookExecutionPlan,
    get_hook: &dyn Fn(&str) -> Option<T>,
) -> Plan<T> {
    let mut plan = get_plan_for_stage(endpoint, stage, host_plan, get_hook);
    plan.extend(get_plan_for_stage(endpoint, stage, account_plan, get_hook));
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_plan_empty() {
        let cfg = HookExecutionPlan::default();
        let plan: Plan<String> = get_plan_for_stage("/openrtb2/auction", Stage::Entrypoint, &cfg, &|_| None);
        assert!(plan.is_empty());
    }

    #[test]
    fn test_get_plan_with_hooks() {
        let mut stages = std::collections::HashMap::new();
        stages.insert("entrypoint".to_string(), StageConfig {
            groups: vec![HookExecutionGroup {
                timeout: 500,
                hook_sequence: vec![HookSequenceConfig {
                    module_code: "prebid.ortb2blocking".to_string(),
                    hook_impl_code: "block".to_string(),
                }],
            }],
        });
        let mut endpoints = std::collections::HashMap::new();
        endpoints.insert("/openrtb2/auction".to_string(), EndpointConfig { stages });
        let cfg = HookExecutionPlan { endpoints };

        let plan: Plan<String> = get_plan_for_stage(
            "/openrtb2/auction",
            Stage::Entrypoint,
            &cfg,
            &|name| {
                if name == "prebid.ortb2blocking" {
                    Some("hook_instance".to_string())
                } else {
                    None
                }
            },
        );

        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].hooks.len(), 1);
        assert_eq!(plan[0].hooks[0].module, "prebid.ortb2blocking");
        assert_eq!(plan[0].timeout, Duration::from_millis(500));
    }
}
