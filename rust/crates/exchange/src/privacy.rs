/// Privacy enforcement module covering GDPR, CCPA, COPPA, LMT, and GPP.
///
/// Provides a unified `AuctionPrivacyConfig` extracted from a `BidRequest` and
/// a single `check_privacy_for_bidder` function that returns a typed `PrivacyResult`.

// GPP Section ID constants (from IAB Global Privacy Platform spec).
/// TCF EU v2 section ID.
pub const GPP_SID_TCF_EU2: i8 = 2;
/// US Privacy (CCPA) v1 section ID.
pub const GPP_SID_USP_V1: i8 = 6;

/// Parsed GPP (Global Privacy Platform) policy from `regs.gpp` and `regs.gpp_sid`.
///
/// GPP is a newer privacy framework that replaces/supplements GDPR and CCPA signals.
/// When GPP is present, legacy signals can be derived from the GPP consent string
/// based on which section IDs are listed in `gpp_sid`.
#[derive(Debug, Clone, Default)]
pub struct GppPolicy {
    /// The raw GPP consent string from `regs.gpp`.
    pub consent_string: Option<String>,
    /// Section IDs from `regs.gpp_sid` indicating which regulations apply.
    pub section_ids: Vec<i8>,
}

impl GppPolicy {
    /// Whether the GPP policy has any content.
    pub fn is_empty(&self) -> bool {
        self.consent_string.is_none() && self.section_ids.is_empty()
    }

    /// Whether the given section ID is present in `gpp_sid`.
    pub fn has_section(&self, sid: i8) -> bool {
        self.section_ids.contains(&sid)
    }

    /// Whether TCF EU v2 (GDPR) section is present in `gpp_sid`.
    pub fn has_tcf_eu2(&self) -> bool {
        self.has_section(GPP_SID_TCF_EU2)
    }

    /// Whether US Privacy (CCPA) section is present in `gpp_sid`.
    pub fn has_usp_v1(&self) -> bool {
        self.has_section(GPP_SID_USP_V1)
    }
}

/// Parse GPP fields from a `BidRequest`.
///
/// Extracts `regs.gpp` (consent string) and `regs.gpp_sid` (section IDs).
pub fn parse_gpp(req: &openrtb::BidRequest) -> GppPolicy {
    let consent_string = req
        .regs
        .as_ref()
        .and_then(|r| r.gpp.clone())
        .filter(|s| !s.is_empty());

    let section_ids = req
        .regs
        .as_ref()
        .and_then(|r| r.gpp_sid.clone())
        .unwrap_or_default();

    GppPolicy {
        consent_string,
        section_ids,
    }
}

/// Derive legacy GDPR signals from GPP when legacy fields are absent.
///
/// Mirrors Go's `setLegacyGDPRFromGPP()`:
/// - If `regs.ext.gdpr` is not set and `gpp_sid` is present, set `gdpr_applies`
///   to true only if SID 2 (TCF EU v2) is in the list.
/// - If `user.consent` is empty, use the GPP consent string when TCF EU v2 is active.
fn derive_gdpr_from_gpp(
    gdpr_applies: bool,
    consent_string: &Option<String>,
    gpp: &GppPolicy,
    has_legacy_gdpr: bool,
) -> (bool, Option<String>) {
    let mut gdpr = gdpr_applies;
    let mut consent = consent_string.clone();

    // Only override gdpr_applies when the legacy regs.ext.gdpr field was absent.
    if !has_legacy_gdpr && !gpp.section_ids.is_empty() {
        gdpr = gpp.has_tcf_eu2();
    }

    // If no legacy consent string, derive from GPP when TCF EU v2 is active.
    if consent.is_none() && gpp.has_tcf_eu2() {
        if let Some(ref gpp_consent) = gpp.consent_string {
            consent = Some(gpp_consent.clone());
        }
    }

    (gdpr, consent)
}

/// Derive legacy US Privacy string from GPP when legacy field is absent.
///
/// Mirrors Go's `setLegacyUSPFromGPP()`:
/// - If `regs.us_privacy` is empty and `gpp_sid` contains 6 (USP v1), use the
///   GPP consent string as the US Privacy value.
fn derive_usp_from_gpp(us_privacy: &Option<String>, gpp: &GppPolicy) -> Option<String> {
    if us_privacy.is_some() {
        return us_privacy.clone();
    }
    if gpp.section_ids.is_empty() {
        return None;
    }
    if gpp.has_usp_v1() {
        return gpp.consent_string.clone();
    }
    None
}

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
    /// GPP policy parsed from the request.
    pub gpp: GppPolicy,
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
    // Check whether the legacy regs.ext.gdpr field is explicitly present.
    let legacy_gdpr_value = req
        .regs
        .as_ref()
        .and_then(|r| r.ext.as_ref())
        .and_then(|e| e.get("gdpr"))
        .and_then(|v| v.as_i64());

    let has_legacy_gdpr = legacy_gdpr_value.is_some();
    let gdpr_applies = legacy_gdpr_value.map(|g| g == 1).unwrap_or(false);

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

    // Parse GPP and derive legacy signals from it when legacy fields are absent.
    let gpp = parse_gpp(req);
    let (gdpr_applies, consent_string) =
        derive_gdpr_from_gpp(gdpr_applies, &consent_string, &gpp, has_legacy_gdpr);
    let us_privacy = derive_usp_from_gpp(&us_privacy, &gpp);

    AuctionPrivacyConfig {
        gdpr_applies,
        consent_string,
        us_privacy,
        lmt,
        coppa,
        gpp,
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

    /// Validate a CCPA consent string.
    /// Returns true if empty or valid per IAB spec (4 chars, version '1').
    pub fn validate_consent(consent: &str) -> bool {
        if consent.is_empty() {
            return true;
        }
        let chars: Vec<char> = consent.chars().collect();
        if chars.len() != 4 {
            return false;
        }
        chars[0] == '1'
    }
}

/// Parsed CCPA policy for enforcement decisions.
///
/// Determines whether a specific bidder should be blocked from
/// receiving the bid request based on opt-out and no-sale lists.
///
/// Mirrors Go `ccpa.ParsedPolicy`.
#[derive(Debug, Clone, Default)]
pub struct ParsedCcpaPolicy {
    /// Whether consent was explicitly provided.
    pub consent_specified: bool,
    /// Whether the user opted out of sale.
    pub consent_opt_out_sale: bool,
    /// Whether all bidders are in the no-sale list.
    pub no_sale_for_all_bidders: bool,
    /// Specific bidders in the no-sale list.
    pub no_sale_specific_bidders: std::collections::HashSet<String>,
}

impl ParsedCcpaPolicy {
    /// Parse a CCPA policy with no-sale bidders.
    ///
    /// `no_sale_bidders` is from `req.ext.prebid.nosale`.
    pub fn parse(consent: &str, no_sale_bidders: &[String]) -> Result<Self, String> {
        let (consent_specified, consent_opt_out_sale) = if consent.is_empty() {
            (false, false)
        } else {
            if !CcpaPolicy::validate_consent(consent) {
                return Err(format!(
                    "request.regs.ext.us_privacy must contain 4 characters"
                ));
            }
            let chars: Vec<char> = consent.chars().collect();
            (true, chars.len() >= 3 && chars[2] == 'Y')
        };

        let mut no_sale_for_all = false;
        let mut no_sale_specific = std::collections::HashSet::new();
        for bidder in no_sale_bidders {
            if bidder == "*" {
                no_sale_for_all = true;
            } else {
                no_sale_specific.insert(bidder.clone());
            }
        }

        Ok(Self {
            consent_specified,
            consent_opt_out_sale,
            no_sale_for_all_bidders: no_sale_for_all,
            no_sale_specific_bidders: no_sale_specific,
        })
    }

    /// Returns true when consent is explicitly provided.
    pub fn can_enforce(&self) -> bool {
        self.consent_specified
    }

    /// Returns true when the bid should be blocked for this bidder.
    pub fn should_enforce(&self, bidder: &str) -> bool {
        if !self.consent_opt_out_sale {
            return false;
        }
        // If bidder is in no-sale list, enforcement is NOT needed
        // (publisher has already ensured no sale for this bidder).
        !self.is_no_sale_for_bidder(bidder)
    }

    fn is_no_sale_for_bidder(&self, bidder: &str) -> bool {
        self.no_sale_for_all_bidders || self.no_sale_specific_bidders.contains(bidder)
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

// ---------------------------------------------------------------------------
// Privacy Scrubber — mirrors Go privacy/scrubber.go
// ---------------------------------------------------------------------------
// Provides functions to strip PII from BidRequest based on privacy regulations.

/// IP masking configuration.
#[derive(Debug, Clone)]
pub struct IpConf {
    pub ipv4_anon_keep_bits: u8,
    pub ipv6_anon_keep_bits: u8,
}

impl Default for IpConf {
    fn default() -> Self {
        IpConf {
            ipv4_anon_keep_bits: 24,
            ipv6_anon_keep_bits: 56,
        }
    }
}

/// Scrub device hardware identifiers.
pub fn scrub_device_ids(req: &mut openrtb::BidRequest) {
    if let Some(device) = &mut req.device {
        device.didmd5 = None;
        device.didsha1 = None;
        device.dpidmd5 = None;
        device.dpidsha1 = None;
        device.ifa = None;
        device.macmd5 = None;
        device.macsha1 = None;
    }
}

/// Scrub user identifiers and demographic data.
pub fn scrub_user_ids(req: &mut openrtb::BidRequest) {
    if let Some(user) = &mut req.user {
        user.data = Vec::new();
        user.id = None;
        user.buyeruid = None;
        user.yob = None;
        user.gender = None;
        user.keywords = None;
    }
}

/// Scrub user demographics (ID, buyeruid, yob, gender).
pub fn scrub_user_demographics(req: &mut openrtb::BidRequest) {
    if let Some(user) = &mut req.user {
        user.buyeruid = None;
        user.id = None;
        user.yob = None;
        user.gender = None;
    }
}

/// Scrub EIDs (Extended IDs) from user.eids and user.ext.eids.
pub fn scrub_eids(req: &mut openrtb::BidRequest) {
    if let Some(user) = &mut req.user {
        user.eids = None;
        scrub_ext_field(&mut user.ext, "eids");
    }
}

/// Scrub TID (Transaction ID) from source.tid and imp[].ext.tid.
pub fn scrub_tid(req: &mut openrtb::BidRequest) {
    if let Some(source) = &mut req.source {
        source.tid = None;
    }
    for imp in &mut req.imp {
        scrub_ext_field(&mut imp.ext, "tid");
    }
}

/// Reduce geographic precision by rounding lat/lon to 2 decimal places.
pub fn scrub_geo_precision(req: &mut openrtb::BidRequest) {
    if let Some(user) = &mut req.user {
        if let Some(geo) = &mut user.geo {
            round_geo_precision(geo);
        }
    }
    if let Some(device) = &mut req.device {
        if let Some(geo) = &mut device.geo {
            round_geo_precision(geo);
        }
    }
}

/// Remove all geographic data.
pub fn scrub_geo_full(req: &mut openrtb::BidRequest) {
    if let Some(user) = &mut req.user {
        if user.geo.is_some() {
            user.geo = Some(openrtb::Geo::default());
        }
    }
    if let Some(device) = &mut req.device {
        if device.geo.is_some() {
            device.geo = Some(openrtb::Geo::default());
        }
    }
}

/// Mask device IP addresses for anonymization.
pub fn scrub_device_ip(req: &mut openrtb::BidRequest, ip_conf: &IpConf) {
    if let Some(device) = &mut req.device {
        if let Some(ref ip) = device.ip {
            device.ip = Some(scrub_ip(ip, ip_conf.ipv4_anon_keep_bits, 32));
        }
        if let Some(ref ipv6) = device.ipv6 {
            device.ipv6 = Some(scrub_ip(ipv6, ip_conf.ipv6_anon_keep_bits, 128));
        }
    }
}

/// Full privacy scrub: device IDs, IPs, user demographics, ext field, and geo.
/// Mirrors Go `ScrubDeviceIDsIPsUserDemoExt`.
pub fn scrub_device_ids_ips_user_demo_ext(
    req: &mut openrtb::BidRequest,
    ip_conf: &IpConf,
    ext_field_name: &str,
    scrub_full_geo: bool,
) {
    scrub_device_ids(req);
    scrub_device_ip(req, ip_conf);
    scrub_user_demographics(req);
    if let Some(user) = &mut req.user {
        scrub_ext_field(&mut user.ext, ext_field_name);
    }
    if scrub_full_geo {
        scrub_geo_full(req);
    } else {
        scrub_geo_precision(req);
    }
}

/// Scrub user FPD (First Party Data). Mirrors Go `ScrubUserFPD`.
pub fn scrub_user_fpd(req: &mut openrtb::BidRequest) {
    scrub_device_ids(req);
    scrub_user_ids(req);
    if let Some(user) = &mut req.user {
        scrub_ext_field(&mut user.ext, "data");
        user.eids = None;
    }
}

/// Scrub GDPR-related identifiers. Mirrors Go `ScrubGdprID`.
pub fn scrub_gdpr_id(req: &mut openrtb::BidRequest) {
    scrub_device_ids(req);
    scrub_user_demographics(req);
    if let Some(user) = &mut req.user {
        scrub_ext_field(&mut user.ext, "eids");
    }
}

/// Scrub geo and device IP. Mirrors Go `ScrubGeoAndDeviceIP`.
pub fn scrub_geo_and_device_ip(req: &mut openrtb::BidRequest, ip_conf: &IpConf) {
    scrub_device_ip(req, ip_conf);
    scrub_geo_precision(req);
}

// -- internal helpers --

fn scrub_ip(ip: &str, keep_bits: u8, total_bits: u8) -> String {
    if ip.is_empty() {
        return String::new();
    }
    if total_bits == 32 {
        pbs_util::iputil::mask_ipv4(ip, keep_bits).unwrap_or_default()
    } else {
        pbs_util::iputil::mask_ipv6(ip, keep_bits).unwrap_or_default()
    }
}

fn round_geo_precision(geo: &mut openrtb::Geo) {
    if let Some(lat) = geo.lat {
        geo.lat = Some((lat * 100.0 + 0.5).floor() / 100.0);
    }
    if let Some(lon) = geo.lon {
        geo.lon = Some((lon * 100.0 + 0.5).floor() / 100.0);
    }
}

fn scrub_ext_field(ext: &mut Option<serde_json::Value>, field: &str) {
    if let Some(serde_json::Value::Object(map)) = ext {
        map.remove(field);
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
            gpp: GppPolicy::default(),
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
            gdpr: None,
            us_privacy: None,
            gpp: None,
            gpp_sid: None,
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
            gdpr: None,
            us_privacy: None,
            gpp: None,
            gpp_sid: None,
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

    // -----------------------------------------------------------------------
    // GPP tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_gpp_empty_request() {
        let req = openrtb::BidRequest::default();
        let gpp = parse_gpp(&req);
        assert!(gpp.is_empty());
        assert!(gpp.consent_string.is_none());
        assert!(gpp.section_ids.is_empty());
    }

    #[test]
    fn test_parse_gpp_with_consent_and_sids() {
        let mut req = openrtb::BidRequest::default();
        req.regs = Some(openrtb::Regs {
            gpp: Some("DBACNYA~CPXxRfAPXxRfAAfKABENB-CgAAAAAAAAAAYgAAAAAAAA~1YNN".to_string()),
            gpp_sid: Some(vec![2, 6]),
            ..Default::default()
        });
        let gpp = parse_gpp(&req);
        assert!(!gpp.is_empty());
        assert!(gpp.has_tcf_eu2());
        assert!(gpp.has_usp_v1());
        assert!(!gpp.has_section(7));
    }

    #[test]
    fn test_parse_gpp_empty_string_treated_as_none() {
        let mut req = openrtb::BidRequest::default();
        req.regs = Some(openrtb::Regs {
            gpp: Some("".to_string()),
            gpp_sid: Some(vec![2]),
            ..Default::default()
        });
        let gpp = parse_gpp(&req);
        assert!(gpp.consent_string.is_none());
        assert!(gpp.has_tcf_eu2());
    }

    #[test]
    fn test_gpp_derives_gdpr_when_legacy_absent() {
        // No legacy regs.ext.gdpr, but GPP SID contains TCF EU v2 (2).
        let mut req = openrtb::BidRequest::default();
        req.regs = Some(openrtb::Regs {
            gpp: Some("GPP_CONSENT_TCF".to_string()),
            gpp_sid: Some(vec![GPP_SID_TCF_EU2]),
            ..Default::default()
        });
        let cfg = extract_privacy_config(&req);
        assert!(cfg.gdpr_applies);
        // Consent string derived from GPP.
        assert_eq!(cfg.consent_string.as_deref(), Some("GPP_CONSENT_TCF"));
    }

    #[test]
    fn test_gpp_does_not_override_legacy_gdpr() {
        // Legacy regs.ext.gdpr=0 should NOT be overridden by GPP SID.
        let mut req = openrtb::BidRequest::default();
        req.regs = Some(openrtb::Regs {
            gpp: Some("GPP_CONSENT".to_string()),
            gpp_sid: Some(vec![GPP_SID_TCF_EU2]),
            ext: Some(serde_json::json!({"gdpr": 0})),
            ..Default::default()
        });
        let cfg = extract_privacy_config(&req);
        assert!(!cfg.gdpr_applies);
    }

    #[test]
    fn test_gpp_does_not_override_legacy_consent() {
        // Legacy consent string present should NOT be replaced by GPP.
        let mut req = openrtb::BidRequest::default();
        req.regs = Some(openrtb::Regs {
            gpp: Some("GPP_CONSENT".to_string()),
            gpp_sid: Some(vec![GPP_SID_TCF_EU2]),
            ..Default::default()
        });
        req.user = Some(openrtb::User {
            ext: Some(serde_json::json!({"consent": "LEGACY_CONSENT"})),
            ..Default::default()
        });
        let cfg = extract_privacy_config(&req);
        assert_eq!(cfg.consent_string.as_deref(), Some("LEGACY_CONSENT"));
    }

    #[test]
    fn test_gpp_gdpr_not_applied_without_tcf_sid() {
        // GPP SID present but does NOT contain TCF EU v2 -> gdpr_applies=false.
        let mut req = openrtb::BidRequest::default();
        req.regs = Some(openrtb::Regs {
            gpp: Some("GPP_CONSENT".to_string()),
            gpp_sid: Some(vec![GPP_SID_USP_V1]),
            ..Default::default()
        });
        let cfg = extract_privacy_config(&req);
        assert!(!cfg.gdpr_applies);
        assert!(cfg.consent_string.is_none());
    }

    #[test]
    fn test_gpp_derives_usp_when_legacy_absent() {
        // No legacy us_privacy, but GPP SID contains USP v1 (6).
        let mut req = openrtb::BidRequest::default();
        req.regs = Some(openrtb::Regs {
            gpp: Some("1YYN".to_string()),
            gpp_sid: Some(vec![GPP_SID_USP_V1]),
            ..Default::default()
        });
        let cfg = extract_privacy_config(&req);
        assert_eq!(cfg.us_privacy.as_deref(), Some("1YYN"));
    }

    #[test]
    fn test_gpp_does_not_override_legacy_usp() {
        // Legacy us_privacy present should NOT be replaced by GPP.
        let mut req = openrtb::BidRequest::default();
        req.regs = Some(openrtb::Regs {
            us_privacy: Some("1NNN".to_string()),
            gpp: Some("1YYN".to_string()),
            gpp_sid: Some(vec![GPP_SID_USP_V1]),
            ..Default::default()
        });
        let cfg = extract_privacy_config(&req);
        assert_eq!(cfg.us_privacy.as_deref(), Some("1NNN"));
    }

    #[test]
    fn test_gpp_usp_not_applied_without_usp_sid() {
        // GPP SID present but does NOT contain USP v1 -> us_privacy stays None.
        let mut req = openrtb::BidRequest::default();
        req.regs = Some(openrtb::Regs {
            gpp: Some("1YYN".to_string()),
            gpp_sid: Some(vec![GPP_SID_TCF_EU2]),
            ..Default::default()
        });
        let cfg = extract_privacy_config(&req);
        assert!(cfg.us_privacy.is_none());
    }

    #[test]
    fn test_gpp_no_derive_when_sid_empty() {
        // GPP consent present but gpp_sid is empty -> no legacy derivation.
        let mut req = openrtb::BidRequest::default();
        req.regs = Some(openrtb::Regs {
            gpp: Some("GPP_CONSENT".to_string()),
            gpp_sid: None,
            ..Default::default()
        });
        let cfg = extract_privacy_config(&req);
        assert!(!cfg.gdpr_applies);
        assert!(cfg.consent_string.is_none());
        assert!(cfg.us_privacy.is_none());
    }

    #[test]
    fn test_gpp_both_tcf_and_usp_sids() {
        // GPP SID contains both TCF EU v2 and USP v1.
        let mut req = openrtb::BidRequest::default();
        req.regs = Some(openrtb::Regs {
            gpp: Some("GPP_BOTH".to_string()),
            gpp_sid: Some(vec![GPP_SID_TCF_EU2, GPP_SID_USP_V1]),
            ..Default::default()
        });
        let cfg = extract_privacy_config(&req);
        assert!(cfg.gdpr_applies);
        assert_eq!(cfg.consent_string.as_deref(), Some("GPP_BOTH"));
        assert_eq!(cfg.us_privacy.as_deref(), Some("GPP_BOTH"));
    }

    #[test]
    fn test_gpp_policy_struct() {
        let policy = GppPolicy {
            consent_string: Some("test".to_string()),
            section_ids: vec![2, 6],
        };
        assert!(!policy.is_empty());
        assert!(policy.has_tcf_eu2());
        assert!(policy.has_usp_v1());
        assert!(!policy.has_section(3));

        let empty = GppPolicy::default();
        assert!(empty.is_empty());
        assert!(!empty.has_tcf_eu2());
    }

    #[test]
    fn test_gpp_stored_in_config() {
        let mut req = openrtb::BidRequest::default();
        req.regs = Some(openrtb::Regs {
            gpp: Some("GPP_STRING".to_string()),
            gpp_sid: Some(vec![2, 6]),
            ..Default::default()
        });
        let cfg = extract_privacy_config(&req);
        assert_eq!(cfg.gpp.consent_string.as_deref(), Some("GPP_STRING"));
        assert_eq!(cfg.gpp.section_ids, vec![2, 6]);
    }
}
