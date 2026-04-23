//! US California (CCPA/CPRA) GPP section — core segment parser.
//!
//! Based on IAB GPP US-CA section. Uses USNat as the baseline for
//! field ordering — where California diverges in the real spec
//! (e.g. different sensitive-data category counts) the differences
//! are noted in comments below.
//!
//! Approximate core-segment layout (bit widths):
//!   version                             6
//!   sale_opt_out_notice                 2
//!   sharing_opt_out_notice              2
//!   sensitive_data_limit_use_notice     2
//!   sale_opt_out                        2
//!   sharing_opt_out                     2
//!   sensitive_data_processing           9 x 2  (CA has 9 categories)
//!   known_child_sensitive_data_consents 2 x 2
//!   personal_data_consents              2
//!   mspa_covered_transaction            2
//!   mspa_opt_out_option_mode            2
//!   mspa_service_provider_mode          2
//!
//! To keep the struct uniform across states, we also expose
//! `sharing_notice` and `targeted_advertising_*` fields; for CA they
//! are reported as 0 (spec: not applicable).

use crate::gpp::bitfield::BitReader;
use crate::gpp::GppError;

/// Parsed core-segment fields of a US-CA GPP section.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UsCaSection {
    pub version: u8,
    pub sharing_notice: u8,
    pub sale_opt_out_notice: u8,
    pub sharing_opt_out_notice: u8,
    pub targeted_advertising_opt_out_notice: u8,
    pub sale_opt_out: u8,
    pub sharing_opt_out: u8,
    pub targeted_advertising_opt_out: u8,
    /// 9 two-bit values in the CA spec.
    pub sensitive_data_processing: Vec<u8>,
    /// 2 two-bit values in the CA spec.
    pub known_child_sensitive_data_consents: Vec<u8>,
    pub personal_data_consents: u8,
    pub mspa_covered: u8,
    pub mspa_opt_out_option_mode: u8,
    pub mspa_service_provider_mode: u8,
}

const SENSITIVE_DATA_CATEGORIES: usize = 9;
const KNOWN_CHILD_FIELDS: usize = 2;

/// Parse the core segment of a US-CA GPP section from raw bytes.
pub fn parse(bytes: &[u8]) -> Result<UsCaSection, GppError> {
    if bytes.is_empty() {
        return Err(GppError::BadBitfield("empty UsCa payload".into()));
    }
    let mut r = BitReader::new(bytes);

    let version = r.read_u6()?;
    // CA has no dedicated "sharing notice" — we report 0 for schema parity.
    let sharing_notice = 0u8;
    let sale_opt_out_notice = r.read_bits(2)? as u8;
    let sharing_opt_out_notice = r.read_bits(2)? as u8;
    // CA uses "sensitive data limit use notice" in place of TA-opt-out-notice.
    let targeted_advertising_opt_out_notice = r.read_bits(2)? as u8;
    let sale_opt_out = r.read_bits(2)? as u8;
    let sharing_opt_out = r.read_bits(2)? as u8;
    // CA spec has no targeted-advertising opt-out; reported as 0.
    let targeted_advertising_opt_out = 0u8;

    let mut sensitive_data_processing = Vec::with_capacity(SENSITIVE_DATA_CATEGORIES);
    for _ in 0..SENSITIVE_DATA_CATEGORIES {
        sensitive_data_processing.push(r.read_bits(2)? as u8);
    }

    let mut known_child_sensitive_data_consents = Vec::with_capacity(KNOWN_CHILD_FIELDS);
    for _ in 0..KNOWN_CHILD_FIELDS {
        known_child_sensitive_data_consents.push(r.read_bits(2)? as u8);
    }

    let personal_data_consents = r.read_bits(2)? as u8;
    let mspa_covered = r.read_bits(2)? as u8;
    let mspa_opt_out_option_mode = r.read_bits(2)? as u8;
    let mspa_service_provider_mode = r.read_bits(2)? as u8;

    Ok(UsCaSection {
        version,
        sharing_notice,
        sale_opt_out_notice,
        sharing_opt_out_notice,
        targeted_advertising_opt_out_notice,
        sale_opt_out,
        sharing_opt_out,
        targeted_advertising_opt_out,
        sensitive_data_processing,
        known_child_sensitive_data_consents,
        personal_data_consents,
        mspa_covered,
        mspa_opt_out_option_mode,
        mspa_service_provider_mode,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a byte vector by packing a sequence of (value, bit_width)
    /// into big-endian order — MSB of byte 0 first.
    fn pack(fields: &[(u64, usize)]) -> Vec<u8> {
        let total_bits: usize = fields.iter().map(|(_, w)| *w).sum();
        let n_bytes = total_bits.div_ceil(8);
        let mut out = vec![0u8; n_bytes];
        let mut pos = 0usize;
        for (value, width) in fields {
            for i in 0..*width {
                let bit = ((value >> (width - 1 - i)) & 1) as u8;
                if bit == 1 {
                    let byte_idx = pos / 8;
                    let bit_in_byte = 7 - (pos % 8);
                    out[byte_idx] |= 1 << bit_in_byte;
                }
                pos += 1;
            }
        }
        out
    }

    #[test]
    fn parse_all_zero_payload() {
        // version=1, and every following field = 0.
        // Fields after version (all 2 bits):
        //   sale_opt_out_notice, sharing_opt_out_notice,
        //   sensitive_data_limit_use_notice (=>TA_notice),
        //   sale_opt_out, sharing_opt_out,
        //   9 sensitive_data, 2 known_child,
        //   personal_data_consents, mspa_covered,
        //   mspa_opt_out_option_mode, mspa_service_provider_mode
        //   => 5 + 9 + 2 + 4 = 20 two-bit fields -> 40 bits
        // Total = 6 + 40 = 46 bits -> 6 bytes.
        let mut fields: Vec<(u64, usize)> = vec![(1, 6)];
        for _ in 0..20 {
            fields.push((0, 2));
        }
        let bytes = pack(&fields);
        let s = parse(&bytes).unwrap();
        assert_eq!(s.version, 1);
        assert_eq!(s.sensitive_data_processing.len(), 9);
        assert!(s.sensitive_data_processing.iter().all(|v| *v == 0));
        assert_eq!(s.known_child_sensitive_data_consents.len(), 2);
        assert_eq!(s.mspa_covered, 0);
    }

    #[test]
    fn parse_known_values() {
        // version=3, sale_opt_out_notice=1, sharing_opt_out_notice=2,
        // TA_notice=3, sale_opt_out=1, sharing_opt_out=2,
        // 9 sensitive_data = [1,2,3,0,1,2,3,0,1],
        // 2 known_child = [2,3],
        // personal_data_consents=1, mspa_covered=2,
        // mspa_opt_out_option_mode=3, mspa_service_provider_mode=1
        let mut fields: Vec<(u64, usize)> = vec![
            (3, 6),
            (1, 2),
            (2, 2),
            (3, 2),
            (1, 2),
            (2, 2),
        ];
        for v in [1u64, 2, 3, 0, 1, 2, 3, 0, 1] {
            fields.push((v, 2));
        }
        for v in [2u64, 3] {
            fields.push((v, 2));
        }
        fields.push((1, 2));
        fields.push((2, 2));
        fields.push((3, 2));
        fields.push((1, 2));
        let bytes = pack(&fields);

        let s = parse(&bytes).unwrap();
        assert_eq!(s.version, 3);
        assert_eq!(s.sale_opt_out_notice, 1);
        assert_eq!(s.sharing_opt_out_notice, 2);
        assert_eq!(s.targeted_advertising_opt_out_notice, 3);
        assert_eq!(s.sale_opt_out, 1);
        assert_eq!(s.sharing_opt_out, 2);
        assert_eq!(
            s.sensitive_data_processing,
            vec![1, 2, 3, 0, 1, 2, 3, 0, 1]
        );
        assert_eq!(s.known_child_sensitive_data_consents, vec![2, 3]);
        assert_eq!(s.personal_data_consents, 1);
        assert_eq!(s.mspa_covered, 2);
        assert_eq!(s.mspa_opt_out_option_mode, 3);
        assert_eq!(s.mspa_service_provider_mode, 1);
    }

    #[test]
    fn empty_payload_errors() {
        assert!(parse(&[]).is_err());
    }

    #[test]
    fn truncated_payload_errors() {
        // Only version + a couple of fields — way short.
        let bytes = [0x04, 0x00];
        assert!(parse(&bytes).is_err());
    }
}
