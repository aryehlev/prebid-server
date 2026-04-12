//! Light-weight TCF consent-string wrapper.
//!
//! A full TCF2 decoder is out of scope for this crate - this type simply
//! holds the raw string and exposes enough parsing to identify the encoding
//! version so that callers can dispatch to the correct decoder.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};

use crate::MalformedConsentError;

/// A wrapper around a raw TCF consent string.
///
/// The TCF consent string is a base64url-encoded binary payload. The first
/// 6 bits of the payload encode the specification version (typically `2`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConsentString(String);

impl ConsentString {
    /// Creates a new `ConsentString` from the raw wire form. No validation
    /// is performed at construction time.
    pub fn new(raw: impl Into<String>) -> Self {
        ConsentString(raw.into())
    }

    /// Returns the raw consent string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the wrapper and returns the raw string.
    pub fn into_inner(self) -> String {
        self.0
    }

    /// Returns `true` if the wrapped consent string is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Attempts to decode the encoding version from the TCF payload.
    ///
    /// The version is encoded in the first 6 bits of the decoded payload.
    /// The "core" section of the consent string is the substring up to the
    /// first `.` separator (TCF2 appends optional sections after a `.`).
    pub fn version(&self) -> Result<u8, MalformedConsentError> {
        if self.0.is_empty() {
            return Err(MalformedConsentError {
                consent: self.0.clone(),
                cause: "consent string is empty".into(),
            });
        }

        // TCF2 allows appending optional sections after a '.' separator.
        let core = self.0.split('.').next().unwrap_or("");

        let decoded = URL_SAFE_NO_PAD
            .decode(core.as_bytes())
            .map_err(|e| MalformedConsentError {
                consent: self.0.clone(),
                cause: format!("base64 decode error: {}", e),
            })?;

        if decoded.is_empty() {
            return Err(MalformedConsentError {
                consent: self.0.clone(),
                cause: "decoded payload is empty".into(),
            });
        }

        // Version is the high 6 bits of the first byte.
        Ok(decoded[0] >> 2)
    }
}

impl From<String> for ConsentString {
    fn from(s: String) -> Self {
        ConsentString(s)
    }
}

impl From<&str> for ConsentString {
    fn from(s: &str) -> Self {
        ConsentString(s.to_string())
    }
}

impl AsRef<str> for ConsentString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_and_accessors() {
        let c = ConsentString::new("CPXxRfA");
        assert_eq!(c.as_str(), "CPXxRfA");
        assert!(!c.is_empty());
        assert_eq!(c.clone().into_inner(), "CPXxRfA");
    }

    #[test]
    fn empty_is_empty() {
        assert!(ConsentString::new("").is_empty());
        assert!(ConsentString::new("").version().is_err());
    }

    #[test]
    fn decodes_version_two() {
        // First byte 0b00001000 => version = 0b000010 = 2.
        // Base64url of a single 0x08 byte is "CA".
        let c = ConsentString::new("CA");
        assert_eq!(c.version().unwrap(), 2);
    }

    #[test]
    fn decodes_version_with_trailing_section() {
        // Same core, with an appended dot section that should be stripped.
        let c = ConsentString::new("CA.extra");
        assert_eq!(c.version().unwrap(), 2);
    }

    #[test]
    fn invalid_base64_is_malformed() {
        let c = ConsentString::new("***not-base64***");
        assert!(c.version().is_err());
    }
}
