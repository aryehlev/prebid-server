//! Privacy enforcement module for Prebid Server.
//!
//! Mirrors Go `privacy/` package — CCPA, LMT, activity control,
//! scrubber, and policy enforcement.

pub mod ccpa;
pub mod lmt;
pub mod activity;
pub mod scrubber;

/// Activity defines Prebid Server actions which can be controlled directly
/// by the publisher or via privacy policies.
/// Mirrors Go `privacy.Activity`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Activity {
    SyncUser,
    FetchBids,
    EnrichUserFPD,
    ReportAnalytics,
    TransmitUserFPD,
    TransmitPreciseGeo,
    TransmitUniqueRequestIDs,
    TransmitTIDs,
}

impl std::fmt::Display for Activity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Activity::SyncUser => write!(f, "syncUser"),
            Activity::FetchBids => write!(f, "fetchBids"),
            Activity::EnrichUserFPD => write!(f, "enrichUfpd"),
            Activity::ReportAnalytics => write!(f, "reportAnalytics"),
            Activity::TransmitUserFPD => write!(f, "transmitUfpd"),
            Activity::TransmitPreciseGeo => write!(f, "transmitPreciseGeo"),
            Activity::TransmitUniqueRequestIDs => write!(f, "transmitUniqueRequestIds"),
            Activity::TransmitTIDs => write!(f, "transmitTid"),
        }
    }
}

/// ActivityResult represents the outcome of an activity evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityResult {
    Abstain,
    Allow,
    Deny,
}

/// PolicyEnforcer determines if PII should be removed or anonymized per the policy.
/// Mirrors Go `privacy.PolicyEnforcer`.
pub trait PolicyEnforcer {
    fn can_enforce(&self) -> bool;
    fn should_enforce(&self, bidder: &str) -> bool;
}

/// NilPolicyEnforcer always returns false — no enforcement.
pub struct NilPolicyEnforcer;

impl PolicyEnforcer for NilPolicyEnforcer {
    fn can_enforce(&self) -> bool { false }
    fn should_enforce(&self, _bidder: &str) -> bool { false }
}

/// EnabledPolicyEnforcer decorates a PolicyEnforcer with an enabled flag.
pub struct EnabledPolicyEnforcer<E: PolicyEnforcer> {
    pub enabled: bool,
    pub enforcer: E,
}

impl<E: PolicyEnforcer> PolicyEnforcer for EnabledPolicyEnforcer<E> {
    fn can_enforce(&self) -> bool {
        self.enforcer.can_enforce()
    }
    fn should_enforce(&self, bidder: &str) -> bool {
        if self.enabled {
            self.enforcer.should_enforce(bidder)
        } else {
            false
        }
    }
}

/// PolicyWriter mutates an OpenRTB bid request with regulatory information.
/// Mirrors Go `privacy.PolicyWriter`.
pub trait PolicyWriter {
    fn write(&self, req: &mut openrtb::BidRequest) -> Result<(), String>;
}

/// NilPolicyWriter performs no action.
pub struct NilPolicyWriter;

impl PolicyWriter for NilPolicyWriter {
    fn write(&self, _req: &mut openrtb::BidRequest) -> Result<(), String> {
        Ok(())
    }
}

/// Component represents a named entity for activity control evaluation.
/// Mirrors Go `privacy.Component`.
#[derive(Debug, Clone)]
pub struct Component {
    pub component_type: String,
    pub name: String,
}

impl Component {
    pub fn new(component_type: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            component_type: component_type.into(),
            name: name.into(),
        }
    }

    pub fn matches_name(&self, v: &str) -> bool {
        self.name.eq_ignore_ascii_case(v)
    }

    pub fn matches_type(&self, v: &str) -> bool {
        self.component_type.eq_ignore_ascii_case(v)
    }
}

/// Well-known component types.
pub const COMPONENT_TYPE_BIDDER: &str = "bidder";
pub const COMPONENT_TYPE_ANALYTICS: &str = "analytics";
pub const COMPONENT_TYPE_RTD: &str = "rtd";
pub const COMPONENT_TYPE_GENERAL: &str = "general";

/// Policies contains privacy signals for non-OpenRTB activities.
/// Mirrors Go `privacy.Policies`.
#[derive(Debug, Clone, Default)]
pub struct Policies {
    pub gpp_sid: Vec<i8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_activity_display() {
        assert_eq!(Activity::SyncUser.to_string(), "syncUser");
        assert_eq!(Activity::TransmitTIDs.to_string(), "transmitTid");
    }

    #[test]
    fn test_nil_policy_enforcer() {
        let e = NilPolicyEnforcer;
        assert!(!e.can_enforce());
        assert!(!e.should_enforce("appnexus"));
    }

    #[test]
    fn test_component_matching() {
        let c = Component::new("bidder", "AppNexus");
        assert!(c.matches_name("appnexus"));
        assert!(c.matches_name("APPNEXUS"));
        assert!(!c.matches_name("rubicon"));
        assert!(c.matches_type("bidder"));
        assert!(c.matches_type("BIDDER"));
    }
}
