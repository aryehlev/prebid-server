//! TCF 2.2 consent string decoder.
//!
//! This module decodes the "core" segment of a TCF 2.2 consent string
//! (everything up to the first `.`). The core segment carries metadata
//! (CMP id, timestamps, language), publisher-level purpose flags, and the
//! vendor-consent set (either a bitfield or a run-length range list).
//!
//! The decoder intentionally stops at the end of the core segment -
//! disclosed-vendors, publisher-restrictions, and other optional segments
//! that follow after a `.` are not currently consumed here.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::tcf2_bits::BitReader;
use crate::MalformedConsentError;

/// A single `[start, end]` vendor id range (inclusive).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VendorRange {
    pub start: u16,
    pub end: u16,
}

/// The vendor-consent section of a TCF 2.2 core segment. The encoder may
/// choose either a bitfield or a range-list representation depending on
/// which is more compact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VendorConsent {
    /// Bitfield of length `max_vendor_id`. Vendor id `i` (1-indexed) maps
    /// to index `i - 1`.
    Bitfield(Vec<bool>),
    /// Run-length encoded list of inclusive ranges.
    RangeList(Vec<VendorRange>),
}

impl VendorConsent {
    /// Returns true if the given vendor id is covered.
    pub fn contains(&self, vendor_id: u16) -> bool {
        if vendor_id == 0 {
            return false;
        }
        match self {
            VendorConsent::Bitfield(bits) => {
                let idx = (vendor_id - 1) as usize;
                bits.get(idx).copied().unwrap_or(false)
            }
            VendorConsent::RangeList(ranges) => ranges
                .iter()
                .any(|r| vendor_id >= r.start && vendor_id <= r.end),
        }
    }
}

/// A decoded TCF 2.2 core consent segment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsentString {
    // Round-trippable raw wire form (everything, not just the core).
    raw: String,

    pub version: u8,
    pub created: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
    pub cmp_id: u16,
    pub cmp_version: u16,
    pub consent_screen: u8,
    pub consent_language: String,
    pub vendor_list_version: u16,
    pub tcf_policy_version: u8,
    pub is_service_specific: bool,
    pub use_non_standard_stacks: bool,
    pub special_feature_optins: [bool; 12],
    pub purposes_consent: [bool; 24],
    pub purposes_li_transparency: [bool; 24],
    pub purpose_one_treatment: bool,
    pub publisher_cc: String,
    pub vendors_consent: VendorConsent,
}

impl ConsentString {
    /// Constructs a [`ConsentString`] by parsing the given raw wire form.
    ///
    /// This decodes the core segment only. Additional segments separated
    /// by `.` are preserved in [`Self::raw`] but not otherwise interpreted.
    pub fn parse(raw: &str) -> Result<Self, MalformedConsentError> {
        if raw.is_empty() {
            return Err(MalformedConsentError {
                consent: raw.to_string(),
                cause: "consent string is empty".into(),
            });
        }

        let core_b64 = raw.split('.').next().unwrap_or("");
        if core_b64.is_empty() {
            return Err(MalformedConsentError {
                consent: raw.to_string(),
                cause: "core segment is empty".into(),
            });
        }

        let bytes = decode_base64url(core_b64).map_err(|e| MalformedConsentError {
            consent: raw.to_string(),
            cause: format!("base64 decode error: {}", e),
        })?;

        let mut reader = BitReader::new(&bytes);
        decode_core(&mut reader)
            .map_err(|cause| MalformedConsentError {
                consent: raw.to_string(),
                cause,
            })
            .map(|mut cs| {
                cs.raw = raw.to_string();
                cs
            })
    }

    /// Backward-compatible constructor for callers that just want to hold a
    /// raw consent string without eagerly parsing it.
    pub fn new(raw: impl Into<String>) -> Self {
        let s = raw.into();
        // Best-effort parse; if parsing fails, return a minimal placeholder
        // that still round-trips the raw string and reports version 0.
        match ConsentString::parse(&s) {
            Ok(parsed) => parsed,
            Err(_) => ConsentString {
                raw: s,
                version: 0,
                created: DateTime::<Utc>::from_timestamp(0, 0).unwrap(),
                last_updated: DateTime::<Utc>::from_timestamp(0, 0).unwrap(),
                cmp_id: 0,
                cmp_version: 0,
                consent_screen: 0,
                consent_language: String::new(),
                vendor_list_version: 0,
                tcf_policy_version: 0,
                is_service_specific: false,
                use_non_standard_stacks: false,
                special_feature_optins: [false; 12],
                purposes_consent: [false; 24],
                purposes_li_transparency: [false; 24],
                purpose_one_treatment: false,
                publisher_cc: String::new(),
                vendors_consent: VendorConsent::Bitfield(Vec::new()),
            },
        }
    }

    /// Raw round-trippable wire form.
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// Alias for [`Self::raw`] kept for compatibility with older callers.
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// Consumes the wrapper and returns the raw string.
    pub fn into_inner(self) -> String {
        self.raw
    }

    /// Returns `true` if the raw wire form is empty.
    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    /// Returns the decoded specification version. Fallible for
    /// backward-compatibility with callers that handle the error path.
    pub fn version(&self) -> Result<u8, MalformedConsentError> {
        if self.raw.is_empty() {
            return Err(MalformedConsentError {
                consent: self.raw.clone(),
                cause: "consent string is empty".into(),
            });
        }
        Ok(self.version)
    }
}

impl From<String> for ConsentString {
    fn from(s: String) -> Self {
        ConsentString::new(s)
    }
}

impl From<&str> for ConsentString {
    fn from(s: &str) -> Self {
        ConsentString::new(s.to_string())
    }
}

impl AsRef<str> for ConsentString {
    fn as_ref(&self) -> &str {
        &self.raw
    }
}

/// Decodes a base64url payload, padding manually if necessary.
pub(crate) fn decode_base64url(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    // URL_SAFE_NO_PAD already accepts unpadded input; we additionally strip
    // any stray '=' padding so that callers may pass padded or unpadded
    // strings interchangeably.
    let trimmed = s.trim_end_matches('=');
    URL_SAFE_NO_PAD.decode(trimmed.as_bytes())
}

/// Decodes the fixed-width prefix of a TCF 2.2 core segment up to and
/// including the vendor-consent section.
fn decode_core(r: &mut BitReader<'_>) -> Result<ConsentString, String> {
    let version = r.read_u6().map_err(|e| format!("version: {}", e))?;
    let created = r.read_datetime().map_err(|e| format!("created: {}", e))?;
    let last_updated = r
        .read_datetime()
        .map_err(|e| format!("last_updated: {}", e))?;
    let cmp_id = r.read_u12().map_err(|e| format!("cmp_id: {}", e))?;
    let cmp_version = r.read_u12().map_err(|e| format!("cmp_version: {}", e))?;
    let consent_screen = r.read_u6().map_err(|e| format!("consent_screen: {}", e))?;
    let consent_language = r
        .read_language()
        .map_err(|e| format!("consent_language: {}", e))?;
    let vendor_list_version = r
        .read_u12()
        .map_err(|e| format!("vendor_list_version: {}", e))?;
    let tcf_policy_version = r
        .read_u6()
        .map_err(|e| format!("tcf_policy_version: {}", e))?;
    let is_service_specific = r
        .read_bool()
        .map_err(|e| format!("is_service_specific: {}", e))?;
    let use_non_standard_stacks = r
        .read_bool()
        .map_err(|e| format!("use_non_standard_stacks: {}", e))?;

    let mut special_feature_optins = [false; 12];
    for (i, slot) in special_feature_optins.iter_mut().enumerate() {
        *slot = r
            .read_bool()
            .map_err(|e| format!("special_feature_optins[{}]: {}", i, e))?;
    }

    let mut purposes_consent = [false; 24];
    for (i, slot) in purposes_consent.iter_mut().enumerate() {
        *slot = r
            .read_bool()
            .map_err(|e| format!("purposes_consent[{}]: {}", i, e))?;
    }

    let mut purposes_li_transparency = [false; 24];
    for (i, slot) in purposes_li_transparency.iter_mut().enumerate() {
        *slot = r
            .read_bool()
            .map_err(|e| format!("purposes_li_transparency[{}]: {}", i, e))?;
    }

    let purpose_one_treatment = r
        .read_bool()
        .map_err(|e| format!("purpose_one_treatment: {}", e))?;
    let publisher_cc = r
        .read_language()
        .map_err(|e| format!("publisher_cc: {}", e))?;

    let vendors_consent =
        decode_vendor_section(r).map_err(|e| format!("vendors_consent: {}", e))?;

    Ok(ConsentString {
        raw: String::new(), // filled in by caller
        version,
        created,
        last_updated,
        cmp_id,
        cmp_version,
        consent_screen,
        consent_language,
        vendor_list_version,
        tcf_policy_version,
        is_service_specific,
        use_non_standard_stacks,
        special_feature_optins,
        purposes_consent,
        purposes_li_transparency,
        purpose_one_treatment,
        publisher_cc,
        vendors_consent,
    })
}

/// Decodes a vendor consent section: `max_vendor_id(16) | is_range(1) |
/// { bitfield | range_list }`.
fn decode_vendor_section(r: &mut BitReader<'_>) -> Result<VendorConsent, String> {
    let max_vendor_id = r.read_u16().map_err(|e| format!("max_vendor_id: {}", e))?;
    let is_range = r
        .read_bool()
        .map_err(|e| format!("is_range_encoding: {}", e))?;

    if is_range {
        let num_entries = r.read_u12().map_err(|e| format!("num_entries: {}", e))?;
        let mut ranges = Vec::with_capacity(num_entries as usize);
        for i in 0..num_entries {
            let is_a_range = r
                .read_bool()
                .map_err(|e| format!("range[{}].is_range: {}", i, e))?;
            let start = r
                .read_u16()
                .map_err(|e| format!("range[{}].start: {}", i, e))?;
            let end = if is_a_range {
                r.read_u16()
                    .map_err(|e| format!("range[{}].end: {}", i, e))?
            } else {
                start
            };
            if start == 0 || end < start || end > max_vendor_id {
                return Err(format!(
                    "range[{}] invalid: start={} end={} max_vendor_id={}",
                    i, start, end, max_vendor_id
                ));
            }
            ranges.push(VendorRange { start, end });
        }
        Ok(VendorConsent::RangeList(ranges))
    } else {
        let mut bits = Vec::with_capacity(max_vendor_id as usize);
        for i in 0..max_vendor_id {
            bits.push(
                r.read_bool()
                    .map_err(|e| format!("bitfield[{}]: {}", i, e))?,
            );
        }
        Ok(VendorConsent::Bitfield(bits))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    /// Small helper that writes TCF consent bit streams for tests. All
    /// writes push to the MSB-first bit order.
    struct BitWriter {
        bits: Vec<bool>,
    }

    impl BitWriter {
        fn new() -> Self {
            BitWriter { bits: Vec::new() }
        }

        fn write_bits(&mut self, value: u64, n: usize) {
            for i in (0..n).rev() {
                self.bits.push(((value >> i) & 1) == 1);
            }
        }

        fn write_bool(&mut self, b: bool) {
            self.bits.push(b);
        }

        fn write_language(&mut self, s: &str) {
            let chars: Vec<char> = s.chars().collect();
            assert_eq!(chars.len(), 2);
            for c in chars {
                let v = (c.to_ascii_uppercase() as u8 - b'A') as u64;
                self.write_bits(v, 6);
            }
        }

        fn finish(mut self) -> Vec<u8> {
            // Pad with zero bits up to byte boundary.
            while self.bits.len() % 8 != 0 {
                self.bits.push(false);
            }
            let mut out = Vec::with_capacity(self.bits.len() / 8);
            for chunk in self.bits.chunks(8) {
                let mut byte: u8 = 0;
                for (i, b) in chunk.iter().enumerate() {
                    if *b {
                        byte |= 1 << (7 - i);
                    }
                }
                out.push(byte);
            }
            out
        }
    }

    /// Build a synthetic TCF 2.2 core segment with the given fields and
    /// return the base64url-encoded string.
    #[allow(clippy::too_many_arguments)]
    fn build_core(
        created_ds: u64,
        updated_ds: u64,
        cmp_id: u16,
        cmp_version: u16,
        consent_screen: u8,
        consent_language: &str,
        vendor_list_version: u16,
        tcf_policy_version: u8,
        is_service_specific: bool,
        use_non_standard_stacks: bool,
        special_feature_optins: [bool; 12],
        purposes_consent: [bool; 24],
        purposes_li_transparency: [bool; 24],
        purpose_one_treatment: bool,
        publisher_cc: &str,
        vendors: VendorConsent,
    ) -> String {
        let mut w = BitWriter::new();
        w.write_bits(2, 6); // version
        w.write_bits(created_ds, 36);
        w.write_bits(updated_ds, 36);
        w.write_bits(cmp_id as u64, 12);
        w.write_bits(cmp_version as u64, 12);
        w.write_bits(consent_screen as u64, 6);
        w.write_language(consent_language);
        w.write_bits(vendor_list_version as u64, 12);
        w.write_bits(tcf_policy_version as u64, 6);
        w.write_bool(is_service_specific);
        w.write_bool(use_non_standard_stacks);
        for b in special_feature_optins {
            w.write_bool(b);
        }
        for b in purposes_consent {
            w.write_bool(b);
        }
        for b in purposes_li_transparency {
            w.write_bool(b);
        }
        w.write_bool(purpose_one_treatment);
        w.write_language(publisher_cc);

        match vendors {
            VendorConsent::Bitfield(bits) => {
                let max = bits.len() as u16;
                w.write_bits(max as u64, 16);
                w.write_bool(false); // is_range_encoding
                for b in bits {
                    w.write_bool(b);
                }
            }
            VendorConsent::RangeList(ranges) => {
                let max = ranges.iter().map(|r| r.end).max().unwrap_or(0);
                w.write_bits(max as u64, 16);
                w.write_bool(true);
                w.write_bits(ranges.len() as u64, 12);
                for r in ranges {
                    let is_range = r.end != r.start;
                    w.write_bool(is_range);
                    w.write_bits(r.start as u64, 16);
                    if is_range {
                        w.write_bits(r.end as u64, 16);
                    }
                }
            }
        }

        URL_SAFE_NO_PAD.encode(w.finish())
    }

    fn base_optins_consent() -> ([bool; 12], [bool; 24], [bool; 24]) {
        let mut sp = [false; 12];
        sp[0] = true;
        sp[5] = true;
        let mut pc = [false; 24];
        pc[0] = true;
        pc[1] = true;
        pc[9] = true;
        pc[23] = true;
        let mut pli = [false; 24];
        pli[2] = true;
        pli[3] = true;
        (sp, pc, pli)
    }

    #[test]
    fn parse_synthetic_bitfield_consent() {
        let (sp, pc, pli) = base_optins_consent();
        let vendors =
            VendorConsent::Bitfield(vec![true, false, true, true, false, false, false, true]);
        let raw = build_core(
            16_000_000_000, // 1600000000000 ms ~= 2020-09-13
            16_500_000_000,
            42,
            3,
            1,
            "EN",
            150,
            4,
            true,
            false,
            sp,
            pc,
            pli,
            false,
            "US",
            vendors.clone(),
        );

        let parsed = ConsentString::parse(&raw).expect("parse ok");
        assert_eq!(parsed.version, 2);
        assert_eq!(parsed.cmp_id, 42);
        assert_eq!(parsed.cmp_version, 3);
        assert_eq!(parsed.consent_screen, 1);
        assert_eq!(parsed.consent_language, "EN");
        assert_eq!(parsed.vendor_list_version, 150);
        assert_eq!(parsed.tcf_policy_version, 4);
        assert!(parsed.is_service_specific);
        assert!(!parsed.use_non_standard_stacks);
        assert_eq!(parsed.special_feature_optins, sp);
        assert_eq!(parsed.purposes_consent, pc);
        assert_eq!(parsed.purposes_li_transparency, pli);
        assert!(!parsed.purpose_one_treatment);
        assert_eq!(parsed.publisher_cc, "US");
        assert_eq!(parsed.vendors_consent, vendors);
        assert_eq!(parsed.raw(), raw);
        assert_eq!(parsed.version().unwrap(), 2);
    }

    #[test]
    fn parse_synthetic_range_list_consent() {
        let (sp, pc, pli) = base_optins_consent();
        let vendors = VendorConsent::RangeList(vec![
            VendorRange { start: 1, end: 1 },
            VendorRange { start: 5, end: 9 },
            VendorRange { start: 42, end: 42 },
        ]);
        let raw = build_core(
            17_000_000_000,
            17_100_000_000,
            120,
            7,
            2,
            "FR",
            200,
            4,
            false,
            true,
            sp,
            pc,
            pli,
            true,
            "DE",
            vendors.clone(),
        );

        let parsed = ConsentString::parse(&raw).expect("parse ok");
        assert_eq!(parsed.cmp_id, 120);
        assert_eq!(parsed.consent_language, "FR");
        assert_eq!(parsed.publisher_cc, "DE");
        assert!(parsed.purpose_one_treatment);
        assert_eq!(parsed.vendors_consent, vendors);

        // contains() checks on the range list.
        assert!(parsed.vendors_consent.contains(1));
        assert!(!parsed.vendors_consent.contains(2));
        assert!(parsed.vendors_consent.contains(5));
        assert!(parsed.vendors_consent.contains(9));
        assert!(parsed.vendors_consent.contains(42));
        assert!(!parsed.vendors_consent.contains(43));
    }

    #[test]
    fn parse_with_trailing_segment() {
        let (sp, pc, pli) = base_optins_consent();
        let vendors = VendorConsent::Bitfield(vec![true, true, false]);
        let core = build_core(
            15_000_000_000,
            15_000_000_000,
            1,
            1,
            0,
            "DE",
            99,
            2,
            false,
            false,
            sp,
            pc,
            pli,
            false,
            "DE",
            vendors.clone(),
        );
        let raw = format!("{}.IGNOREDSEGMENT", core);
        let parsed = ConsentString::parse(&raw).expect("parse ok");
        assert_eq!(parsed.version, 2);
        assert_eq!(parsed.consent_language, "DE");
        assert_eq!(parsed.vendors_consent, vendors);
        assert_eq!(parsed.raw(), raw);
    }

    #[test]
    fn vendor_bitfield_contains() {
        let vc = VendorConsent::Bitfield(vec![true, false, true, false]);
        assert!(vc.contains(1));
        assert!(!vc.contains(2));
        assert!(vc.contains(3));
        assert!(!vc.contains(4));
        assert!(!vc.contains(5));
        assert!(!vc.contains(0));
    }

    #[test]
    fn invalid_base64_is_malformed() {
        let err = ConsentString::parse("***not-base64***").unwrap_err();
        assert!(err.cause.contains("base64"), "cause was {}", err.cause);
    }

    #[test]
    fn truncated_core_is_malformed() {
        // Just two bytes - nowhere near enough to fit all fields.
        let short = URL_SAFE_NO_PAD.encode([0x08, 0x00]);
        let err = ConsentString::parse(&short).unwrap_err();
        assert!(
            err.cause.contains("past end")
                || err.cause.contains("created")
                || err.cause.contains("cmp_id"),
            "cause was {}",
            err.cause
        );
    }

    #[test]
    fn empty_string_is_malformed() {
        assert!(ConsentString::parse("").is_err());
    }

    #[test]
    fn backward_compat_constructor_keeps_raw() {
        // When parsing fails, `new` still preserves the raw string so old
        // call-sites that only stored it don't break.
        let c = ConsentString::new("***not-base64***");
        assert_eq!(c.raw(), "***not-base64***");
        assert!(c.version().is_ok()); // returns the stub version (0)
        assert_eq!(c.version, 0);
    }

    #[test]
    fn backward_compat_version_decode() {
        // A valid core segment round-trips through the legacy accessors.
        let (sp, pc, pli) = base_optins_consent();
        let raw = build_core(
            15_000_000_000,
            15_000_000_000,
            1,
            1,
            0,
            "EN",
            99,
            2,
            false,
            false,
            sp,
            pc,
            pli,
            false,
            "US",
            VendorConsent::Bitfield(vec![false, false]),
        );
        let c = ConsentString::new(raw.clone());
        assert_eq!(c.version().unwrap(), 2);
        assert_eq!(c.as_str(), raw);
    }
}
