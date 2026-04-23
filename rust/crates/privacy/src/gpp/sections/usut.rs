//! US Utah (UCPA) GPP section — core segment parser.
//!
//! Uses USNat as the baseline. Utah has 8 sensitive-data categories
//! and 1 known-child field. Utah has no "sharing" notion (so
//! sharing_notice / sharing_opt_out / sharing_opt_out_notice are
//! reported as 0 for uniform struct shape).
//!
//! Approximate layout (bit widths):
//!   version                              6
//!   sharing_notice                       2   (UT: reported 0)
//!   sale_opt_out_notice                  2
//!   targeted_advertising_opt_out_notice  2
//!   sensitive_data_processing_opt_notice 2
//!   sale_opt_out                         2
//!   targeted_advertising_opt_out         2
//!   sensitive_data_processing            8 x 2
//!   known_child_sensitive_data_consents  1 x 2
//!   mspa_covered_transaction             2
//!   mspa_opt_out_option_mode             2
//!   mspa_service_provider_mode           2

use crate::gpp::bitfield::BitReader;
use crate::gpp::GppError;

/// Parsed core-segment fields of a US-UT GPP section.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UsUtSection {
    pub version: u8,
    pub sharing_notice: u8,
    pub sale_opt_out_notice: u8,
    pub sharing_opt_out_notice: u8,
    pub targeted_advertising_opt_out_notice: u8,
    pub sale_opt_out: u8,
    pub sharing_opt_out: u8,
    pub targeted_advertising_opt_out: u8,
    /// 8 two-bit values in UT.
    pub sensitive_data_processing: Vec<u8>,
    /// 1 two-bit value in UT.
    pub known_child_sensitive_data_consents: Vec<u8>,
    pub personal_data_consents: u8,
    pub mspa_covered: u8,
    pub mspa_opt_out_option_mode: u8,
    pub mspa_service_provider_mode: u8,
}

const SENSITIVE_DATA_CATEGORIES: usize = 8;
const KNOWN_CHILD_FIELDS: usize = 1;

/// Parse the core segment of a US-UT GPP section from raw bytes.
pub fn parse(bytes: &[u8]) -> Result<UsUtSection, GppError> {
    if bytes.is_empty() {
        return Err(GppError::BadBitfield("empty UsUt payload".into()));
    }
    let mut r = BitReader::new(bytes);

    let version = r.read_u6()?;
    let sharing_notice = 0u8;
    let sale_opt_out_notice = r.read_bits(2)? as u8;
    let sharing_opt_out_notice = 0u8;
    let targeted_advertising_opt_out_notice = r.read_bits(2)? as u8;
    // sensitive_data_processing_opt_notice in spec — we skip to stay
    // uniform with other state structs.
    r.skip_bits(2)?;
    let sale_opt_out = r.read_bits(2)? as u8;
    let sharing_opt_out = 0u8;
    let targeted_advertising_opt_out = r.read_bits(2)? as u8;

    let mut sensitive_data_processing = Vec::with_capacity(SENSITIVE_DATA_CATEGORIES);
    for _ in 0..SENSITIVE_DATA_CATEGORIES {
        sensitive_data_processing.push(r.read_bits(2)? as u8);
    }

    let mut known_child_sensitive_data_consents = Vec::with_capacity(KNOWN_CHILD_FIELDS);
    for _ in 0..KNOWN_CHILD_FIELDS {
        known_child_sensitive_data_consents.push(r.read_bits(2)? as u8);
    }

    // UT has no personal_data_consents per spec; reported as 0 for parity.
    let personal_data_consents = 0u8;
    let mspa_covered = r.read_bits(2)? as u8;
    let mspa_opt_out_option_mode = r.read_bits(2)? as u8;
    let mspa_service_provider_mode = r.read_bits(2)? as u8;

    Ok(UsUtSection {
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
        // version (6) + (5 skipped+read + 8 sens + 1 kc + 3 mspa) two-bit fields
        // = 6 + 34 = 40 bits = 5 bytes.
        let mut fields: Vec<(u64, usize)> = vec![(1, 6)];
        // 5 read/skip + 8 sens + 1 kc + 3 mspa = 17 two-bit fields
        for _ in 0..17 {
            fields.push((0, 2));
        }
        let bytes = pack(&fields);
        let s = parse(&bytes).unwrap();
        assert_eq!(s.version, 1);
        assert_eq!(s.sensitive_data_processing.len(), 8);
        assert_eq!(s.known_child_sensitive_data_consents.len(), 1);
    }

    #[test]
    fn parse_known_values() {
        // version=5,
        // sale_opt_out_notice=1,
        // TA_notice=2,
        // (skipped sensitive_data_processing_opt_notice=3),
        // sale_opt_out=2,
        // TA_opt_out=1,
        // sensitive_data = [3,2,1,0,1,2,3,0],
        // known_child = [2],
        // mspa_covered=1, mspa_opt_mode=3, mspa_sp_mode=2
        let mut fields: Vec<(u64, usize)> = vec![
            (5, 6),
            (1, 2),
            (2, 2),
            (3, 2), // skipped
            (2, 2),
            (1, 2),
        ];
        for v in [3u64, 2, 1, 0, 1, 2, 3, 0] {
            fields.push((v, 2));
        }
        fields.push((2, 2));
        fields.push((1, 2));
        fields.push((3, 2));
        fields.push((2, 2));
        let bytes = pack(&fields);

        let s = parse(&bytes).unwrap();
        assert_eq!(s.version, 5);
        assert_eq!(s.sale_opt_out_notice, 1);
        assert_eq!(s.targeted_advertising_opt_out_notice, 2);
        assert_eq!(s.sale_opt_out, 2);
        assert_eq!(s.targeted_advertising_opt_out, 1);
        assert_eq!(
            s.sensitive_data_processing,
            vec![3, 2, 1, 0, 1, 2, 3, 0]
        );
        assert_eq!(s.known_child_sensitive_data_consents, vec![2]);
        assert_eq!(s.mspa_covered, 1);
        assert_eq!(s.mspa_opt_out_option_mode, 3);
        assert_eq!(s.mspa_service_provider_mode, 2);
    }

    #[test]
    fn empty_payload_errors() {
        assert!(parse(&[]).is_err());
    }
}
