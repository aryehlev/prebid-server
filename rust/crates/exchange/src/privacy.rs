/// Privacy enforcement module covering GDPR, CCPA, COPPA, and LMT.
///
/// Provides a unified `AuctionPrivacyConfig` extracted from a `BidRequest` and
/// a single `check_privacy_for_bidder` function that returns a typed `PrivacyResult`.

/// Privacy policies applied to a request.
#[derive(Debug, Clone, Default)]
pub struct AuctionPrivacyConfig {
    pub gdpr_applies: bool,
    pub consent_string: Option<String>,
    pub us_privacy: Option<String>,
    /// Limit Ad Tracking from the device (`device.lmt`).
    pub lmt: Option<i32>,
    /// Child-directed flag (`regs.coppa`).
    pub coppa: Option<i32>,
}

/// Result of a per-bidder privacy check.
#[derive(Debug, Clone, PartialEq)]
pub enum PrivacyResult {
    Allow,
    BlockGdpr,
    BlockCcpa,
    BlockCoppa,
    BlockLmt,
}

/// Extracts privacy settings from a `BidRequest`.
///
/// - `gdpr_applies` is read from `regs.ext.gdpr` (integer flag).
/// - `consent_string` is read from `user.ext.consent`.
/// - `us_privacy` is read from `regs.us_privacy`.
/// - `lmt` is read from `device.lmt`.
/// - `coppa` is read from `regs.coppa`.
pub fn extract_privacy_config(req: &openrtb::BidRequest) -> AuctionPrivacyConfig {
    let gdpr_applies = req
        .regs
        .as_ref()
        .and_then(|r| r.ext.as_ref())
        .and_then(|e| e.get("gdpr"))
        .and_then(|v| v.as_i64())
        .map(|g| g == 1)
        .unwrap_or(false);

    let consent_string = req
        .user
        .as_ref()
        .and_then(|u| u.ext.as_ref())
        .and_then(|e| e.get("consent"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let us_privacy = req.regs.as_ref().and_then(|r| r.us_privacy.clone());

    let lmt = req.device.as_ref().and_then(|d| d.lmt);

    let coppa = req.regs.as_ref().and_then(|r| r.coppa);

    AuctionPrivacyConfig {
        gdpr_applies,
        consent_string,
        us_privacy,
        lmt,
        coppa,
    }
}

/// Determines whether a bidder should be blocked based on the privacy config.
///
/// Checks are applied in priority order:
/// 1. COPPA — blocks all bidders unconditionally when `regs.coppa == 1`.
/// 2. LMT   — blocks when `device.lmt == 1`.
/// 3. CCPA  — blocks when `us_privacy[2] == 'Y'` (opt-out of sale).
/// 4. GDPR  — blocks when GDPR applies and no valid consent string is present.
pub fn check_privacy_for_bidder(config: &AuctionPrivacyConfig, _bidder: &str) -> PrivacyResult {
    // COPPA blocks all bidders.
    if config.coppa == Some(1) {
        return PrivacyResult::BlockCoppa;
    }

    // LMT blocks when set to 1.
    if config.lmt == Some(1) {
        return PrivacyResult::BlockLmt;
    }

    // CCPA: third character of us_privacy == 'Y' means user opted out of sale.
    if let Some(us_privacy) = &config.us_privacy {
        let chars: Vec<char> = us_privacy.chars().collect();
        if chars.len() >= 3 && chars[2] == 'Y' {
            return PrivacyResult::BlockCcpa;
        }
    }

    // GDPR: if applicable, a non-empty consent string is required.
    if config.gdpr_applies {
        match &config.consent_string {
            None | Some(_) if config.consent_string.as_deref().map(|s| s.is_empty()).unwrap_or(true) => {
                return PrivacyResult::BlockGdpr;
            }
            _ => {}
        }
    }

    PrivacyResult::Allow
}

// ---------------------------------------------------------------------------
// CcpaPolicy — CCPA/US Privacy string enforcement (mirrors Go privacy/ccpa/)
// ---------------------------------------------------------------------------

/// Parsed CCPA/US Privacy policy from the `regs.us_privacy` string.
///
/// The US Privacy string has 4 characters: Version, Notice, OptOutSale, LSPA.
/// Format: `1YNN` where position 3 (index 2) is the opt-out-of-sale flag.
#[derive(Debug, Clone, Default)]
pub struct CcpaPolicy {
    /// Full US Privacy string (e.g. "1YNN").
    pub consent_string: String,
    /// Whether the user opted out of sale (third char == 'Y').
    pub opt_out_sale: bool,
}

impl CcpaPolicy {
    /// Parse a US Privacy string.
    pub fn parse(us_privacy: &str) -> Self {
        let chars: Vec<char> = us_privacy.chars().collect();
        let opt_out = chars.len() >= 3 && chars[2] == 'Y';
        Self {
            consent_string: us_privacy.to_string(),
            opt_out_sale: opt_out,
        }
    }

    /// Whether the request should be blocked under CCPA.
    pub fn should_block(&self) -> bool {
        self.opt_out_sale
    }
}

// ---------------------------------------------------------------------------
// LmtPolicy — Limit Ad Tracking enforcement (mirrors Go privacy/lmt/)
// ---------------------------------------------------------------------------

/// LMT (Limit Ad Tracking) policy from `device.lmt`.
#[derive(Debug, Clone, Default)]
pub struct LmtPolicy {
    /// The raw LMT value from device.lmt.
    pub lmt: Option<i32>,
}

impl LmtPolicy {
    pub fn new(lmt: Option<i32>) -> Self {
        Self { lmt }
    }

    /// Whether LMT is enabled (device.lmt == 1).
    pub fn is_enabled(&self) -> bool {
        self.lmt == Some(1)
    }

    /// Whether the request should be blocked under LMT.
    pub fn should_block(&self) -> bool {
        self.is_enabled()
    }
}

// ---------------------------------------------------------------------------
// ActivityControl — activity-based privacy (mirrors Go privacy/activitycontrol.go)
// ---------------------------------------------------------------------------

/// Activity types that can be controlled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Activity {
    SyncUser,
    FetchBids,
    EnrichUserFPD,
    ReportAnalytics,
    TransmitUserFPD,
    TransmitPreciseGeo,
    TransmitUniqueIds,
    TransmitTid,
}

/// Result of an activity control check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityResult {
    Allow,
    Deny,
    Abstain,
}

/// A component (bidder, analytics, etc.) that is subject to activity rules.
#[derive(Debug, Clone)]
pub struct ActivityComponent {
    pub component_type: String,  // "bidder", "analytics", "rtd"
    pub component_name: String,  // e.g. "appnexus"
}

/// A single activity rule with conditions and an allow/deny result.
#[derive(Debug, Clone)]
pub struct ActivityRule {
    /// Whether this rule allows or denies the activity.
    pub allow: bool,
    /// Conditions that must match for this rule to apply.
    pub conditions: Vec<ActivityCondition>,
}

/// A condition within an activity rule.
#[derive(Debug, Clone)]
pub struct ActivityCondition {
    /// Component names that match (empty = match all).
    pub component_name: Vec<String>,
    /// Component types that match (empty = match all).
    pub component_type: Vec<String>,
}

impl ActivityCondition {
    /// Check if a component matches this condition.
    pub fn matches(&self, component: &ActivityComponent) -> bool {
        let name_ok = self.component_name.is_empty()
            || self.component_name.contains(&component.component_name);
        let type_ok = self.component_type.is_empty()
            || self.component_type.contains(&component.component_type);
        name_ok && type_ok
    }
}

/// Activity control configuration for a specific activity.
#[derive(Debug, Clone, Default)]
pub struct ActivityConfig {
    /// Default result when no rules match.
    pub default_result: bool,
    /// Ordered rules evaluated top-to-bottom.
    pub rules: Vec<ActivityRule>,
}

impl ActivityConfig {
    /// Evaluate whether the activity is allowed for the given component.
    pub fn evaluate(&self, component: &ActivityComponent) -> ActivityResult {
        for rule in &self.rules {
            let matches = rule.conditions.is_empty()
                || rule.conditions.iter().any(|c| c.matches(component));
            if matches {
                return if rule.allow {
                    ActivityResult::Allow
                } else {
                    ActivityResult::Deny
                };
            }
        }
        if self.default_result {
            ActivityResult::Allow
        } else {
            ActivityResult::Abstain
        }
    }
}

/// Full activity control map for all activities.
#[derive(Debug, Clone, Default)]
pub struct ActivityControl {
    pub activities: std::collections::HashMap<Activity, ActivityConfig>,
}

impl ActivityControl {
    /// Check if an activity is allowed for a component.
    pub fn is_allowed(&self, activity: Activity, component: &ActivityComponent) -> bool {
        match self.activities.get(&activity) {
            Some(config) => config.evaluate(component) != ActivityResult::Deny,
            None => true, // No config = allowed
        }
    }
}

/// Sanitize a `BidRequest` for COPPA compliance by stripping user and device
/// identifiers before the request is forwarded to bidders.
///
/// Fields removed when `regs.coppa == 1`:
/// - `user.id`
/// - `user.buyeruid`
/// - `device.ifa`
/// - `device.dpidmd5`
/// - `device.dpidsha1`
pub fn sanitize_request_for_coppa(req: &mut openrtb::BidRequest) {
    if req.regs.as_ref().and_then(|r| r.coppa) != Some(1) {
        return;
    }

    if let Some(user) = &mut req.user {
        user.id = None;
        user.buyeruid = None;
    }

    if let Some(device) = &mut req.device {
        device.ifa = None;
        device.dpidmd5 = None;
        device.dpidsha1 = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(
        gdpr: bool,
        consent: Option<&str>,
        us_privacy: Option<&str>,
        lmt: Option<i32>,
        coppa: Option<i32>,
    ) -> AuctionPrivacyConfig {
        AuctionPrivacyConfig {
            gdpr_applies: gdpr,
            consent_string: consent.map(|s| s.to_string()),
            us_privacy: us_privacy.map(|s| s.to_string()),
            lmt,
            coppa,
        }
    }

    #[test]
    fn test_allow_no_restrictions() {
        let cfg = make_config(false, None, None, None, None);
        assert_eq!(check_privacy_for_bidder(&cfg, "appnexus"), PrivacyResult::Allow);
    }

    #[test]
    fn test_coppa_blocks() {
        let cfg = make_config(false, None, None, None, Some(1));
        assert_eq!(check_privacy_for_bidder(&cfg, "appnexus"), PrivacyResult::BlockCoppa);
    }

    #[test]
    fn test_lmt_blocks() {
        let cfg = make_config(false, None, None, Some(1), None);
        assert_eq!(check_privacy_for_bidder(&cfg, "appnexus"), PrivacyResult::BlockLmt);
    }

    #[test]
    fn test_ccpa_opt_out_blocks() {
        let cfg = make_config(false, None, Some("1YYN"), None, None);
        assert_eq!(check_privacy_for_bidder(&cfg, "appnexus"), PrivacyResult::BlockCcpa);
    }

    #[test]
    fn test_ccpa_no_opt_out_allows() {
        let cfg = make_config(false, None, Some("1YNN"), None, None);
        assert_eq!(check_privacy_for_bidder(&cfg, "appnexus"), PrivacyResult::Allow);
    }

    #[test]
    fn test_gdpr_no_consent_blocks() {
        let cfg = make_config(true, None, None, None, None);
        assert_eq!(check_privacy_for_bidder(&cfg, "appnexus"), PrivacyResult::BlockGdpr);
    }

    #[test]
    fn test_gdpr_with_consent_allows() {
        let cfg = make_config(true, Some("BOEFEAyOEFEAyAHABDENAI4AAAB9vABAASA"), None, None, None);
        assert_eq!(check_privacy_for_bidder(&cfg, "appnexus"), PrivacyResult::Allow);
    }

    #[test]
    fn test_coppa_takes_priority_over_lmt() {
        let cfg = make_config(false, None, None, Some(1), Some(1));
        assert_eq!(check_privacy_for_bidder(&cfg, "appnexus"), PrivacyResult::BlockCoppa);
    }

    #[test]
    fn test_extract_privacy_config_gdpr() {
        let mut req = openrtb::BidRequest::default();
        req.regs = Some(openrtb::Regs {
            coppa: None,
            us_privacy: None,
            dsa: None,
            ext: Some(serde_json::json!({"gdpr": 1})),
        });
        req.user = Some(openrtb::User {
            ext: Some(serde_json::json!({"consent": "someconsentstring"})),
            ..Default::default()
        });
        let cfg = extract_privacy_config(&req);
        assert!(cfg.gdpr_applies);
        assert_eq!(cfg.consent_string.as_deref(), Some("someconsentstring"));
    }

    #[test]
    fn test_sanitize_coppa_strips_identifiers() {
        let mut req = openrtb::BidRequest::default();
        req.regs = Some(openrtb::Regs {
            coppa: Some(1),
            us_privacy: None,
            dsa: None,
            ext: None,
        });
        req.user = Some(openrtb::User {
            id: Some("user-id-123".to_string()),
            buyeruid: Some("buyer-uid-456".to_string()),
            ..Default::default()
        });
        req.device = Some(openrtb::Device {
            ifa: Some("ifa-abc".to_string()),
            dpidmd5: Some("md5hash".to_string()),
            dpidsha1: Some("sha1hash".to_string()),
            ..Default::default()
        });
        sanitize_request_for_coppa(&mut req);
        let user = req.user.as_ref().unwrap();
        assert!(user.id.is_none());
        assert!(user.buyeruid.is_none());
        let device = req.device.as_ref().unwrap();
        assert!(device.ifa.is_none());
        assert!(device.dpidmd5.is_none());
        assert!(device.dpidsha1.is_none());
    }

    #[test]
    fn test_sanitize_no_coppa_leaves_data() {
        let mut req = openrtb::BidRequest::default();
        req.user = Some(openrtb::User {
            id: Some("user-id-123".to_string()),
            ..Default::default()
        });
        sanitize_request_for_coppa(&mut req);
        assert_eq!(req.user.as_ref().unwrap().id.as_deref(), Some("user-id-123"));
    }
}
