//! GPP section identifier registry.
//!
//! Mirrors the Go `gpp.SectionID` constants from the upstream IAB
//! `iabgpp-es` / go-gpp projects.

use super::GppError;

/// Well-known GPP section identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum SectionId {
    TcfEuV2 = 2,
    UspV1 = 6,
    UsNatV1 = 7,
    UsCaV1 = 8,
    UsVaV1 = 9,
    UsCoV1 = 10,
    UsUtV1 = 11,
    UsCtV1 = 12,
}

impl SectionId {
    /// Convert a raw i32 section id to a known [`SectionId`].
    pub fn from_i32(v: i32) -> Result<Self, GppError> {
        match v {
            2 => Ok(SectionId::TcfEuV2),
            6 => Ok(SectionId::UspV1),
            7 => Ok(SectionId::UsNatV1),
            8 => Ok(SectionId::UsCaV1),
            9 => Ok(SectionId::UsVaV1),
            10 => Ok(SectionId::UsCoV1),
            11 => Ok(SectionId::UsUtV1),
            12 => Ok(SectionId::UsCtV1),
            other => Err(GppError::UnknownSection(other)),
        }
    }

    /// Return the integer id.
    pub fn as_i32(self) -> i32 {
        self as i32
    }

    /// Return the canonical short name for this section.
    pub fn name(self) -> &'static str {
        match self {
            SectionId::TcfEuV2 => "tcfeuv2",
            SectionId::UspV1 => "uspv1",
            SectionId::UsNatV1 => "usnatv1",
            SectionId::UsCaV1 => "uscav1",
            SectionId::UsVaV1 => "usvav1",
            SectionId::UsCoV1 => "uscov1",
            SectionId::UsUtV1 => "usutv1",
            SectionId::UsCtV1 => "usctv1",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for id in [
            SectionId::TcfEuV2,
            SectionId::UspV1,
            SectionId::UsNatV1,
            SectionId::UsCaV1,
            SectionId::UsVaV1,
            SectionId::UsCoV1,
            SectionId::UsUtV1,
            SectionId::UsCtV1,
        ] {
            assert_eq!(SectionId::from_i32(id.as_i32()).unwrap(), id);
            assert!(!id.name().is_empty());
        }
    }

    #[test]
    fn unknown() {
        assert!(SectionId::from_i32(99).is_err());
    }
}
