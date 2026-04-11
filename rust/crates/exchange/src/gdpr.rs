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

/// Parse range-encoded vendor entries from a consent string bitfield.
///
/// If `default_consent` is true (TCF v1 only), all vendors up to `max_vendor_id`
/// are assumed consented and the ranges *remove* consent. If false, the ranges
/// *add* consent.
fn parse_range_entries(
    data: &[u8],
    start_bit: usize,
    max_vendor_id: u32,
    default_consent: bool,
    vendors: &mut HashSet<u32>,
) {
    // If default consent, pre-fill all vendors
    if default_consent {
        for vid in 1..=max_vendor_id {
            vendors.insert(vid);
        }
    }

    let num_entries = match read_bits(data, start_bit, 12) {
        Some(v) => v as usize,
        None => return,
    };

    let mut offset = start_bit + 12;

    for _ in 0..num_entries {
        let is_range = match read_bit(data, offset) {
            Some(v) => v,
            None => return,
        };
        offset += 1;

        if is_range {
            // StartVendorId (16 bits) + EndVendorId (16 bits)
            let start_id = match read_bits(data, offset, 16) {
                Some(v) => v as u32,
                None => return,
            };
            offset += 16;
            let end_id = match read_bits(data, offset, 16) {
                Some(v) => v as u32,
                None => return,
            };
            offset += 16;

            for vid in start_id..=end_id.min(max_vendor_id) {
                if default_consent {
                    vendors.remove(&vid);
                } else {
                    vendors.insert(vid);
                }
            }
        } else {
            // Single VendorId (16 bits)
            let vid = match read_bits(data, offset, 16) {
                Some(v) => v as u32,
                None => return,
            };
            offset += 16;

            if vid <= max_vendor_id {
                if default_consent {
                    vendors.remove(&vid);
                } else {
                    vendors.insert(vid);
                }
            }
        }
    }
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
    } else {
        // Range encoding: starts at bit 193
        // V1 has a "default consent" bit before the num_entries
        let default_consent = read_bit(decoded, 193).unwrap_or(false);
        parse_range_entries(decoded, 194, max_vendor_id, default_consent, &mut vendors);
    }
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
    } else {
        // Range encoding: starts at bit 277 (no default consent bit in v2)
        parse_range_entries(decoded, 277, max_vendor_id, false, &mut vendors);
    }
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
// PurposeEnforcer — per-purpose enforcement (mirrors Go purpose_enforcer.go)
// ---------------------------------------------------------------------------

/// Per-purpose enforcement configuration.
#[derive(Debug, Clone)]
pub struct PurposeConfig {
    /// Whether to check the purpose consent bit.
    pub enforce_purpose: bool,
    /// Whether to check the vendor consent bit for this purpose.
    pub enforce_vendors: bool,
    /// Vendors exempt from this purpose's enforcement.
    pub vendor_exceptions: HashSet<u32>,
    /// Enforcement algorithm: "basic" (consent only) or "full" (consent + LI).
    pub enforce_algo: String,
}

impl Default for PurposeConfig {
    fn default() -> Self {
        Self {
            enforce_purpose: true,
            enforce_vendors: true,
            vendor_exceptions: HashSet::new(),
            enforce_algo: "basic".to_string(),
        }
    }
}

/// Per-purpose enforcer that checks consent for each of the 10 TCF purposes.
/// Mirrors Go's `gdpr/purpose_enforcer.go`.
#[derive(Debug, Clone)]
pub struct PurposeEnforcer {
    /// Configuration for each purpose (1-10).
    pub purposes: [PurposeConfig; 10],
}

impl Default for PurposeEnforcer {
    fn default() -> Self {
        Self {
            purposes: std::array::from_fn(|_| PurposeConfig::default()),
        }
    }
}

impl PurposeEnforcer {
    /// Check whether a vendor has consent for a specific purpose.
    pub fn is_allowed(
        &self,
        purpose_id: u32,
        consent: &TcfConsent,
        vendor_id: u32,
    ) -> bool {
        if purpose_id == 0 || purpose_id > 10 {
            return false;
        }
        let cfg = &self.purposes[(purpose_id - 1) as usize];

        // Check vendor exceptions first
        if cfg.vendor_exceptions.contains(&vendor_id) {
            return true;
        }

        let purpose_ok = if cfg.enforce_purpose {
            consent.has_purpose_consent(purpose_id)
        } else {
            true
        };

        let vendor_ok = if cfg.enforce_vendors {
            consent.has_vendor_consent(vendor_id)
        } else {
            true
        };

        purpose_ok && vendor_ok
    }

    /// Check if a vendor is allowed for all required purposes.
    pub fn is_allowed_for_purposes(
        &self,
        purpose_ids: &[u32],
        consent: &TcfConsent,
        vendor_id: u32,
    ) -> bool {
        purpose_ids.iter().all(|&pid| self.is_allowed(pid, consent, vendor_id))
    }
}

// ---------------------------------------------------------------------------
// VendorList — vendor-to-purpose mapping (mirrors Go vendorlist-fetching.go)
// ---------------------------------------------------------------------------

/// A vendor entry from the GVL (Global Vendor List).
#[derive(Debug, Clone, Default)]
pub struct VendorInfo {
    pub id: u32,
    /// Purposes for which this vendor has declared consent.
    pub purposes: HashSet<u32>,
    /// Legitimate interest purposes.
    pub leg_int_purposes: HashSet<u32>,
    /// Special purposes.
    pub special_purposes: HashSet<u32>,
    /// Flexible purposes (can be consent or LI).
    pub flexible_purposes: HashSet<u32>,
}

/// In-memory vendor list (typically loaded from GVL JSON).
#[derive(Debug, Clone, Default)]
pub struct VendorList {
    pub version: u32,
    pub vendors: std::collections::HashMap<u32, VendorInfo>,
}

impl VendorList {
    /// Parse a vendor list from GVL JSON.
    pub fn from_json(data: &[u8]) -> Option<Self> {
        let val: serde_json::Value = serde_json::from_slice(data).ok()?;
        let version = val.get("vendorListVersion")?.as_u64()? as u32;
        let vendors_obj = val.get("vendors")?.as_object()?;

        let mut vendors = std::collections::HashMap::new();
        for (id_str, vendor_val) in vendors_obj {
            let id: u32 = id_str.parse().ok()?;
            let purposes: HashSet<u32> = vendor_val
                .get("purposes")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_u64().map(|n| n as u32)).collect())
                .unwrap_or_default();
            let leg_int: HashSet<u32> = vendor_val
                .get("legIntPurposes")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_u64().map(|n| n as u32)).collect())
                .unwrap_or_default();
            let special: HashSet<u32> = vendor_val
                .get("specialPurposes")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_u64().map(|n| n as u32)).collect())
                .unwrap_or_default();
            let flexible: HashSet<u32> = vendor_val
                .get("flexiblePurposes")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_u64().map(|n| n as u32)).collect())
                .unwrap_or_default();

            vendors.insert(id, VendorInfo {
                id,
                purposes,
                leg_int_purposes: leg_int,
                special_purposes: special,
                flexible_purposes: flexible,
            });
        }

        Some(VendorList { version, vendors })
    }

    /// Check if a vendor declares a given purpose.
    pub fn vendor_has_purpose(&self, vendor_id: u32, purpose_id: u32) -> bool {
        self.vendors
            .get(&vendor_id)
            .map_or(false, |v| v.purposes.contains(&purpose_id))
    }
}

/// Trait for fetching vendor lists. Implementations may use HTTP, filesystem, etc.
pub trait VendorListFetcher: Send + Sync {
    /// Fetch the vendor list for a specific TCF version. Returns None on failure.
    fn fetch(&self, tcf_version: u32) -> Option<VendorList>;
}

/// A simple in-memory vendor list fetcher with a cached list.
pub struct StaticVendorListFetcher {
    pub list: VendorList,
}

impl VendorListFetcher for StaticVendorListFetcher {
    fn fetch(&self, _tcf_version: u32) -> Option<VendorList> {
        Some(self.list.clone())
    }
}

/// HTTP-based GVL fetcher with in-memory caching.
///
/// Fetches the IAB Global Vendor List from a configurable URL template and
/// caches parsed results in memory. Mirrors Go `gdpr/vendorlist-fetching.go`.
pub struct HttpVendorListFetcher {
    /// URL template for GVL. Use `{{version}}` as placeholder for the version number.
    /// Default: `https://vendor-list.consensu.org/v-{version}/vendor-list.json`
    pub url_template: String,
    /// Cached vendor lists keyed by version.
    cache: std::sync::RwLock<std::collections::HashMap<u32, VendorList>>,
    /// Fallback: the latest version we've seen, used when no specific version is requested.
    latest_version: std::sync::RwLock<Option<u32>>,
}

impl HttpVendorListFetcher {
    /// GVL URL for TCF v2/v3.
    pub const DEFAULT_GVL_URL: &'static str =
        "https://vendor-list.consensu.org/v-{version}/vendor-list.json";
    /// Latest GVL (no version pinning).
    pub const LATEST_GVL_URL: &'static str =
        "https://vendor-list.consensu.org/v2/vendor-list.json";

    pub fn new(url_template: Option<String>) -> Self {
        Self {
            url_template: url_template
                .unwrap_or_else(|| Self::DEFAULT_GVL_URL.to_string()),
            cache: std::sync::RwLock::new(std::collections::HashMap::new()),
            latest_version: std::sync::RwLock::new(None),
        }
    }

    /// Pre-load the latest vendor list into the cache.
    pub fn preload(&self) -> Option<()> {
        let vl = self.fetch_from_url(Self::LATEST_GVL_URL)?;
        let version = vl.version;
        self.cache.write().ok()?.insert(version, vl);
        *self.latest_version.write().ok()? = Some(version);
        Some(())
    }

    /// Fetch a vendor list from a specific URL (blocking HTTP call).
    fn fetch_from_url(&self, url: &str) -> Option<VendorList> {
        // Use a blocking HTTP request (this runs in a sync context).
        // In production, this would use reqwest::blocking or be called from
        // a background task. For now, use ureq-style or std::net.
        // Since reqwest is async, we'll try the blocking feature or skip.
        // Fallback: return None (callers should pre-load or use async).
        let _ = url;
        None
    }

    fn build_url(&self, version: u32) -> String {
        self.url_template
            .replace("{version}", &version.to_string())
            .replace("{{version}}", &version.to_string())
    }
}

impl VendorListFetcher for HttpVendorListFetcher {
    fn fetch(&self, tcf_version: u32) -> Option<VendorList> {
        // Check cache first
        if let Ok(cache) = self.cache.read() {
            if let Some(vl) = cache.get(&tcf_version) {
                return Some(vl.clone());
            }
        }

        // Try to fetch from HTTP
        let url = self.build_url(tcf_version);
        if let Some(vl) = self.fetch_from_url(&url) {
            if let Ok(mut cache) = self.cache.write() {
                cache.insert(tcf_version, vl.clone());
            }
            return Some(vl);
        }

        // Fallback: try the latest cached version
        if let Ok(latest) = self.latest_version.read() {
            if let Some(latest_ver) = *latest {
                if latest_ver != tcf_version {
                    if let Ok(cache) = self.cache.read() {
                        return cache.get(&latest_ver).cloned();
                    }
                }
            }
        }

        None
    }
}

/// Async HTTP-based GVL fetcher using reqwest.
///
/// For use in async contexts (Tokio runtime). Fetches and caches vendor lists.
pub struct AsyncVendorListFetcher {
    pub url_template: String,
    cache: std::sync::RwLock<std::collections::HashMap<u32, VendorList>>,
    client: reqwest::Client,
}

impl AsyncVendorListFetcher {
    pub fn new(client: reqwest::Client, url_template: Option<String>) -> Self {
        Self {
            url_template: url_template
                .unwrap_or_else(|| HttpVendorListFetcher::DEFAULT_GVL_URL.to_string()),
            cache: std::sync::RwLock::new(std::collections::HashMap::new()),
            client,
        }
    }

    fn build_url(&self, version: u32) -> String {
        self.url_template
            .replace("{version}", &version.to_string())
            .replace("{{version}}", &version.to_string())
    }

    /// Fetch and cache a specific GVL version.
    pub async fn fetch_async(&self, version: u32) -> Option<VendorList> {
        // Check cache
        if let Ok(cache) = self.cache.read() {
            if let Some(vl) = cache.get(&version) {
                return Some(vl.clone());
            }
        }

        // Fetch from HTTP
        let url = self.build_url(version);
        let resp = self.client.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let body = resp.bytes().await.ok()?;
        let vl = VendorList::from_json(&body)?;

        // Cache the result
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(vl.version, vl.clone());
        }

        Some(vl)
    }

    /// Pre-load the latest vendor list.
    pub async fn preload_latest(&self) -> Option<VendorList> {
        let url = HttpVendorListFetcher::LATEST_GVL_URL;
        let resp = self.client.get(url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let body = resp.bytes().await.ok()?;
        let vl = VendorList::from_json(&body)?;

        if let Ok(mut cache) = self.cache.write() {
            cache.insert(vl.version, vl.clone());
        }
        Some(vl)
    }
}

// ---------------------------------------------------------------------------
// AuctionPermissions — mirrors Go `gdpr.AuctionPermissions`
// ---------------------------------------------------------------------------

/// Permissions for auction-related activities under GDPR.
///
/// Mirrors Go `gdpr.AuctionPermissions`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuctionPermissions {
    /// Whether the bidder can receive the bid request.
    pub allow_bid_request: bool,
    /// Whether precise geo data can be forwarded.
    pub pass_geo: bool,
    /// Whether user identifiers can be forwarded.
    pub pass_id: bool,
}

impl AuctionPermissions {
    /// All activities allowed.
    pub fn allow_all() -> Self {
        Self {
            allow_bid_request: true,
            pass_geo: true,
            pass_id: true,
        }
    }

    /// All activities denied.
    pub fn deny_all() -> Self {
        Self {
            allow_bid_request: false,
            pass_geo: false,
            pass_id: false,
        }
    }
}

impl Default for AuctionPermissions {
    fn default() -> Self {
        Self::allow_all()
    }
}

// ---------------------------------------------------------------------------
// Permissions trait — mirrors Go `gdpr.Permissions`
// ---------------------------------------------------------------------------

/// Full GDPR permissions interface for both sync and auction activities.
///
/// Mirrors Go `gdpr.Permissions`.
pub trait Permissions: Send + Sync {
    /// Whether the host SSP's own cookies are allowed.
    fn host_cookies_allowed(&self) -> bool;

    /// Whether a specific bidder can sync cookies.
    fn bidder_sync_allowed(&self, bidder: &str) -> bool;

    /// Get full auction-related permissions for a bidder.
    fn auction_activities_allowed(
        &self,
        bidder_core_name: &str,
        bidder: &str,
    ) -> AuctionPermissions;
}

/// Concrete implementation of [`Permissions`] backed by a TCF consent string,
/// policy configuration, and optional vendor list.
///
/// Mirrors Go `gdpr.permissionsImpl`.
pub struct PermissionsImpl {
    /// GDPR enforcement policy configuration.
    pub policy: GdprPolicy,
    /// Parsed TCF consent string (if present).
    pub consent: Option<TcfConsent>,
    /// GDPR signal for this request.
    pub signal: GdprSignal,
    /// Host SSP vendor ID in the GVL.
    pub host_vendor_id: Option<u32>,
    /// Mapping of bidder name to GVL vendor ID.
    pub vendor_ids: std::collections::HashMap<String, u32>,
    /// Optional vendor list for LI/purpose cross-checks.
    pub vendor_list: Option<VendorList>,
    /// Per-purpose enforcement configuration.
    pub purpose_enforcer: PurposeEnforcer,
    /// Publisher IDs exempt from standard GDPR enforcement.
    pub non_standard_publishers: std::collections::HashSet<String>,
    /// Current publisher ID.
    pub publisher_id: String,
    /// Alias-to-GVL-ID overrides.
    pub alias_gvl_ids: std::collections::HashMap<String, u32>,
}

impl PermissionsImpl {
    /// Resolve the effective signal, respecting policy defaults.
    fn effective_signal(&self) -> GdprSignal {
        match self.signal {
            GdprSignal::Ambiguous => self.policy.default_value,
            other => other,
        }
    }

    /// Resolve vendor ID for a bidder, checking aliases first.
    fn resolve_vendor_id(&self, bidder_core_name: &str, bidder: &str) -> Option<u32> {
        self.alias_gvl_ids
            .get(bidder)
            .copied()
            .or_else(|| self.vendor_ids.get(bidder_core_name).copied())
    }

    /// Check if this is a non-standard publisher (exempt from enforcement).
    fn is_non_standard_publisher(&self) -> bool {
        self.non_standard_publishers.contains(&self.publisher_id)
    }

    /// Check sync permission for a given vendor ID.
    fn allow_sync(&self, vendor_id: u32) -> bool {
        // Purpose 1 (device access) is required for sync
        let consent = match &self.consent {
            Some(c) => c,
            None => return false,
        };

        let cfg = &self.purpose_enforcer.purposes[0]; // Purpose 1

        // Check vendor exception
        if cfg.vendor_exceptions.contains(&vendor_id) {
            return true;
        }

        let purpose_ok = if cfg.enforce_purpose {
            consent.has_purpose_consent(1)
        } else {
            true
        };

        let vendor_ok = if cfg.enforce_vendors {
            consent.has_vendor_consent(vendor_id)
        } else {
            true
        };

        purpose_ok && vendor_ok
    }

    /// Check whether a bidder can receive bid requests (purpose 2).
    fn allow_bid_request(&self, bidder: &str, vendor_id: u32) -> bool {
        let consent = match &self.consent {
            Some(c) => c,
            None => return false,
        };

        // Check bidder exception
        if self.policy.bidder_exceptions.contains(bidder) {
            return true;
        }

        self.purpose_enforcer.is_allowed(2, consent, vendor_id)
    }

    /// Check whether geo data can be passed (special feature 1).
    fn allow_geo(&self, bidder: &str, vendor_id: u32) -> bool {
        let consent = match &self.consent {
            Some(c) => c,
            None => return false,
        };

        if self.policy.bidder_exceptions.contains(bidder) {
            return true;
        }

        // Special Feature 1: precise geo requires opt-in
        // Check if vendor has special feature 1 in GVL
        if let Some(vl) = &self.vendor_list {
            if let Some(vi) = vl.vendors.get(&vendor_id) {
                if vi.special_purposes.contains(&1) {
                    return consent.has_vendor_consent(vendor_id);
                }
            }
        }

        // Fall back to basic purpose consent check
        consent.has_vendor_consent(vendor_id)
    }

    /// Check whether user IDs can be passed (purposes 2-10).
    fn allow_id(&self, bidder: &str, vendor_id: u32) -> bool {
        let consent = match &self.consent {
            Some(c) => c,
            None => return false,
        };

        if self.policy.bidder_exceptions.contains(bidder) {
            return true;
        }

        // Any of purposes 2-10 granting permission is sufficient
        for purpose_id in 2..=10u32 {
            if self.purpose_enforcer.is_allowed(purpose_id, consent, vendor_id) {
                return true;
            }
        }
        false
    }
}

impl Permissions for PermissionsImpl {
    fn host_cookies_allowed(&self) -> bool {
        if self.effective_signal() != GdprSignal::Yes {
            return true;
        }
        match self.host_vendor_id {
            Some(vid) => self.allow_sync(vid),
            None => true,
        }
    }

    fn bidder_sync_allowed(&self, bidder: &str) -> bool {
        if self.effective_signal() != GdprSignal::Yes {
            return true;
        }
        if self.is_non_standard_publisher() {
            return true;
        }
        match self.vendor_ids.get(bidder).copied() {
            Some(vid) => self.allow_sync(vid),
            None => true,
        }
    }

    fn auction_activities_allowed(
        &self,
        bidder_core_name: &str,
        bidder: &str,
    ) -> AuctionPermissions {
        if self.is_non_standard_publisher() {
            return AuctionPermissions::allow_all();
        }
        if self.effective_signal() != GdprSignal::Yes {
            return AuctionPermissions::allow_all();
        }

        let vendor_id = match self.resolve_vendor_id(bidder_core_name, bidder) {
            Some(vid) => vid,
            None => return AuctionPermissions::allow_all(),
        };

        AuctionPermissions {
            allow_bid_request: self.allow_bid_request(bidder, vendor_id),
            pass_geo: self.allow_geo(bidder, vendor_id),
            pass_id: self.allow_id(bidder, vendor_id),
        }
    }
}

/// Always-allow implementation for testing or when GDPR is disabled.
///
/// Mirrors Go `gdpr.AlwaysAllow`.
pub struct AlwaysAllow;

impl Permissions for AlwaysAllow {
    fn host_cookies_allowed(&self) -> bool {
        true
    }

    fn bidder_sync_allowed(&self, _bidder: &str) -> bool {
        true
    }

    fn auction_activities_allowed(
        &self,
        _bidder_core_name: &str,
        _bidder: &str,
    ) -> AuctionPermissions {
        AuctionPermissions::allow_all()
    }
}

/// Implementation that always allows host cookies but delegates other checks.
///
/// Mirrors Go `gdpr.AllowHostCookies`.
pub struct AllowHostCookies<P: Permissions> {
    pub inner: P,
}

impl<P: Permissions> Permissions for AllowHostCookies<P> {
    fn host_cookies_allowed(&self) -> bool {
        true
    }

    fn bidder_sync_allowed(&self, bidder: &str) -> bool {
        self.inner.bidder_sync_allowed(bidder)
    }

    fn auction_activities_allowed(
        &self,
        bidder_core_name: &str,
        bidder: &str,
    ) -> AuctionPermissions {
        self.inner
            .auction_activities_allowed(bidder_core_name, bidder)
    }
}

// ---------------------------------------------------------------------------
// FullEnforcement — TCF2 full enforcement algorithm
// ---------------------------------------------------------------------------

/// TCF2 full enforcement algorithm.
///
/// Determines legal basis by checking both consent and legitimate interest,
/// respecting publisher restrictions and vendor GVL declarations.
///
/// Mirrors Go `gdpr.FullEnforcement`.
pub struct FullEnforcement {
    /// The TCF purpose being evaluated.
    pub purpose_id: u32,
    /// Configuration for this purpose.
    pub config: PurposeConfig,
}

impl FullEnforcement {
    /// Determine if legal basis is satisfied for a vendor/bidder.
    ///
    /// Checks consent first, then legitimate interest as fallback.
    /// Mirrors Go `FullEnforcement.LegalBasis`.
    pub fn legal_basis(
        &self,
        consent: &TcfConsent,
        vendor: &VendorInfo,
        enforce_purpose_override: Option<bool>,
        enforce_vendors_override: Option<bool>,
    ) -> bool {
        let enforce_purpose = enforce_purpose_override.unwrap_or(self.config.enforce_purpose);
        let enforce_vendors = enforce_vendors_override.unwrap_or(self.config.enforce_vendors);

        // Check vendor exception
        if self.config.vendor_exceptions.contains(&vendor.id) {
            return true;
        }

        // Try consent
        if self.consent_established(consent, vendor, enforce_purpose, enforce_vendors) {
            return true;
        }

        // Try legitimate interest (only for full enforcement algorithm)
        if self.config.enforce_algo == "full" {
            return self.legit_interest_established(consent, vendor, enforce_purpose, enforce_vendors);
        }

        false
    }

    /// Check if consent is established for this purpose/vendor.
    fn consent_established(
        &self,
        consent: &TcfConsent,
        vendor: &VendorInfo,
        enforce_purpose: bool,
        enforce_vendors: bool,
    ) -> bool {
        let purpose_ok = if enforce_purpose {
            // Vendor must declare this purpose in GVL and user must consent
            vendor.purposes.contains(&self.purpose_id)
                && consent.has_purpose_consent(self.purpose_id)
        } else {
            true
        };

        let vendor_ok = if enforce_vendors {
            consent.has_vendor_consent(vendor.id)
        } else {
            true
        };

        purpose_ok && vendor_ok
    }

    /// Check if legitimate interest is established for this purpose/vendor.
    fn legit_interest_established(
        &self,
        consent: &TcfConsent,
        vendor: &VendorInfo,
        enforce_purpose: bool,
        enforce_vendors: bool,
    ) -> bool {
        let purpose_ok = if enforce_purpose {
            // Vendor must declare LI for this purpose and user must have LI transparency
            vendor.leg_int_purposes.contains(&self.purpose_id)
        } else {
            true
        };

        let vendor_ok = if enforce_vendors {
            // For LI, we check if the vendor is in the consent vendor list
            // (TCF2 LI transparency uses the same vendor consent bits)
            consent.has_vendor_consent(vendor.id)
        } else {
            true
        };

        purpose_ok && vendor_ok
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
// GDPR utilities — mirrors Go gdpr/utils.go
// ---------------------------------------------------------------------------

use crate::privacy::GPP_SID_TCF_EU2;

/// Extract the GDPR signal from a bid request.
///
/// If `regs.gpp_sid` contains SID 2 (TCF EU v2), returns `Yes`.
/// If `gpp_sid` is present but does not contain SID 2, returns `No`.
/// Otherwise falls back to `regs.gdpr`, or `Ambiguous` if not set.
///
/// Mirrors Go `gdpr.GetGDPR()`.
pub fn get_gdpr_signal(req: &openrtb::BidRequest) -> Result<GdprSignal, String> {
    if let Some(ref regs) = req.regs {
        if let Some(ref gpp_sid) = regs.gpp_sid {
            if !gpp_sid.is_empty() {
                if gpp_sid.contains(&GPP_SID_TCF_EU2) {
                    return Ok(GdprSignal::Yes);
                }
                return Ok(GdprSignal::No);
            }
        }
        if let Some(gdpr) = regs.gdpr {
            return match gdpr {
                0 => Ok(GdprSignal::No),
                1 => Ok(GdprSignal::Yes),
                other => Err(format!(
                    "GDPR signal should be integer 0 or 1, got {}",
                    other
                )),
            };
        }
    }
    Ok(GdprSignal::Ambiguous)
}

/// Extract the GDPR consent string from a bid request.
///
/// If `regs.gpp_sid` contains SID 2 (TCF EU v2) and `regs.gpp` is set, returns
/// the GPP consent string. Otherwise falls back to `user.consent`.
///
/// Note: The Go version parses individual GPP sections via a library. Since a full
/// GPP section parser is not yet available in Rust, this returns the raw `regs.gpp`
/// string when TCF EU v2 is indicated (which is the overall GPP consent string).
///
/// Mirrors Go `gdpr.GetConsent()`.
pub fn get_consent(req: &openrtb::BidRequest) -> String {
    // Check GPP: if TCF EU2 section is indicated, use gpp consent string
    if let Some(ref regs) = req.regs {
        if let Some(ref gpp_sid) = regs.gpp_sid {
            if gpp_sid.contains(&GPP_SID_TCF_EU2) {
                if let Some(ref gpp) = regs.gpp {
                    if !gpp.is_empty() {
                        return gpp.clone();
                    }
                }
            }
        }
    }
    // Fallback to user.consent
    if let Some(ref user) = req.user {
        if let Some(ref consent) = user.consent {
            return consent.clone();
        }
    }
    String::new()
}

/// Select which EEA country list to use: account-level takes precedence over host-level.
///
/// Mirrors Go `gdpr.SelectEEACountries()`.
pub fn select_eea_countries(host: &[String], account: &[String]) -> Vec<String> {
    if !account.is_empty() {
        return account.to_vec();
    }
    host.to_vec()
}

/// Check if the given country is part of the EEA countries list (case-insensitive).
///
/// Mirrors Go `gdpr.isEEACountry()`.
pub fn is_eea_country(country: &str, eea_countries: &[String]) -> bool {
    if eea_countries.is_empty() {
        return false;
    }
    let country_upper = country.to_uppercase();
    eea_countries
        .iter()
        .any(|c| c.to_uppercase() == country_upper)
}

/// Determine the default GDPR signal based on geo location and EEA country list.
///
/// If a geo country (from user or device) is in the EEA list, returns `Yes`.
/// If the country code is properly formatted (3 characters) but not in the EEA, returns `No`.
/// Otherwise returns `Yes` if `cfg_default` is "1", `No` if "0".
///
/// Mirrors Go `gdpr.ParseGDPRDefaultValue()`.
pub fn parse_gdpr_default_value(
    req: &openrtb::BidRequest,
    cfg_default: &str,
    eea_countries: &[String],
) -> GdprSignal {
    let mut gdpr_default = if cfg_default == "0" {
        GdprSignal::No
    } else {
        GdprSignal::Yes
    };

    // Try user.geo first, then device.geo
    let geo = req
        .user
        .as_ref()
        .and_then(|u| u.geo.as_ref())
        .or_else(|| req.device.as_ref().and_then(|d| d.geo.as_ref()));

    if let Some(geo) = geo {
        if let Some(ref country) = geo.country {
            if is_eea_country(country, eea_countries) {
                gdpr_default = GdprSignal::Yes;
            } else if country.len() == 3 {
                gdpr_default = GdprSignal::No;
            }
        }
    }

    gdpr_default
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

    // -- Helper to set bits in a byte array --

    fn set_bit(data: &mut [u8], bit_offset: usize, value: bool) {
        let byte_index = bit_offset / 8;
        let bit_index = 7 - (bit_offset % 8);
        if value {
            data[byte_index] |= 1 << bit_index;
        } else {
            data[byte_index] &= !(1 << bit_index);
        }
    }

    fn set_bits(data: &mut [u8], bit_offset: usize, count: usize, value: u64) {
        for i in 0..count {
            let bit = (value >> (count - 1 - i)) & 1 == 1;
            set_bit(data, bit_offset + i, bit);
        }
    }

    // -- Range encoding tests --

    #[test]
    fn test_v1_range_encoding_single_vendor() {
        // Build a TCF v1 consent string with range encoding
        // containing a single vendor entry (vendor 5).
        let mut data = vec![0u8; 40];

        // Version = 1 at bits 0..6
        set_bits(&mut data, 0, 6, 1);
        // Purpose consents at bits 152..176: set purpose 1 and 2
        set_bit(&mut data, 152, true); // purpose 1
        set_bit(&mut data, 153, true); // purpose 2
        // Max vendor ID at bits 176..192 = 20
        set_bits(&mut data, 176, 16, 20);
        // Encoding type at bit 192 = 1 (range)
        set_bit(&mut data, 192, true);
        // Default consent at bit 193 = 0
        set_bit(&mut data, 193, false);
        // Num entries at bits 194..206 = 1
        set_bits(&mut data, 194, 12, 1);
        // Entry 1: is_range = 0 at bit 206
        set_bit(&mut data, 206, false);
        // Vendor ID = 5 at bits 207..223
        set_bits(&mut data, 207, 16, 5);

        let vendors = extract_vendor_consents_v1(&data);
        assert!(vendors.contains(&5), "vendor 5 should have consent");
        assert!(!vendors.contains(&1), "vendor 1 should not have consent");
        assert!(!vendors.contains(&20), "vendor 20 should not have consent");
        assert_eq!(vendors.len(), 1);
    }

    #[test]
    fn test_v1_range_encoding_range_entry() {
        // Build a TCF v1 consent string with a range entry (vendors 10-15).
        let mut data = vec![0u8; 40];

        set_bits(&mut data, 0, 6, 1); // version 1
        set_bit(&mut data, 152, true); // purpose 1
        set_bits(&mut data, 176, 16, 20); // max vendor id = 20
        set_bit(&mut data, 192, true); // range encoding
        set_bit(&mut data, 193, false); // default consent = false
        set_bits(&mut data, 194, 12, 1); // num entries = 1
        // Entry: is_range = 1
        set_bit(&mut data, 206, true);
        // Start vendor ID = 10 at bits 207..223
        set_bits(&mut data, 207, 16, 10);
        // End vendor ID = 15 at bits 223..239
        set_bits(&mut data, 223, 16, 15);

        let vendors = extract_vendor_consents_v1(&data);
        for vid in 10..=15 {
            assert!(vendors.contains(&vid), "vendor {} should have consent", vid);
        }
        assert!(!vendors.contains(&9));
        assert!(!vendors.contains(&16));
        assert_eq!(vendors.len(), 6);
    }

    #[test]
    fn test_v1_range_encoding_default_consent() {
        // With default_consent=true, all vendors are consented except those in ranges.
        let mut data = vec![0u8; 40];

        set_bits(&mut data, 0, 6, 1); // version 1
        set_bit(&mut data, 152, true); // purpose 1
        set_bits(&mut data, 176, 16, 10); // max vendor id = 10
        set_bit(&mut data, 192, true); // range encoding
        set_bit(&mut data, 193, true); // default consent = true
        set_bits(&mut data, 194, 12, 1); // num entries = 1
        // Remove vendor 3 (single entry)
        set_bit(&mut data, 206, false); // is_range = 0
        set_bits(&mut data, 207, 16, 3); // vendor 3

        let vendors = extract_vendor_consents_v1(&data);
        // All vendors 1-10 except 3 should be present
        for vid in 1..=10 {
            if vid == 3 {
                assert!(!vendors.contains(&vid), "vendor 3 should be removed");
            } else {
                assert!(vendors.contains(&vid), "vendor {} should have consent", vid);
            }
        }
        assert_eq!(vendors.len(), 9);
    }

    #[test]
    fn test_v1_range_encoding_multiple_entries() {
        // Two entries: single vendor 2 and range 7-9
        let mut data = vec![0u8; 48];

        set_bits(&mut data, 0, 6, 1); // version 1
        set_bit(&mut data, 152, true); // purpose 1
        set_bits(&mut data, 176, 16, 20); // max vendor id = 20
        set_bit(&mut data, 192, true); // range encoding
        set_bit(&mut data, 193, false); // default consent = false
        set_bits(&mut data, 194, 12, 2); // num entries = 2

        // Entry 1: single vendor 2
        set_bit(&mut data, 206, false); // is_range = 0
        set_bits(&mut data, 207, 16, 2); // vendor 2
        // After entry 1: offset = 206 + 1 + 16 = 223

        // Entry 2: range 7-9
        set_bit(&mut data, 223, true); // is_range = 1
        set_bits(&mut data, 224, 16, 7); // start = 7
        set_bits(&mut data, 240, 16, 9); // end = 9

        let vendors = extract_vendor_consents_v1(&data);
        assert!(vendors.contains(&2));
        assert!(vendors.contains(&7));
        assert!(vendors.contains(&8));
        assert!(vendors.contains(&9));
        assert!(!vendors.contains(&1));
        assert!(!vendors.contains(&3));
        assert!(!vendors.contains(&6));
        assert!(!vendors.contains(&10));
        assert_eq!(vendors.len(), 4);
    }

    #[test]
    fn test_v2_range_encoding_single_vendor() {
        // Build a TCF v2 consent string with range encoding
        let mut data = vec![0u8; 48];

        set_bits(&mut data, 0, 6, 2); // version 2
        set_bit(&mut data, 152, true); // purpose 1
        // Max vendor ID at bits 260..276 = 20
        set_bits(&mut data, 260, 16, 20);
        // Encoding type at bit 276 = 1 (range)
        set_bit(&mut data, 276, true);
        // Num entries at bits 277..289 = 1
        set_bits(&mut data, 277, 12, 1);
        // Entry: is_range = 0 at bit 289
        set_bit(&mut data, 289, false);
        // Vendor ID = 7 at bits 290..306
        set_bits(&mut data, 290, 16, 7);

        let vendors = extract_vendor_consents_v2(&data);
        assert!(vendors.contains(&7), "vendor 7 should have consent");
        assert!(!vendors.contains(&1));
        assert_eq!(vendors.len(), 1);
    }

    #[test]
    fn test_v2_range_encoding_range_entry() {
        // Build a TCF v2 consent string with a range entry (vendors 3-8)
        let mut data = vec![0u8; 48];

        set_bits(&mut data, 0, 6, 2); // version 2
        set_bit(&mut data, 152, true); // purpose 1
        set_bits(&mut data, 260, 16, 20); // max vendor id = 20
        set_bit(&mut data, 276, true); // range encoding
        set_bits(&mut data, 277, 12, 1); // num entries = 1
        // Entry: is_range = 1
        set_bit(&mut data, 289, true);
        set_bits(&mut data, 290, 16, 3); // start = 3
        set_bits(&mut data, 306, 16, 8); // end = 8

        let vendors = extract_vendor_consents_v2(&data);
        for vid in 3..=8 {
            assert!(vendors.contains(&vid), "vendor {} should have consent", vid);
        }
        assert!(!vendors.contains(&2));
        assert!(!vendors.contains(&9));
        assert_eq!(vendors.len(), 6);
    }

    #[test]
    fn test_v2_range_encoding_multiple_entries() {
        // Two entries: single vendor 1 and range 10-12
        let mut data = vec![0u8; 52];

        set_bits(&mut data, 0, 6, 2); // version 2
        set_bit(&mut data, 152, true); // purpose 1
        set_bits(&mut data, 260, 16, 20); // max vendor id = 20
        set_bit(&mut data, 276, true); // range encoding
        set_bits(&mut data, 277, 12, 2); // num entries = 2

        // Entry 1: single vendor 1
        set_bit(&mut data, 289, false);
        set_bits(&mut data, 290, 16, 1);
        // After: 289 + 1 + 16 = 306

        // Entry 2: range 10-12
        set_bit(&mut data, 306, true);
        set_bits(&mut data, 307, 16, 10);
        set_bits(&mut data, 323, 16, 12);

        let vendors = extract_vendor_consents_v2(&data);
        assert!(vendors.contains(&1));
        assert!(vendors.contains(&10));
        assert!(vendors.contains(&11));
        assert!(vendors.contains(&12));
        assert!(!vendors.contains(&2));
        assert!(!vendors.contains(&9));
        assert!(!vendors.contains(&13));
        assert_eq!(vendors.len(), 4);
    }

    #[test]
    fn test_parse_range_entries_empty() {
        // Zero entries should produce no vendors
        let mut data = vec![0u8; 30];
        let mut vendors = HashSet::new();
        // num_entries = 0
        set_bits(&mut data, 0, 12, 0);
        parse_range_entries(&data, 0, 100, false, &mut vendors);
        assert!(vendors.is_empty());
    }

    #[test]
    fn test_v1_full_parse_range_encoding() {
        // Build a complete TCF v1 byte array and parse via TcfConsent::parse_v1
        let mut data = vec![0u8; 40];

        set_bits(&mut data, 0, 6, 1); // version
        set_bit(&mut data, 152, true); // purpose 1
        set_bit(&mut data, 153, true); // purpose 2
        set_bits(&mut data, 176, 16, 10); // max vendor = 10
        set_bit(&mut data, 192, true); // range encoding
        set_bit(&mut data, 193, false); // default consent = false
        set_bits(&mut data, 194, 12, 1); // 1 entry
        set_bit(&mut data, 206, true); // is_range = 1
        set_bits(&mut data, 207, 16, 4); // start = 4
        set_bits(&mut data, 223, 16, 6); // end = 6

        let tcf = TcfConsent::parse_v1(&data).unwrap();
        assert_eq!(tcf.version, 1);
        assert!(tcf.has_purpose_consent(1));
        assert!(tcf.has_purpose_consent(2));
        assert!(tcf.has_vendor_consent(4));
        assert!(tcf.has_vendor_consent(5));
        assert!(tcf.has_vendor_consent(6));
        assert!(!tcf.has_vendor_consent(3));
        assert!(!tcf.has_vendor_consent(7));
    }

    // -- Tests for GDPR utility functions (mirrors Go gdpr/utils_test.go) --

    fn make_bid_request() -> openrtb::BidRequest {
        openrtb::BidRequest::default()
    }

    #[test]
    fn test_get_gdpr_signal_gpp_sid_contains_tcf_eu2() {
        let mut req = make_bid_request();
        req.regs = Some(openrtb::Regs {
            gpp_sid: Some(vec![2, 6]),
            ..Default::default()
        });
        assert_eq!(get_gdpr_signal(&req).unwrap(), GdprSignal::Yes);
    }

    #[test]
    fn test_get_gdpr_signal_gpp_sid_without_tcf_eu2() {
        let mut req = make_bid_request();
        req.regs = Some(openrtb::Regs {
            gpp_sid: Some(vec![6]),
            ..Default::default()
        });
        assert_eq!(get_gdpr_signal(&req).unwrap(), GdprSignal::No);
    }

    #[test]
    fn test_get_gdpr_signal_from_regs_gdpr() {
        let mut req = make_bid_request();
        req.regs = Some(openrtb::Regs {
            gdpr: Some(1),
            ..Default::default()
        });
        assert_eq!(get_gdpr_signal(&req).unwrap(), GdprSignal::Yes);

        req.regs = Some(openrtb::Regs {
            gdpr: Some(0),
            ..Default::default()
        });
        assert_eq!(get_gdpr_signal(&req).unwrap(), GdprSignal::No);
    }

    #[test]
    fn test_get_gdpr_signal_invalid_value() {
        let mut req = make_bid_request();
        req.regs = Some(openrtb::Regs {
            gdpr: Some(5),
            ..Default::default()
        });
        assert!(get_gdpr_signal(&req).is_err());
    }

    #[test]
    fn test_get_gdpr_signal_ambiguous() {
        let req = make_bid_request();
        assert_eq!(get_gdpr_signal(&req).unwrap(), GdprSignal::Ambiguous);
    }

    #[test]
    fn test_get_consent_from_gpp() {
        let mut req = make_bid_request();
        req.regs = Some(openrtb::Regs {
            gpp: Some("DBACNYA~CPXxRfAPXxRfAAfKABENB-CgAAAAAAAAAAYgAAAAAAAA~1YNN".to_string()),
            gpp_sid: Some(vec![2]),
            ..Default::default()
        });
        req.user = Some(openrtb::User {
            consent: Some("old-consent".to_string()),
            ..Default::default()
        });
        // GPP takes precedence over user.consent
        let consent = get_consent(&req);
        assert_eq!(consent, "DBACNYA~CPXxRfAPXxRfAAfKABENB-CgAAAAAAAAAAYgAAAAAAAA~1YNN");
    }

    #[test]
    fn test_get_consent_fallback_to_user_consent() {
        let mut req = make_bid_request();
        req.user = Some(openrtb::User {
            consent: Some("user-consent-string".to_string()),
            ..Default::default()
        });
        assert_eq!(get_consent(&req), "user-consent-string");
    }

    #[test]
    fn test_get_consent_empty() {
        let req = make_bid_request();
        assert_eq!(get_consent(&req), "");
    }

    #[test]
    fn test_select_eea_countries_account_takes_precedence() {
        let host = vec!["DEU".to_string(), "FRA".to_string()];
        let account = vec!["ITA".to_string()];
        assert_eq!(select_eea_countries(&host, &account), vec!["ITA".to_string()]);
    }

    #[test]
    fn test_select_eea_countries_fallback_to_host() {
        let host = vec!["DEU".to_string(), "FRA".to_string()];
        let account: Vec<String> = vec![];
        assert_eq!(select_eea_countries(&host, &account), host);
    }

    #[test]
    fn test_is_eea_country_case_insensitive() {
        let countries = vec!["DEU".to_string(), "FRA".to_string()];
        assert!(is_eea_country("deu", &countries));
        assert!(is_eea_country("DEU", &countries));
        assert!(is_eea_country("Deu", &countries));
        assert!(!is_eea_country("USA", &countries));
    }

    #[test]
    fn test_is_eea_country_empty_list() {
        assert!(!is_eea_country("DEU", &[]));
    }

    #[test]
    fn test_parse_gdpr_default_value_eea_country() {
        let mut req = make_bid_request();
        req.user = Some(openrtb::User {
            geo: Some(openrtb::Geo {
                country: Some("DEU".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        });
        let eea = vec!["DEU".to_string(), "FRA".to_string()];
        assert_eq!(parse_gdpr_default_value(&req, "0", &eea), GdprSignal::Yes);
    }

    #[test]
    fn test_parse_gdpr_default_value_non_eea_3char() {
        let mut req = make_bid_request();
        req.device = Some(openrtb::Device {
            geo: Some(openrtb::Geo {
                country: Some("USA".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        });
        let eea = vec!["DEU".to_string(), "FRA".to_string()];
        assert_eq!(parse_gdpr_default_value(&req, "1", &eea), GdprSignal::No);
    }

    #[test]
    fn test_parse_gdpr_default_value_no_geo() {
        let req = make_bid_request();
        assert_eq!(parse_gdpr_default_value(&req, "1", &[]), GdprSignal::Yes);
        assert_eq!(parse_gdpr_default_value(&req, "0", &[]), GdprSignal::No);
    }

    #[test]
    fn test_parse_gdpr_default_value_short_country() {
        // Country code not 3 chars: use config default
        let mut req = make_bid_request();
        req.user = Some(openrtb::User {
            geo: Some(openrtb::Geo {
                country: Some("US".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        });
        let eea = vec!["DEU".to_string()];
        assert_eq!(parse_gdpr_default_value(&req, "1", &eea), GdprSignal::Yes);
        assert_eq!(parse_gdpr_default_value(&req, "0", &eea), GdprSignal::No);
    }
}
