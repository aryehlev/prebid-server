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
    UsFlV1 = 13,
    UsMtV1 = 14,
    UsOrV1 = 15,
    UsTxV1 = 16,
    UsDeV1 = 17,
    UsIaV1 = 18,
    UsNeV1 = 19,
    UsNhV1 = 20,
    UsNjV1 = 21,
    UsTnV1 = 22,
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
            13 => Ok(SectionId::UsFlV1),
            14 => Ok(SectionId::UsMtV1),
            15 => Ok(SectionId::UsOrV1),
            16 => Ok(SectionId::UsTxV1),
            17 => Ok(SectionId::UsDeV1),
            18 => Ok(SectionId::UsIaV1),
            19 => Ok(SectionId::UsNeV1),
            20 => Ok(SectionId::UsNhV1),
            21 => Ok(SectionId::UsNjV1),
            22 => Ok(SectionId::UsTnV1),
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
            SectionId::UsFlV1 => "usflv1",
            SectionId::UsMtV1 => "usmtv1",
            SectionId::UsOrV1 => "usorv1",
            SectionId::UsTxV1 => "ustxv1",
            SectionId::UsDeV1 => "usdev1",
            SectionId::UsIaV1 => "usiav1",
            SectionId::UsNeV1 => "usnev1",
            SectionId::UsNhV1 => "usnhv1",
            SectionId::UsNjV1 => "usnjv1",
            SectionId::UsTnV1 => "ustnv1",
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
