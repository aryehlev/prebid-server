//! Activity control system.
//!
//! Mirrors Go `privacy/activitycontrol.go` + `privacy/rule_condition.go` —
//! publisher-controlled activity permissions with rule-based evaluation.

use std::collections::HashMap;
use crate::{Activity, ActivityResult, Component, Policies};

/// Rule evaluates whether an activity is allowed for a given target.
/// Mirrors Go `privacy.Rule`.
pub trait Rule: Send + Sync {
    fn evaluate(&self, target: &Component, gpp_sid: &[i8]) -> ActivityResult;
}

/// ConditionRule matches based on component name, type, and GPP SID.
/// Mirrors Go `privacy.ConditionRule`.
#[derive(Debug, Clone)]
pub struct ConditionRule {
    pub result: ActivityResult,
    pub component_name: Vec<String>,
    pub component_type: Vec<String>,
    pub gpp_sid: Vec<i8>,
}

impl Rule for ConditionRule {
    fn evaluate(&self, target: &Component, gpp_sid: &[i8]) -> ActivityResult {
        if !evaluate_component_name(target, &self.component_name) {
            return ActivityResult::Abstain;
        }
        if !evaluate_component_type(target, &self.component_type) {
            return ActivityResult::Abstain;
        }
        if !evaluate_gpp_sid(&self.gpp_sid, gpp_sid) {
            return ActivityResult::Abstain;
        }
        self.result
    }
}

fn evaluate_component_name(target: &Component, names: &[String]) -> bool {
    if names.is_empty() {
        return true; // no clauses defined = match
    }
    names.iter().any(|n| target.matches_name(n))
}

fn evaluate_component_type(target: &Component, types: &[String]) -> bool {
    if types.is_empty() {
        return true; // no clauses defined = match
    }
    types.iter().any(|t| target.matches_type(t))
}

fn evaluate_gpp_sid(rule_sids: &[i8], request_sids: &[i8]) -> bool {
    if rule_sids.is_empty() {
        return true; // no clauses defined = match
    }
    for &x in request_sids {
        for &y in rule_sids {
            if x == y {
                return true;
            }
        }
    }
    false
}

/// ActivityPlan contains rules and a default result for one activity.
/// Mirrors Go `privacy.ActivityPlan`.
#[derive(Debug, Clone)]
pub struct ActivityPlan {
    pub default_result: bool,
    pub rules: Vec<ConditionRule>,
}

impl ActivityPlan {
    /// Evaluate the plan for a target component.
    pub fn evaluate(&self, target: &Component, gpp_sid: &[i8]) -> bool {
        for rule in &self.rules {
            let result = rule.evaluate(target, gpp_sid);
            match result {
                ActivityResult::Allow => return true,
                ActivityResult::Deny => return false,
                ActivityResult::Abstain => continue,
            }
        }
        self.default_result
    }
}

/// ActivityControl manages activity plans for all activity types.
/// Mirrors Go `privacy.ActivityControl`.
#[derive(Debug, Clone, Default)]
pub struct ActivityControl {
    plans: HashMap<Activity, ActivityPlan>,
}

const DEFAULT_ACTIVITY_RESULT: bool = true;

impl ActivityControl {
    pub fn new() -> Self {
        Self { plans: HashMap::new() }
    }

    /// Register a plan for an activity.
    pub fn set_plan(&mut self, activity: Activity, plan: ActivityPlan) {
        self.plans.insert(activity, plan);
    }

    /// Check if an activity is allowed for a target component.
    /// Mirrors Go `ActivityControl.Allow`.
    pub fn allow(&self, activity: Activity, target: &Component, policies: &Policies) -> bool {
        match self.plans.get(&activity) {
            Some(plan) => plan.evaluate(target, &policies.gpp_sid),
            None => DEFAULT_ACTIVITY_RESULT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_condition_rule_name_match() {
        let rule = ConditionRule {
            result: ActivityResult::Deny,
            component_name: vec!["appnexus".to_string()],
            component_type: vec![],
            gpp_sid: vec![],
        };
        let target = Component::new("bidder", "appnexus");
        assert_eq!(rule.evaluate(&target, &[]), ActivityResult::Deny);
    }

    #[test]
    fn test_condition_rule_name_no_match() {
        let rule = ConditionRule {
            result: ActivityResult::Deny,
            component_name: vec!["rubicon".to_string()],
            component_type: vec![],
            gpp_sid: vec![],
        };
        let target = Component::new("bidder", "appnexus");
        assert_eq!(rule.evaluate(&target, &[]), ActivityResult::Abstain);
    }

    #[test]
    fn test_condition_rule_type_match() {
        let rule = ConditionRule {
            result: ActivityResult::Allow,
            component_name: vec![],
            component_type: vec!["bidder".to_string()],
            gpp_sid: vec![],
        };
        let target = Component::new("bidder", "appnexus");
        assert_eq!(rule.evaluate(&target, &[]), ActivityResult::Allow);
    }

    #[test]
    fn test_activity_plan_default() {
        let plan = ActivityPlan {
            default_result: true,
            rules: vec![],
        };
        let target = Component::new("bidder", "appnexus");
        assert!(plan.evaluate(&target, &[]));
    }

    #[test]
    fn test_activity_plan_deny_rule() {
        let plan = ActivityPlan {
            default_result: true,
            rules: vec![ConditionRule {
                result: ActivityResult::Deny,
                component_name: vec!["appnexus".to_string()],
                component_type: vec![],
                gpp_sid: vec![],
            }],
        };
        let target = Component::new("bidder", "appnexus");
        assert!(!plan.evaluate(&target, &[]));
    }

    #[test]
    fn test_activity_control_no_plan() {
        let ctrl = ActivityControl::new();
        let target = Component::new("bidder", "appnexus");
        let policies = Policies::default();
        assert!(ctrl.allow(Activity::FetchBids, &target, &policies));
    }

    #[test]
    fn test_activity_control_with_plan() {
        let mut ctrl = ActivityControl::new();
        ctrl.set_plan(Activity::SyncUser, ActivityPlan {
            default_result: false,
            rules: vec![ConditionRule {
                result: ActivityResult::Allow,
                component_name: vec!["appnexus".to_string()],
                component_type: vec![],
                gpp_sid: vec![],
            }],
        });
        let policies = Policies::default();
        assert!(ctrl.allow(Activity::SyncUser, &Component::new("bidder", "appnexus"), &policies));
        assert!(!ctrl.allow(Activity::SyncUser, &Component::new("bidder", "rubicon"), &policies));
    }

    #[test]
    fn test_gpp_sid_matching() {
        let rule = ConditionRule {
            result: ActivityResult::Deny,
            component_name: vec![],
            component_type: vec![],
            gpp_sid: vec![6, 7],
        };
        let target = Component::new("bidder", "appnexus");
        // No GPP SIDs in request — rule abstains
        assert_eq!(rule.evaluate(&target, &[]), ActivityResult::Abstain);
        // Matching SID
        assert_eq!(rule.evaluate(&target, &[6]), ActivityResult::Deny);
        // Non-matching SID
        assert_eq!(rule.evaluate(&target, &[5]), ActivityResult::Abstain);
    }
}
