//! Prebid-specific extensions for `App.ext`.
//!
//! Mirrors `openrtb_ext/app.go` (`ExtApp`, `ExtAppPrebid`).

use serde::{Deserialize, Serialize};

/// `ExtApp` — `app.ext`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtApp {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prebid: Option<ExtAppPrebid>,
}

/// `ExtAppPrebid` — `app.ext.prebid`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtAppPrebid {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ext_app() {
        let a = ExtApp {
            prebid: Some(ExtAppPrebid {
                source: Some("prebid-mobile".into()),
                version: Some("1.2.3".into()),
            }),
        };
        let s = serde_json::to_string(&a).unwrap();
        let parsed: ExtApp = serde_json::from_str(&s).unwrap();
        assert_eq!(a, parsed);
    }
}
