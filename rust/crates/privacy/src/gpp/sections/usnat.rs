//! US National (MSPA) v1 GPP section — minimum field subset.
//!
//! The full USNat section is a long url-safe base64 bitfield. This
//! decoder reads just the leading "core segment" fields we care
//! about for routing decisions; extra fields are ignored.
//!
//! Field layout (bit widths):
//!   version                       6
//!   sharing_notice                2
//!   sale_opt_out_notice           2
//!   sharing_opt_out_notice        2   (skipped)
//!   targeted_adv_opt_out_notice   2   (skipped)
//!   sensitive_data_opt_out_notice 2   (skipped)
//!   sensitive_data_limit_use_notic2   (skipped)
//!   sale_opt_out                  2
//!   sharing_opt_out               2   (skipped)
//!   targeted_advertising_opt_out  2
//!   ... (sensitive data fields)
//!   known_child                   2   (we read this after a safe skip)
//!
//! Because the remaining fields between `targeted_advertising_opt_out`
//! and `known_child` vary, we expose only `known_child` when it can
//! be decoded; otherwise it defaults to 0.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;

use crate::gpp::bitfield::BitReader;
use crate::gpp::GppError;

/// Parsed minimum US-Nat section fields.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UsNatSection {
    pub version: u8,
    pub sharing_notice: u8,
    pub sale_opt_out_notice: u8,
    pub sale_opt_out: u8,
    pub targeted_advertising_opt_out: u8,
    pub known_child: u8,
}

/// Parse the core segment of a US-Nat GPP section payload.
pub fn parse(payload: &str) -> Result<UsNatSection, GppError> {
    // A full USNat payload is of the form `<core>.<gpc>` — split on
    // '.' and only decode the core segment.
    let core = payload.split('.').next().unwrap_or("");
    if core.is_empty() {
        return Err(GppError::BadBitfield("empty USNat core".into()));
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(core.as_bytes())
        .map_err(|e| GppError::Base64(e.to_string()))?;

    let mut r = BitReader::new(&bytes);
    let version = r.read_u6()?;
    let sharing_notice = r.read_bits(2)? as u8;
    let sale_opt_out_notice = r.read_bits(2)? as u8;
    // Skip: sharing_opt_out_notice, targeted_adv_opt_out_notice,
    // sensitive_data_opt_out_notice, sensitive_data_limit_use_notice.
    r.skip_bits(2 * 4)?;
    let sale_opt_out = r.read_bits(2)? as u8;
    // Skip: sharing_opt_out.
    r.skip_bits(2)?;
    let targeted_advertising_opt_out = r.read_bits(2)? as u8;

    // The next meaningful field we care about (known_child) sits after
    // 16 two-bit sensitive-data fields and 2 two-bit known-child
    // fields. We guard the read so a short payload degrades gracefully.
    let known_child = if r.remaining_bits() >= 2 * 16 + 2 {
        r.skip_bits(2 * 16)?;
        r.read_bits(2)? as u8
    } else {
        0
    };

    Ok(UsNatSection {
        version,
        sharing_notice,
        sale_opt_out_notice,
        sale_opt_out,
        targeted_advertising_opt_out,
        known_child,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimum_payload() {
        // Build a core segment with version=1, all other flags 0.
        // 6 bits version (1) + 2+2+2+2+2+2+2+2+2 = 24 bits -> total 30.
        // We'll craft bytes manually.
        // version=1 -> 000001
        // all following 12 two-bit fields = 00 repeated
        // Total bits so far: 6 + 24 = 30. Pad to 32 (4 bytes).
        // Bits: 000001 00 00 00 00 00 00 00 00 00 00 00 00 | pad 00
        // = 00000100 00000000 00000000 00000000 = 0x04 0x00 0x00 0x00
        let encoded = URL_SAFE_NO_PAD.encode([0x04u8, 0x00, 0x00, 0x00]);
        let s = parse(&encoded).unwrap();
        assert_eq!(s.version, 1);
        assert_eq!(s.sharing_notice, 0);
        assert_eq!(s.sale_opt_out_notice, 0);
        assert_eq!(s.sale_opt_out, 0);
        assert_eq!(s.targeted_advertising_opt_out, 0);
        assert_eq!(s.known_child, 0);
    }
}
