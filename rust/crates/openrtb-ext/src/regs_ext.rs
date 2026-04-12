//! Prebid-specific extensions for `Regs.ext`.
//!
//! Mirrors `openrtb_ext/regs.go` (`ExtRegs`, `ExtRegsDSA`,
//! `ExtBidDSATransparency`). Also exposes a minimal `ExtRegsGpp` wrapper
//! for the `gpp` / `gpp_sid` fields commonly placed in `regs.ext`.

use serde::{Deserialize, Serialize};

/// `ExtRegs` — `regs.ext`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtRegs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dsa: Option<ExtRegsDsa>,
    /// "1" if GDPR applies, "0" if not, undefined if unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gdpr: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub us_privacy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpp: Option<ExtRegsGpp>,
}

/// `ExtRegsDSA` — `regs.ext.dsa`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtRegsDsa {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dsarequired: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pubrender: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datatopub: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transparency: Option<Vec<ExtBidDsaTransparency>>,
}

/// `ExtBidDSATransparency` — a single transparency entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtBidDsaTransparency {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dsaparams: Option<Vec<i64>>,
}

/// `ExtRegsGpp` — holder for Global Privacy Platform signal + section ids.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtRegsGpp {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpp_sid: Option<Vec<i64>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ext_regs() {
        let r = ExtRegs {
            dsa: Some(ExtRegsDsa {
                dsarequired: Some(1),
                pubrender: Some(0),
                datatopub: Some(1),
                transparency: Some(vec![ExtBidDsaTransparency {
                    domain: Some("example.com".into()),
                    dsaparams: Some(vec![1, 2, 3]),
                }]),
            }),
            gdpr: Some(1),
            us_privacy: Some("1YNN".into()),
            gpp: Some(ExtRegsGpp {
                gpp: Some("DBABMA~CPXxRfAPXxRfAAfKABENB-CgAAAAAAAAAAYgAAAAAAAA".into()),
                gpp_sid: Some(vec![6]),
            }),
        };
        let s = serde_json::to_string(&r).unwrap();
        let parsed: ExtRegs = serde_json::from_str(&s).unwrap();
        assert_eq!(r, parsed);
    }
}
