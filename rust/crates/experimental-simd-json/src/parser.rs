//! Fast OpenRTB request parser built on top of `simd-json`.
//!
//! This module is experimental. The goal is to demonstrate how a SIMD JSON
//! parser can be integrated into the Prebid Server pipeline without adopting
//! it for real traffic. All extraction helpers walk an [`OwnedValue`] in a
//! defensive manner so that malformed payloads simply yield empty results.

use simd_json::prelude::*;
use simd_json::OwnedValue;

use crate::error::ParseError;

/// Zero-sized namespace struct grouping the fast parser entry points.
///
/// Using a unit struct rather than a module keeps the public surface closer
/// to the rest of the Rust crates which often expose parser "types".
pub struct FastJsonParser;

impl FastJsonParser {
    /// Parse an OpenRTB JSON payload into a [`simd_json::OwnedValue`].
    ///
    /// Note: `simd-json` mutates the input buffer while parsing, so the
    /// caller must provide a mutable slice they are willing to have
    /// clobbered.
    pub fn parse_request_owned(bytes: &mut [u8]) -> Result<OwnedValue, ParseError> {
        let value = simd_json::to_owned_value(bytes)?;
        if !value.is_object() {
            return Err(ParseError::NotAnObject);
        }
        Ok(value)
    }
}

/// Free-function alias for [`FastJsonParser::parse_request_owned`].
pub fn parse_request_owned(bytes: &mut [u8]) -> Result<OwnedValue, ParseError> {
    FastJsonParser::parse_request_owned(bytes)
}

/// Extract the `id` of every entry in `request.imp[]`, in order.
///
/// Missing fields, wrong types, and absent `imp` arrays all yield an empty
/// vector rather than an error — this mirrors the behaviour most downstream
/// call sites want for best-effort extraction.
pub fn extract_imp_ids(value: &OwnedValue) -> Vec<String> {
    let Some(imps) = value.get("imp").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(imps.len());
    for imp in imps {
        if let Some(id) = imp.get("id").and_then(|v| v.as_str()) {
            out.push(id.to_string());
        }
    }
    out
}

/// Extract the `tmax` field as an `i64`, if present and numeric.
pub fn extract_tmax(value: &OwnedValue) -> Option<i64> {
    value.get("tmax").and_then(|v| v.as_i64())
}

/// Resolve the account id by walking the standard Prebid fallback chain:
///
/// 1. `ext.prebid.storedrequest.id`
/// 2. `site.publisher.id`
/// 3. `app.publisher.id`
pub fn extract_account_id(value: &OwnedValue) -> Option<String> {
    if let Some(id) = value
        .get("ext")
        .and_then(|v| v.get("prebid"))
        .and_then(|v| v.get("storedrequest"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
    {
        return Some(id.to_string());
    }
    if let Some(id) = value
        .get("site")
        .and_then(|v| v.get("publisher"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
    {
        return Some(id.to_string());
    }
    if let Some(id) = value
        .get("app")
        .and_then(|v| v.get("publisher"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
    {
        return Some(id.to_string());
    }
    None
}

/// Collect the set of bidder codes referenced under every
/// `imp[].ext.prebid.bidder` object. Duplicates across impressions are
/// preserved so callers can observe the frequency if they care.
pub fn extract_bidders_from_imp(value: &OwnedValue) -> Vec<String> {
    let Some(imps) = value.get("imp").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for imp in imps {
        let Some(bidder) = imp
            .get("ext")
            .and_then(|v| v.get("prebid"))
            .and_then(|v| v.get("bidder"))
        else {
            continue;
        };
        let Some(obj) = bidder.as_object() else {
            continue;
        };
        for (k, _) in obj.iter() {
            out.push(k.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const CANNED: &str = r#"{
        "id": "req-1",
        "tmax": 250,
        "imp": [
            {
                "id": "imp-1",
                "ext": {
                    "prebid": {
                        "bidder": {
                            "appnexus": {"placementId": 1},
                            "rubicon": {"accountId": 2}
                        }
                    }
                }
            },
            {
                "id": "imp-2",
                "ext": {
                    "prebid": {
                        "bidder": {
                            "openx": {"unit": "x"}
                        }
                    }
                }
            }
        ],
        "site": {
            "publisher": {"id": "site-pub"}
        },
        "app": {
            "publisher": {"id": "app-pub"}
        },
        "ext": {
            "prebid": {
                "storedrequest": {"id": "stored-1"}
            }
        }
    }"#;

    fn parse(s: &str) -> OwnedValue {
        let mut bytes = s.as_bytes().to_vec();
        parse_request_owned(&mut bytes).expect("valid json")
    }

    #[test]
    fn parses_canned_request() {
        let v = parse(CANNED);
        assert_eq!(v.get("id").and_then(|x| x.as_str()), Some("req-1"));
        assert_eq!(extract_tmax(&v), Some(250));
    }

    #[test]
    fn extracts_imp_ids() {
        let v = parse(CANNED);
        assert_eq!(extract_imp_ids(&v), vec!["imp-1".to_string(), "imp-2".to_string()]);
    }

    #[test]
    fn extracts_bidders() {
        let v = parse(CANNED);
        let mut bidders = extract_bidders_from_imp(&v);
        bidders.sort();
        assert_eq!(
            bidders,
            vec![
                "appnexus".to_string(),
                "openx".to_string(),
                "rubicon".to_string(),
            ]
        );
    }

    #[test]
    fn account_id_prefers_stored_request() {
        let v = parse(CANNED);
        assert_eq!(extract_account_id(&v).as_deref(), Some("stored-1"));
    }

    #[test]
    fn account_id_falls_back_to_site_publisher() {
        let s = r#"{"imp":[],"site":{"publisher":{"id":"site-pub"}}}"#;
        let v = parse(s);
        assert_eq!(extract_account_id(&v).as_deref(), Some("site-pub"));
    }

    #[test]
    fn account_id_falls_back_to_app_publisher() {
        let s = r#"{"imp":[],"app":{"publisher":{"id":"app-pub"}}}"#;
        let v = parse(s);
        assert_eq!(extract_account_id(&v).as_deref(), Some("app-pub"));
    }

    #[test]
    fn account_id_absent_is_none() {
        let s = r#"{"imp":[]}"#;
        let v = parse(s);
        assert_eq!(extract_account_id(&v), None);
    }

    #[test]
    fn top_level_must_be_object() {
        let mut bytes = b"[]".to_vec();
        let err = parse_request_owned(&mut bytes).unwrap_err();
        assert!(matches!(err, ParseError::NotAnObject));
    }
}
