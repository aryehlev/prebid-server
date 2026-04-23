//! US-Privacy v1 (CCPA) GPP section.
//!
//! The USP V1 payload is a 4-character ASCII string of the form
//! `VENY` where:
//! - char 0 = version (ASCII digit)
//! - char 1 = explicit notice ('Y'/'N'/'-')
//! - char 2 = opt-out-of-sale ('Y'/'N'/'-')
//! - char 3 = LSPA covered ('Y'/'N'/'-')
//!
//! This mirrors the legacy IAB US-Privacy 1.0 string format.

use crate::gpp::GppError;

/// Parsed US-Privacy v1 section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UspV1Section {
    pub version: u8,
    pub explicit_notice: char,
    pub opt_out_sale: char,
    pub lspa_covered: char,
}

/// Parse a US-Privacy v1 payload.
pub fn parse(payload: &str) -> Result<UspV1Section, GppError> {
    let bytes = payload.as_bytes();
    if bytes.len() != 4 {
        return Err(GppError::BadBitfield(format!(
            "USP V1 must be 4 chars, got {}",
            bytes.len()
        )));
    }
    let version = (bytes[0] as char)
        .to_digit(10)
        .ok_or_else(|| GppError::BadBitfield("USP V1 version not a digit".into()))?
        as u8;
    Ok(UspV1Section {
        version,
        explicit_notice: bytes[1] as char,
        opt_out_sale: bytes[2] as char,
        lspa_covered: bytes[3] as char,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_known_string() {
        let s = parse("1YNN").unwrap();
        assert_eq!(s.version, 1);
        assert_eq!(s.explicit_notice, 'Y');
        assert_eq!(s.opt_out_sale, 'N');
        assert_eq!(s.lspa_covered, 'N');
    }

    #[test]
    fn rejects_bad_length() {
        assert!(parse("1YN").is_err());
        assert!(parse("1YNNN").is_err());
    }
}
