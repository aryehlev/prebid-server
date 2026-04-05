/// GDPR/TCF consent string parsing, validation, and enforcement.
///
/// Implements basic TCF v1 and v2 consent string parsing without external crates.
/// Provides types for GDPR signal, policy configuration, enforcement, and
/// a permissions trait for vendor/purpose consent checking.

use base64::Engine as _;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// GdprSignal
// ---------------------------------------------------------------------------

/// Represents the GDPR applicability signal found in `regs.ext.gdpr`.
///
/// * `Ambiguous` (default) -- the publisher did not provide a signal.
/// * `No` -- GDPR does **not** apply to this request.
/// * `Yes` -- GDPR **does** apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GdprSignal {
    #[default]
    Ambiguous,
    No,
    Yes,
}

impl GdprSignal {
    /// Parse from the integer value found in `regs.ext.gdpr`.
    pub fn from_i64(value: i64) -> Self {
        match value {
            0 => Self::No,
            1 => Self::Yes,
            _ => Self::Ambiguous,
        }
    }

    /// Convert to an optional integer (for writing back into requests).
    pub fn as_opt_i8(&self) -> Option<i8> {
        match self {
            Self::Ambiguous => None,
            Self::No => Some(0),
            Self::Yes => Some(1),
        }
    }
}

// ---------------------------------------------------------------------------
// TCF consent string parsing
// ---------------------------------------------------------------------------

/// Decoded fields from a TCF consent string.
#[derive(Debug, Clone)]
pub struct TcfConsent {
    /// TCF version (1 or 2).
    pub version: u8,
    /// Bitmask of consented purpose IDs (bits 1..=24 for TCF v2).
    /// Bit N is set if purpose N has consent.
    pub purpose_consents: u64,
    /// Set of vendor IDs that have consent (from the consent section).
    pub vendor_consents: HashSet<u32>,
    /// Raw decoded bytes (kept for advanced use).
    #[allow(dead_code)]
    raw: Vec<u8>,
}

impl TcfConsent {
    /// Attempt to parse a base64url-encoded TCF consent string.
    pub fn parse(consent_string: &str) -> Option<Self> {
        if consent_string.is_empty() {
            return None;
        }

        let decoded = decode_base64url(consent_string)?;
        if decoded.len() < 8 {
            return None;
        }

        // Bits 0..5: version (6 bits)
        let version = (decoded[0] >> 2) & 0x3F;

        match version {
            1 => Self::parse_v1(&decoded),
            2 => Self::parse_v2(&decoded),
            _ => None,
        }
    }

    /// Check if a specific purpose (1-based) has consent.
    pub fn has_purpose_consent(&self, purpose_id: u32) -> bool {
        if purpose_id == 0 || purpose_id > 64 {
            return false;
        }
        (self.purpose_consents >> (purpose_id - 1)) & 1 == 1
    }

    /// Check if a specific vendor has consent.
    pub fn has_vendor_consent(&self, vendor_id: u32) -> bool {
        self.vendor_consents.contains(&vendor_id)
    }

    // -- TCF v1 parsing (simplified) --

    fn parse_v1(decoded: &[u8]) -> Option<Self> {
        // TCF v1: purpose consents start at bit 152, 24 bits
        let purpose_consents = extract_purpose_consents_v1(decoded)?;
        let vendor_consents = extract_vendor_consents_v1(decoded);

        Some(TcfConsent {
            version: 1,
            purpose_consents,
            vendor_consents,
            raw: decoded.to_vec(),
        })
    }

    // -- TCF v2 parsing --

    fn parse_v2(decoded: &[u8]) -> Option<Self> {
        // TCF v2: purpose consents start at bit 152, 24 bits
        let purpose_consents = extract_purpose_consents_v2(decoded)?;
        let vendor_consents = extract_vendor_consents_v2(decoded);

        Some(TcfConsent {
            version: 2,
            purpose_consents,
            vendor_consents,
            raw: decoded.to_vec(),
        })
    }
}

/// Decode a base64url string (with or without padding).
fn decode_base64url(input: &str) -> Option<Vec<u8>> {
    // Normalise URL-safe characters to standard base64.
    let normalized: String = input
        .chars()
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            other => other,
        })
        .collect();

    let padded = match normalized.len() % 4 {
        2 => format!("{}==", normalized),
        3 => format!("{}=", normalized),
        _ => normalized,
    };

    base64::engine::general_purpose::STANDARD
        .decode(&padded)
        .ok()
}

/// Read a single bit from a byte slice at the given bit offset.
fn read_bit(data: &[u8], bit_offset: usize) -> Option<bool> {
    let byte_index = bit_offset / 8;
    let bit_index = 7 - (bit_offset % 8);
    if byte_index >= data.len() {
        return None;
    }
    Some((data[byte_index] >> bit_index) & 1 == 1)
}

/// Read `count` bits starting at `bit_offset` and return as u64 (big-endian).
fn read_bits(data: &[u8], bit_offset: usize, count: usize) -> Option<u64> {
    if count > 64 {
        return None;
    }
    let mut value: u64 = 0;
    for i in 0..count {
        let bit = read_bit(data, bit_offset + i)?;
        value = (value << 1) | (bit as u64);
    }
    Some(value)
}

// -- TCF v1 helpers --

fn extract_purpose_consents_v1(decoded: &[u8]) -> Option<u64> {
    // TCF v1: purposes allowed start at bit 152, 24 bits
    let mut consents: u64 = 0;
    for i in 0..24 {
        if read_bit(decoded, 152 + i)? {
            consents |= 1u64 << i;
        }
    }
    Some(consents)
}

fn extract_vendor_consents_v1(decoded: &[u8]) -> HashSet<u32> {
    // TCF v1: max vendor ID at bit 176 (16 bits), then encoding type at bit 192
    let mut vendors = HashSet::new();
    let max_vendor_id = match read_bits(decoded, 176, 16) {
        Some(v) => v as u32,
        None => return vendors,
    };
    let is_range_encoding = read_bit(decoded, 192).unwrap_or(false);

    if !is_range_encoding {
        // Bitfield encoding: one bit per vendor starting at bit 193
        for vid in 1..=max_vendor_id {
            if read_bit(decoded, 193 + (vid as usize - 1)).unwrap_or(false) {
                vendors.insert(vid);
            }
        }
    }
    // Range encoding parsing is complex; skip for simplified implementation.
    vendors
}

// -- TCF v2 helpers --

fn extract_purpose_consents_v2(decoded: &[u8]) -> Option<u64> {
    // TCF v2: purpose consents start at bit 152, 24 bits
    let mut consents: u64 = 0;
    for i in 0..24 {
        if read_bit(decoded, 152 + i)? {
            consents |= 1u64 << i;
        }
    }
    Some(consents)
}

fn extract_vendor_consents_v2(decoded: &[u8]) -> HashSet<u32> {
    // TCF v2: after purpose fields at bit 260, vendor consent section begins
    // Max vendor ID at bit 230 (16 bits) for the consent section
    // (Purpose LI transparency: 24 bits at 176+24=200..224; special feature at 224..236)
    // Vendor consent section: max vendor ID (16 bits) at bit 260, encoding type bit 276
    let mut vendors = HashSet::new();
    let max_vendor_id = match read_bits(decoded, 260, 16) {
        Some(v) => v as u32,
        None => return vendors,
    };
    let is_range_encoding = read_bit(decoded, 276).unwrap_or(false);

    if !is_range_encoding {
        // Bitfield encoding
        for vid in 1..=max_vendor_id.min(2048) {
            if read_bit(decoded, 277 + (vid as usize - 1)).unwrap_or(false) {
                vendors.insert(vid);
            }
        }
    }
    // Range encoding: skip for simplified implementation
    vendors
}

// ---------------------------------------------------------------------------
// GdprPolicy -- configuration for GDPR enforcement
// ---------------------------------------------------------------------------

/// TCF purpose IDs relevant to programmatic advertising.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum TcfPurpose {
    /// Store and/or access information on a device.
    DeviceAccess = 1,
    /// Select basic ads.
    BasicAds = 2,
    /// Create a personalised ads profile.
    PersonalisedAdsProfile = 3,
    /// Select personalised ads.
    PersonalisedAds = 4,
    /// Create a personalised content profile.
    PersonalisedContentProfile = 5,
    /// Select personalised content.
    PersonalisedContent = 6,
    /// Measure ad performance.
    MeasureAdPerformance = 7,
    /// Measure content performance.
    MeasureContentPerformance = 8,
    /// Apply market research to generate audience insights.
    MarketResearch = 9,
    /// Develop and improve products.
    DevelopProducts = 10,
}

/// Configuration that governs how GDPR/TCF is enforced.
#[derive(Debug, Clone)]
pub struct GdprPolicy {
    /// Whether GDPR enforcement is enabled at all.
    pub enabled: bool,
    /// Default GDPR signal when the publisher does not specify one.
    pub default_value: GdprSignal,
    /// Set of TCF purposes that must have consent for a bidder to proceed.
    pub enforce_purposes: HashSet<u32>,
    /// Whether full vendor consent (not just purpose consent) is required.
    pub require_vendor_consent: bool,
    /// Bidders that are exempt from GDPR enforcement (e.g. first-party).
    pub bidder_exceptions: HashSet<String>,
}

impl Default for GdprPolicy {
    fn default() -> Self {
        let mut enforce = HashSet::new();
        // By default enforce purpose 1 (device access) and 2 (basic ads).
        enforce.insert(1);
        enforce.insert(2);

        Self {
            enabled: true,
            default_value: GdprSignal::Ambiguous,
            enforce_purposes: enforce,
            require_vendor_consent: true,
            bidder_exceptions: HashSet::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// GdprEnforcer
// ---------------------------------------------------------------------------

/// Outcome of a GDPR enforcement check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GdprEnforcementResult {
    /// Whether the bidder is allowed to participate.
    pub allow: bool,
    /// Whether personal data (geo, device IDs) should be masked.
    pub mask_personal_data: bool,
    /// Specific purposes that lack consent.
    pub denied_purposes: Vec<u32>,
}

/// Enforces GDPR/TCF rules against a consent string and policy configuration.
#[derive(Debug)]
pub struct GdprEnforcer {
    pub policy: GdprPolicy,
}

impl GdprEnforcer {
    pub fn new(policy: GdprPolicy) -> Self {
        Self { policy }
    }

    /// Determine the effective GDPR signal for a request, falling back to the
    /// policy default when the publisher signal is ambiguous.
    pub fn effective_signal(&self, signal: GdprSignal) -> GdprSignal {
        match signal {
            GdprSignal::Ambiguous => self.policy.default_value,
            other => other,
        }
    }

    /// Check whether a bidder is permitted to receive the request under GDPR.
    pub fn check_bidder(
        &self,
        signal: GdprSignal,
        consent_string: Option<&str>,
        bidder_name: &str,
        vendor_id: Option<u32>,
    ) -> GdprEnforcementResult {
        // If enforcement is disabled, allow everything.
        if !self.policy.enabled {
            return GdprEnforcementResult {
                allow: true,
                mask_personal_data: false,
                denied_purposes: vec![],
            };
        }

        // If the bidder is exempt, allow.
        if self.policy.bidder_exceptions.contains(bidder_name) {
            return GdprEnforcementResult {
                allow: true,
                mask_personal_data: false,
                denied_purposes: vec![],
            };
        }

        let effective = self.effective_signal(signal);

        // If GDPR doesn't apply, allow.
        if effective != GdprSignal::Yes {
            return GdprEnforcementResult {
                allow: true,
                mask_personal_data: false,
                denied_purposes: vec![],
            };
        }

        // GDPR applies -- parse consent.
        let parsed = consent_string.and_then(|s| TcfConsent::parse(s));

        match parsed {
            None => {
                // No valid consent string while GDPR applies: block.
                GdprEnforcementResult {
                    allow: false,
                    mask_personal_data: true,
                    denied_purposes: self.policy.enforce_purposes.iter().copied().collect(),
                }
            }
            Some(tcf) => {
                let mut denied = Vec::new();

                for &purpose_id in &self.policy.enforce_purposes {
                    if !tcf.has_purpose_consent(purpose_id) {
                        denied.push(purpose_id);
                    }
                }

                // Check vendor consent if required.
                let vendor_ok = if self.policy.require_vendor_consent {
                    match vendor_id {
                        Some(vid) => tcf.has_vendor_consent(vid),
                        None => true, // No vendor ID registered -- skip vendor check.
                    }
                } else {
                    true
                };

                let allow = denied.is_empty() && vendor_ok;

                GdprEnforcementResult {
                    allow,
                    mask_personal_data: !allow,
                    denied_purposes: denied,
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// GdprPermissions trait
// ---------------------------------------------------------------------------

/// Trait for querying GDPR/TCF consent permissions.
///
/// Implementations may be backed by a full TCF vendor list, an in-memory
/// consent string parser, or an external service.
pub trait GdprPermissions: Send + Sync {
    /// Whether the host has a valid consent mechanism configured.
    fn can_enforce(&self) -> bool;

    /// Whether a specific purpose has consent.
    fn has_purpose_consent(&self, purpose_id: u32) -> bool;

    /// Whether a specific vendor has consent.
    fn has_vendor_consent(&self, vendor_id: u32) -> bool;

    /// Whether a bidder (by name) is allowed to receive personal data.
    fn bidder_allowed(&self, bidder_name: &str, vendor_id: Option<u32>) -> bool;
}

/// A concrete implementation backed by a parsed TCF consent string.
pub struct ConsentPermissions {
    pub signal: GdprSignal,
    pub consent: Option<TcfConsent>,
    pub enforcer: GdprEnforcer,
}

impl GdprPermissions for ConsentPermissions {
    fn can_enforce(&self) -> bool {
        self.enforcer.policy.enabled && self.enforcer.effective_signal(self.signal) == GdprSignal::Yes
    }

    fn has_purpose_consent(&self, purpose_id: u32) -> bool {
        match &self.consent {
            Some(tcf) => tcf.has_purpose_consent(purpose_id),
            None => false,
        }
    }

    fn has_vendor_consent(&self, vendor_id: u32) -> bool {
        match &self.consent {
            Some(tcf) => tcf.has_vendor_consent(vendor_id),
            None => false,
        }
    }

    fn bidder_allowed(&self, bidder_name: &str, vendor_id: Option<u32>) -> bool {
        let result = self
            .enforcer
            .check_bidder(self.signal, None, bidder_name, vendor_id);
        // Re-check with actual consent data.
        if !self.can_enforce() {
            return true;
        }
        if self.enforcer.policy.bidder_exceptions.contains(bidder_name) {
            return true;
        }
        match &self.consent {
            None => false,
            Some(tcf) => {
                let purposes_ok = self
                    .enforcer
                    .policy
                    .enforce_purposes
                    .iter()
                    .all(|&p| tcf.has_purpose_consent(p));
                let vendor_ok = if self.enforcer.policy.require_vendor_consent {
                    vendor_id.map_or(true, |vid| tcf.has_vendor_consent(vid))
                } else {
                    true
                };
                let _ = result; // suppress unused warning
                purposes_ok && vendor_ok
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Legacy public helpers (kept for backward compatibility with existing callers)
// ---------------------------------------------------------------------------

/// Parse purpose consents from a TCF consent string (base64url encoded).
/// Returns a bitmask of consented purpose IDs, or None if parsing fails.
pub fn parse_purpose_consents(consent_string: &str) -> Option<u64> {
    TcfConsent::parse(consent_string).map(|tcf| tcf.purpose_consents)
}

/// Check if a vendor has consent for a specific purpose.
pub fn vendor_has_consent(consent_string: &str, vendor_id: u32, purpose_id: u32) -> bool {
    match TcfConsent::parse(consent_string) {
        Some(tcf) => tcf.has_purpose_consent(purpose_id) && tcf.has_vendor_consent(vendor_id),
        None => false,
    }
}

/// Check if GDPR enforcement should block a bidder.
pub fn should_block_bidder_gdpr(
    gdpr_applies: bool,
    consent_string: Option<&str>,
    _bidder_name: &str,
) -> bool {
    if !gdpr_applies {
        return false;
    }
    match consent_string {
        None | Some("") => true,
        Some(s) => parse_purpose_consents(s).is_none(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gdpr_signal_from_i64() {
        assert_eq!(GdprSignal::from_i64(0), GdprSignal::No);
        assert_eq!(GdprSignal::from_i64(1), GdprSignal::Yes);
        assert_eq!(GdprSignal::from_i64(2), GdprSignal::Ambiguous);
        assert_eq!(GdprSignal::from_i64(-1), GdprSignal::Ambiguous);
    }

    #[test]
    fn test_gdpr_signal_default() {
        assert_eq!(GdprSignal::default(), GdprSignal::Ambiguous);
    }

    #[test]
    fn test_gdpr_signal_as_opt_i8() {
        assert_eq!(GdprSignal::Ambiguous.as_opt_i8(), None);
        assert_eq!(GdprSignal::No.as_opt_i8(), Some(0));
        assert_eq!(GdprSignal::Yes.as_opt_i8(), Some(1));
    }

    #[test]
    fn test_parse_empty_consent_string() {
        assert!(TcfConsent::parse("").is_none());
    }

    #[test]
    fn test_parse_short_consent_string() {
        assert!(TcfConsent::parse("AA").is_none());
    }

    #[test]
    fn test_parse_purpose_consents_empty() {
        assert!(parse_purpose_consents("").is_none());
    }

    #[test]
    fn test_parse_purpose_consents_valid() {
        // BOEFEAyOEFEAyAHABDENAI4AAAB9vABAASA is a known TCF v1 string
        let result = parse_purpose_consents("BOEFEAyOEFEAyAHABDENAI4AAAB9vABAASA");
        assert!(result.is_some());
    }

    #[test]
    fn test_read_bit() {
        let data = [0b10110000u8];
        assert_eq!(read_bit(&data, 0), Some(true));
        assert_eq!(read_bit(&data, 1), Some(false));
        assert_eq!(read_bit(&data, 2), Some(true));
        assert_eq!(read_bit(&data, 3), Some(true));
        assert_eq!(read_bit(&data, 4), Some(false));
    }

    #[test]
    fn test_read_bits() {
        let data = [0b11001010u8, 0b11110000u8];
        // First 4 bits: 1100 = 12
        assert_eq!(read_bits(&data, 0, 4), Some(12));
        // Bits 4..8: 1010 = 10
        assert_eq!(read_bits(&data, 4, 4), Some(10));
    }

    #[test]
    fn test_tcf_consent_has_purpose_consent_zero() {
        // Purpose 0 is out of range.
        let tcf = TcfConsent {
            version: 2,
            purpose_consents: 0b111,
            vendor_consents: HashSet::new(),
            raw: vec![],
        };
        assert!(!tcf.has_purpose_consent(0));
    }

    #[test]
    fn test_tcf_consent_has_purpose_consent_in_range() {
        let tcf = TcfConsent {
            version: 2,
            purpose_consents: 0b101, // purposes 1 and 3
            vendor_consents: HashSet::new(),
            raw: vec![],
        };
        assert!(tcf.has_purpose_consent(1));
        assert!(!tcf.has_purpose_consent(2));
        assert!(tcf.has_purpose_consent(3));
    }

    #[test]
    fn test_enforcer_disabled() {
        let policy = GdprPolicy {
            enabled: false,
            ..Default::default()
        };
        let enforcer = GdprEnforcer::new(policy);
        let result = enforcer.check_bidder(GdprSignal::Yes, None, "appnexus", None);
        assert!(result.allow);
        assert!(!result.mask_personal_data);
    }

    #[test]
    fn test_enforcer_gdpr_no() {
        let enforcer = GdprEnforcer::new(GdprPolicy::default());
        let result = enforcer.check_bidder(GdprSignal::No, None, "appnexus", None);
        assert!(result.allow);
    }

    #[test]
    fn test_enforcer_gdpr_yes_no_consent() {
        let enforcer = GdprEnforcer::new(GdprPolicy::default());
        let result = enforcer.check_bidder(GdprSignal::Yes, None, "appnexus", None);
        assert!(!result.allow);
        assert!(result.mask_personal_data);
    }

    #[test]
    fn test_enforcer_bidder_exception() {
        let mut policy = GdprPolicy::default();
        policy.bidder_exceptions.insert("trusted".to_string());
        let enforcer = GdprEnforcer::new(policy);
        let result = enforcer.check_bidder(GdprSignal::Yes, None, "trusted", None);
        assert!(result.allow);
    }

    #[test]
    fn test_enforcer_ambiguous_defaults_to_no() {
        let mut policy = GdprPolicy::default();
        policy.default_value = GdprSignal::No;
        let enforcer = GdprEnforcer::new(policy);
        let result = enforcer.check_bidder(GdprSignal::Ambiguous, None, "appnexus", None);
        assert!(result.allow);
    }

    #[test]
    fn test_enforcer_ambiguous_defaults_to_yes() {
        let mut policy = GdprPolicy::default();
        policy.default_value = GdprSignal::Yes;
        let enforcer = GdprEnforcer::new(policy);
        let result = enforcer.check_bidder(GdprSignal::Ambiguous, None, "appnexus", None);
        assert!(!result.allow);
    }

    #[test]
    fn test_should_block_bidder_gdpr_no_gdpr() {
        assert!(!should_block_bidder_gdpr(false, None, "appnexus"));
    }

    #[test]
    fn test_should_block_bidder_gdpr_no_consent() {
        assert!(should_block_bidder_gdpr(true, None, "appnexus"));
        assert!(should_block_bidder_gdpr(true, Some(""), "appnexus"));
    }

    #[test]
    fn test_should_block_bidder_gdpr_valid_consent() {
        assert!(!should_block_bidder_gdpr(
            true,
            Some("BOEFEAyOEFEAyAHABDENAI4AAAB9vABAASA"),
            "appnexus"
        ));
    }
}
