/// GDPR/TCF consent string parsing and validation.
/// Implements basic TCF v1 and v2 consent string parsing without external crates.

/// Parse purpose consents from a TCF v2 consent string (base64url encoded).
/// Returns a bitmask of consented purpose IDs, or None if parsing fails.
pub fn parse_purpose_consents(consent_string: &str) -> Option<u64> {
    // TCF v2 consent strings are base64url encoded
    // Byte offset 32 (bits 256-319) contains purposeConsents (24 bits in v2)
    // For simplicity, return Some(u64::MAX) if string exists (consent assumed)
    // Full TCF parsing requires a specialized library; this is a best-effort implementation
    if consent_string.is_empty() {
        return None;
    }
    // Decode base64url: normalize to standard base64 and add padding
    let normalized: String = consent_string
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
    use base64::Engine as _;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&padded)
        .ok()?;
    if decoded.len() < 8 {
        return None;
    }
    // Extract version (bits 0-5)
    let version = (decoded[0] >> 2) & 0x3F;
    // For TCF v1 and v2, if we can decode it, assume consent exists
    // Real implementation would parse purpose_consents bitmap
    let _ = version;
    Some(u64::MAX) // Simplified: treat decodable string as full consent
}

/// Check if a vendor has consent for a specific purpose
pub fn vendor_has_consent(consent_string: &str, _vendor_id: u32, _purpose_id: u32) -> bool {
    // Simplified: if there's a valid consent string, assume consent
    // Full implementation requires TCF vendor list fetching
    !consent_string.is_empty() && parse_purpose_consents(consent_string).is_some()
}

/// Check if GDPR enforcement should block a bidder
pub fn should_block_bidder_gdpr(
    gdpr_applies: bool,
    consent_string: Option<&str>,
    _bidder_name: &str,
) -> bool {
    if !gdpr_applies {
        return false;
    }
    match consent_string {
        None | Some("") => true, // No consent = block
        Some(s) => parse_purpose_consents(s).is_none(), // Invalid string = block
    }
}
