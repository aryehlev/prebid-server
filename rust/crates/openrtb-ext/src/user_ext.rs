//! Prebid-specific extensions for `User.ext`.
//!
//! Mirrors `openrtb_ext/user.go` (`ExtUser`, `ExtUserPrebid`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `ExtUser` — `user.ext`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtUser {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prebid: Option<ExtUserPrebid>,
    /// Raw openrtb2.EID list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eids: Option<Vec<ExtUserEid>>,
}

/// `ExtUserPrebid` — `user.ext.prebid`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtUserPrebid {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buyeruids: Option<HashMap<String, String>>,
}

/// A single entry in `user.ext.eids`.
///
/// Modeled loosely on `openrtb2.EID` so the crate does not depend on the
/// upstream `openrtb` crate. Unknown fields are preserved via the
/// `source`-keyed `uids` slice and the catch-all `ext` field.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExtUserEid {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uids: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ext_user() {
        let user = ExtUser {
            consent: Some("BONV8oUONV8oUAfZ".into()),
            prebid: Some(ExtUserPrebid {
                buyeruids: Some(HashMap::from([("appnexus".to_string(), "uid-1".to_string())])),
            }),
            eids: Some(vec![ExtUserEid {
                source: Some("liveramp.com".into()),
                uids: Some(vec![serde_json::json!({"id": "abc"})]),
                ext: None,
            }]),
        };
        let s = serde_json::to_string(&user).unwrap();
        let parsed: ExtUser = serde_json::from_str(&s).unwrap();
        assert_eq!(user, parsed);
    }
}
