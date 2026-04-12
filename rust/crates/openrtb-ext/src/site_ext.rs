//! Prebid-specific extensions for `Site.ext`.
//!
//! Mirrors `openrtb_ext/site.go` (`ExtSite`).

use serde::{Deserialize, Serialize};

/// `ExtSite` — `site.ext`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtSite {
    /// 1 if the request comes from an AMP page, 0 if not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amp: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ext_site() {
        let s = ExtSite { amp: Some(1) };
        let j = serde_json::to_string(&s).unwrap();
        let parsed: ExtSite = serde_json::from_str(&j).unwrap();
        assert_eq!(s, parsed);
    }
}
