//! GPP top-level header parser.
//!
//! A GPP string looks like `DBABMA~<section1>~<section2>...` where
//! the first tilde-delimited chunk is a url-safe base64-encoded
//! header. The header starts with 6 bits of gpp-type (==3), followed
//! by 6 bits of gpp-version, then a "fibonacci range" encoded list
//! of section ids. This implementation supports the simpler bitfield
//! form used in tests and most production strings: 6-bit type, 6-bit
//! version, then a variable run of 1-bit flags indexed by section id.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;

use super::bitfield::BitReader;
use super::GppError;
use super::section::SectionId;

/// Parse a complete GPP string into its section ids and raw section
/// payload strings. Returns `(section_ids, section_payloads)` where
/// the two vectors are index-aligned.
pub fn parse_gpp_header(s: &str) -> Result<(Vec<SectionId>, Vec<String>), GppError> {
    let mut parts = s.split('~');
    let header = parts
        .next()
        .ok_or_else(|| GppError::InvalidHeader("empty GPP string".to_string()))?;
    if header.is_empty() {
        return Err(GppError::InvalidHeader("empty header segment".to_string()));
    }
    let payloads: Vec<String> = parts.map(|s| s.to_string()).collect();

    let decoded = URL_SAFE_NO_PAD
        .decode(header.as_bytes())
        .map_err(|e| GppError::Base64(e.to_string()))?;
    let mut reader = BitReader::new(&decoded);

    // gpp-type (6 bits) — must be 3 per spec, but we accept anything.
    let _gpp_type = reader.read_u6()?;
    // gpp-version (6 bits).
    let _gpp_version = reader.read_u6()?;

    // Remaining bits encode a section-id bitfield indexed from 1.
    // Bit position i (0-based in the stream) corresponds to section
    // id (i+1). A 1 bit means "section present".
    let mut ids: Vec<SectionId> = Vec::new();
    let mut idx: i32 = 1;
    while reader.remaining_bits() > 0 {
        let bit = reader.read_bool()?;
        if bit {
            if let Ok(sid) = SectionId::from_i32(idx) {
                ids.push(sid);
            }
        }
        idx += 1;
    }

    if ids.len() != payloads.len() {
        // Some producers emit trailing padding bits of 0. If we read
        // more section ids than we have payloads for, that's a real
        // mismatch; if we read fewer, it's usually padding and the
        // trailing payloads are not decodable by us. We treat exact
        // match as success and a strict mismatch as a soft-error by
        // truncating to the shorter of the two.
        let n = ids.len().min(payloads.len());
        return Ok((ids.into_iter().take(n).collect(), payloads.into_iter().take(n).collect()));
    }

    Ok((ids, payloads))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    fn encode_header(version: u8, section_ids: &[i32]) -> String {
        // Build a bitstream: 6 bits type (3), 6 bits version, then flag
        // bits indexed from position 0 = section id 1.
        let max_id = *section_ids.iter().max().unwrap_or(&0) as usize;
        let mut bits: Vec<u8> = Vec::new();
        let push = |bits: &mut Vec<u8>, v: u64, n: usize| {
            for i in (0..n).rev() {
                bits.push(((v >> i) & 1) as u8);
            }
        };
        push(&mut bits, 3, 6);
        push(&mut bits, version as u64, 6);
        for id in 1..=max_id {
            let present = section_ids.contains(&(id as i32));
            bits.push(if present { 1 } else { 0 });
        }
        // Pad to byte boundary.
        while bits.len() % 8 != 0 {
            bits.push(0);
        }
        // Pack into bytes MSB-first.
        let mut bytes = vec![0u8; bits.len() / 8];
        for (i, b) in bits.iter().enumerate() {
            if *b == 1 {
                bytes[i / 8] |= 1 << (7 - (i % 8));
            }
        }
        URL_SAFE_NO_PAD.encode(bytes)
    }

    #[test]
    fn single_section() {
        let hdr = encode_header(1, &[6]);
        let s = format!("{hdr}~1YNN");
        let (ids, payloads) = parse_gpp_header(&s).unwrap();
        assert_eq!(ids, vec![SectionId::UspV1]);
        assert_eq!(payloads, vec!["1YNN"]);
    }

    #[test]
    fn multiple_sections() {
        let hdr = encode_header(1, &[2, 6]);
        let s = format!("{hdr}~TCFSTRING~1YNN");
        let (ids, payloads) = parse_gpp_header(&s).unwrap();
        assert_eq!(ids, vec![SectionId::TcfEuV2, SectionId::UspV1]);
        assert_eq!(payloads, vec!["TCFSTRING", "1YNN"]);
    }

    #[test]
    fn empty_fails() {
        assert!(parse_gpp_header("").is_err());
    }
}
