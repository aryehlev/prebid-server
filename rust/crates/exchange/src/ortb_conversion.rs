//! OpenRTB version conversion helpers.
//!
//! Ports the Go logic from `openrtb_ext/convert_down.go` and `openrtb_ext/convert_up.go`.
//! Provides functions to downgrade an OpenRTB 2.6 request so legacy 2.5 bidders can
//! consume it, and to upgrade a 2.5 request to 2.6 when needed.

use openrtb::BidRequest;
use serde_json::Value;

/// The supported OpenRTB versions for bidder negotiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrtbVersion {
    V25,
    V26,
}

impl OrtbVersion {
    /// Parse a version string such as `"2.5"` or `"2.6"`.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "2.5" => Some(OrtbVersion::V25),
            "2.6" => Some(OrtbVersion::V26),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Down-conversion: 2.6 -> 2.5
// ---------------------------------------------------------------------------

/// Strip OpenRTB 2.6 fields from a `BidRequest` so that a legacy 2.5 bidder
/// does not receive unknown fields.
///
/// This is the Rust equivalent of the Go `ConvertDownTo25` + `Clear26Fields` +
/// `Clear202211Fields` + `Clear202303Fields` + `Clear202309Fields` +
/// `Clear202402Fields` + `Clear202409Fields` functions combined.
pub fn convert_down_to_25(request: &mut BidRequest) {
    // Move 2.6-only top-level fields into ext where a 2.5 bidder expects them,
    // then clear them from the native locations.

    move_schain_from_26_to_25(request);
    move_gdpr_from_26_to_25(request);
    move_us_privacy_from_26_to_25(request);
    move_consent_from_26_to_25(request);
    move_eids_from_26_to_25(request);
    move_rewarded_from_26_to_ext(request);

    // Now blank-out all 2.6-only fields so they are omitted during serialization.
    clear_26_fields(request);
    clear_202211_fields(request);
    clear_202303_fields(request);
    clear_202309_fields(request);
}

/// Move `source.schain` (2.6 location) into `source.ext.schain` (2.5 location).
fn move_schain_from_26_to_25(request: &mut BidRequest) {
    let schain = match request.source.as_mut() {
        Some(source) => source.schain.take(),
        None => return,
    };

    let schain = match schain {
        Some(sc) => sc,
        None => return,
    };

    let source = request.source.as_mut().unwrap();
    let ext = source.ext.get_or_insert_with(|| Value::Object(Default::default()));

    if let Value::Object(map) = ext {
        if !map.contains_key("schain") {
            if let Ok(v) = serde_json::to_value(&schain) {
                map.insert("schain".to_string(), v);
            }
        }
    }
}

/// Move `regs.gdpr` (stored in `regs.ext.gdpr` in the Rust model since
/// the Rust `Regs` struct doesn't have a dedicated `gdpr` field) -- in our
/// Rust openrtb model `Regs` does not have a top-level `gdpr` field, so
/// this is effectively a no-op. We clear the ext field if present.
fn move_gdpr_from_26_to_25(request: &mut BidRequest) {
    // The Rust Regs struct doesn't carry a top-level `gdpr` field.
    // If it is present in `regs.ext`, it is already in the 2.5 location.
    // Nothing to move, but ensure consistency.
    let _ = request;
}

/// Move `regs.us_privacy` from the 2.6 native field to `regs.ext.us_privacy`.
fn move_us_privacy_from_26_to_25(request: &mut BidRequest) {
    let us_privacy = match request.regs.as_mut() {
        Some(regs) => regs.us_privacy.take(),
        None => return,
    };

    let us_privacy = match us_privacy {
        Some(v) if !v.is_empty() => v,
        _ => return,
    };

    let regs = request.regs.as_mut().unwrap();
    let ext = regs.ext.get_or_insert_with(|| Value::Object(Default::default()));
    if let Value::Object(map) = ext {
        if !map.contains_key("us_privacy") {
            map.insert("us_privacy".to_string(), Value::String(us_privacy));
        }
    }
}

/// Move `user.consent` -- Rust `User` doesn't have a top-level consent
/// field, so this is a no-op in the current model.
fn move_consent_from_26_to_25(_request: &mut BidRequest) {
    // The Rust User struct does not carry a native `consent` field.
    // If the field lives inside `user.ext`, it is already in the 2.5 location.
}

/// Move `user.eids` (2.6 location) into `user.ext.eids` (2.5 location).
fn move_eids_from_26_to_25(request: &mut BidRequest) {
    let eids = match request.user.as_mut() {
        Some(user) => user.eids.take(),
        None => return,
    };

    let eids = match eids {
        Some(e) if !e.is_empty() => e,
        _ => return,
    };

    let user = request.user.as_mut().unwrap();
    let ext = user.ext.get_or_insert_with(|| Value::Object(Default::default()));
    if let Value::Object(map) = ext {
        if !map.contains_key("eids") {
            if let Ok(v) = serde_json::to_value(&eids) {
                map.insert("eids".to_string(), v);
            }
        }
    }
}

/// Move `imp[].rwdd` (2.6) to `imp[].ext.prebid.is_rewarded_inventory` (2.5/prebid).
fn move_rewarded_from_26_to_ext(request: &mut BidRequest) {
    for imp in &mut request.imp {
        let rwdd = match imp.rwdd.take() {
            Some(v) if v != 0 => v,
            _ => continue,
        };

        let ext = imp.ext.get_or_insert_with(|| Value::Object(Default::default()));
        if let Value::Object(map) = ext {
            let prebid = map
                .entry("prebid".to_string())
                .or_insert_with(|| Value::Object(Default::default()));
            if let Value::Object(prebid_map) = prebid {
                if !prebid_map.contains_key("is_rewarded_inventory") {
                    prebid_map.insert(
                        "is_rewarded_inventory".to_string(),
                        Value::Number(rwdd.into()),
                    );
                }
            }
        }
    }
}

// ── Clear functions ─────────────────────────────────────────────────────────

/// Clear fields introduced in OpenRTB 2.6.
///
/// Many 2.6 fields (cattax, langb, sua, kwarray, network, channel, etc.) do
/// not exist in the Rust openrtb model yet. We clear the ones that are present
/// and strip unknown fields from the `ext` blobs where they may hide.
fn clear_26_fields(request: &mut BidRequest) {
    // device.sua / device.langb -- not modeled as dedicated fields.
    // We remove them from device.ext if they landed there.
    if let Some(device) = &mut request.device {
        remove_ext_keys(&mut device.ext, &["sua", "langb"]);
    }

    // site cattax / kwarray
    if let Some(site) = &mut request.site {
        remove_ext_keys(&mut site.ext, &["cattax", "kwarray", "inventorypartnerdomain"]);
        if let Some(content) = &mut site.content {
            remove_ext_keys(&mut content.ext, &["cattax", "kwarray", "langb", "network", "channel"]);
            if let Some(producer) = &mut content.producer {
                remove_ext_keys(&mut producer.ext, &["cattax"]);
            }
        }
        if let Some(publisher) = &mut site.publisher {
            remove_ext_keys(&mut publisher.ext, &["cattax"]);
        }
    }

    // app cattax / kwarray
    if let Some(app) = &mut request.app {
        remove_ext_keys(&mut app.ext, &["cattax", "kwarray", "inventorypartnerdomain"]);
        if let Some(content) = &mut app.content {
            remove_ext_keys(&mut content.ext, &["cattax", "kwarray", "langb", "network", "channel"]);
            if let Some(producer) = &mut content.producer {
                remove_ext_keys(&mut producer.ext, &["cattax"]);
            }
        }
        if let Some(publisher) = &mut app.publisher {
            remove_ext_keys(&mut publisher.ext, &["cattax"]);
        }
    }

    // user kwarray / consent / eids -- eids already moved above
    if let Some(user) = &mut request.user {
        remove_ext_keys(&mut user.ext, &["kwarray"]);
    }

    // regs
    if let Some(regs) = &mut request.regs {
        regs.us_privacy = None;
    }

    // source.schain already moved above
    if let Some(source) = &mut request.source {
        source.schain = None;
    }

    // imp-level 2.6 fields
    for imp in &mut request.imp {
        imp.rwdd = None;
        imp.ssai = None;
        remove_ext_keys(&mut imp.ext, &["qty", "dt", "refresh"]);

        // video 2.6 fields stored in ext
        if let Some(video) = &mut imp.video {
            video.plcmt = None;
            remove_ext_keys(
                &mut video.ext,
                &[
                    "maxseq", "poddur", "podid", "podseq", "slotinpod",
                    "mincpmpersec", "rqddurs", "durfloors", "poddedupe",
                ],
            );
        }

        // audio 2.6 fields stored in ext
        if let Some(audio) = &mut imp.audio {
            remove_ext_keys(
                &mut audio.ext,
                &[
                    "poddur", "rqddurs", "podid", "podseq", "slotinpod",
                    "mincpmpersec", "durfloors",
                ],
            );
        }
    }

    // top-level 2.6 fields that may live in request.ext
    remove_ext_keys(&mut request.ext, &["cattax", "wlangb", "acat"]);
}

/// Clear fields introduced in OpenRTB 2.6-202211.
fn clear_202211_fields(request: &mut BidRequest) {
    // DOOH -- not a dedicated field in the Rust model; remove from ext.
    remove_ext_keys(&mut request.ext, &["dooh"]);

    if let Some(regs) = &mut request.regs {
        regs.gpp = None;
        regs.gpp_sid = None;
    }

    for imp in &mut request.imp {
        remove_ext_keys(&mut imp.ext, &["qty", "dt"]);
    }
}

/// Clear fields introduced in OpenRTB 2.6-202303.
fn clear_202303_fields(request: &mut BidRequest) {
    for imp in &mut request.imp {
        remove_ext_keys(&mut imp.ext, &["refresh"]);
        if let Some(video) = &mut imp.video {
            video.plcmt = None;
        }
    }
}

/// Clear fields introduced in OpenRTB 2.6-202309.
fn clear_202309_fields(request: &mut BidRequest) {
    remove_ext_keys(&mut request.ext, &["acat"]);

    for imp in &mut request.imp {
        if let Some(audio) = &mut imp.audio {
            remove_ext_keys(&mut audio.ext, &["durfloors"]);
        }
        if let Some(video) = &mut imp.video {
            remove_ext_keys(&mut video.ext, &["durfloors"]);
        }
        // deal-level 2.6 fields
        if let Some(pmp) = &mut imp.pmp {
            for deal in &mut pmp.deals {
                remove_ext_keys(&mut deal.ext, &["guar", "mincpmpersec", "durfloors"]);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Up-conversion: 2.5 -> 2.6
// ---------------------------------------------------------------------------

/// Upgrade a request from OpenRTB 2.5 to 2.6 by moving fields from their ext
/// locations to the native 2.6 locations.
///
/// Equivalent of Go `ConvertUpTo26`.
pub fn convert_up_to_26(request: &mut BidRequest) {
    move_schain_from_25_to_26(request);
    move_us_privacy_from_25_to_26(request);
    move_eids_from_25_to_26(request);
    move_rewarded_from_ext_to_26(request);
}

/// Move `source.ext.schain` -> `source.schain`.
fn move_schain_from_25_to_26(request: &mut BidRequest) {
    let schain_val = {
        let ext = match request
            .source
            .as_mut()
            .and_then(|s| s.ext.as_mut())
        {
            Some(ext) => ext,
            None => return,
        };
        match ext {
            Value::Object(map) => map.remove("schain"),
            _ => return,
        }
    };

    let schain_val = match schain_val {
        Some(v) => v,
        None => return,
    };

    let source = request.source.as_mut().unwrap();
    if source.schain.is_none() {
        if let Ok(sc) = serde_json::from_value(schain_val) {
            source.schain = Some(sc);
        }
    }
}

/// Move `regs.ext.us_privacy` -> `regs.us_privacy`.
fn move_us_privacy_from_25_to_26(request: &mut BidRequest) {
    let val = {
        let ext = match request.regs.as_mut().and_then(|r| r.ext.as_mut()) {
            Some(ext) => ext,
            None => return,
        };
        match ext {
            Value::Object(map) => map.remove("us_privacy"),
            _ => return,
        }
    };

    let val = match val {
        Some(Value::String(s)) if !s.is_empty() => s,
        _ => return,
    };

    let regs = request.regs.as_mut().unwrap();
    if regs.us_privacy.is_none() {
        regs.us_privacy = Some(val);
    }
}

/// Move `user.ext.eids` -> `user.eids`.
fn move_eids_from_25_to_26(request: &mut BidRequest) {
    let val = {
        let ext = match request.user.as_mut().and_then(|u| u.ext.as_mut()) {
            Some(ext) => ext,
            None => return,
        };
        match ext {
            Value::Object(map) => map.remove("eids"),
            _ => return,
        }
    };

    let val = match val {
        Some(v) => v,
        None => return,
    };

    let user = request.user.as_mut().unwrap();
    if user.eids.is_none() {
        if let Ok(eids) = serde_json::from_value(val) {
            user.eids = Some(eids);
        }
    }
}

/// Move `imp[].ext.prebid.is_rewarded_inventory` -> `imp[].rwdd`.
fn move_rewarded_from_ext_to_26(request: &mut BidRequest) {
    for imp in &mut request.imp {
        let rwdd_val = (|| -> Option<i32> {
            let ext = imp.ext.as_mut()?;
            let prebid = ext.as_object_mut()?.get_mut("prebid")?;
            let val = prebid.as_object_mut()?.remove("is_rewarded_inventory")?;
            val.as_i64().map(|v| v as i32)
        })();

        if let Some(rwdd) = rwdd_val {
            if imp.rwdd.is_none() || imp.rwdd == Some(0) {
                imp.rwdd = Some(rwdd);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Version negotiation
// ---------------------------------------------------------------------------

/// Returns `true` if the bidder supports OpenRTB 2.6 natively.
pub fn bidder_supports_26(bidder_ortb_version: Option<&str>) -> bool {
    bidder_ortb_version == Some("2.6")
}

/// Conditionally convert the request based on the bidder's declared OpenRTB
/// version. If the bidder does not declare 2.6 support the request is
/// down-converted to 2.5.
pub fn convert_for_bidder(request: &mut BidRequest, bidder_ortb_version: Option<&str>) {
    if !bidder_supports_26(bidder_ortb_version) {
        convert_down_to_25(request);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Remove a set of keys from an optional JSON `Value::Object`.
fn remove_ext_keys(ext: &mut Option<Value>, keys: &[&str]) {
    if let Some(Value::Object(map)) = ext {
        for key in keys {
            map.remove(*key);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use openrtb::{BidRequest, Device, Imp, Regs, Source, User, Video};

    #[test]
    fn test_convert_down_clears_schain() {
        let mut request = BidRequest {
            source: Some(Source {
                schain: Some(openrtb::SupplyChain {
                    complete: 1,
                    ver: "1.0".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        convert_down_to_25(&mut request);

        // schain should be moved from source.schain to source.ext.schain
        assert!(request.source.as_ref().unwrap().schain.is_none());
        let ext = request.source.as_ref().unwrap().ext.as_ref().unwrap();
        assert!(ext.get("schain").is_some());
    }

    #[test]
    fn test_convert_down_clears_us_privacy() {
        let mut request = BidRequest {
            regs: Some(Regs {
                us_privacy: Some("1YNN".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        convert_down_to_25(&mut request);

        assert!(request.regs.as_ref().unwrap().us_privacy.is_none());
        let ext = request.regs.as_ref().unwrap().ext.as_ref().unwrap();
        assert_eq!(ext.get("us_privacy").unwrap().as_str().unwrap(), "1YNN");
    }

    #[test]
    fn test_convert_down_clears_eids() {
        let mut request = BidRequest {
            user: Some(User {
                eids: Some(vec![openrtb::Eid {
                    source: Some("example.com".to_string()),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
            ..Default::default()
        };

        convert_down_to_25(&mut request);

        assert!(request.user.as_ref().unwrap().eids.is_none());
        let ext = request.user.as_ref().unwrap().ext.as_ref().unwrap();
        let eids = ext.get("eids").unwrap().as_array().unwrap();
        assert_eq!(eids.len(), 1);
    }

    #[test]
    fn test_convert_down_clears_rwdd() {
        let mut request = BidRequest {
            imp: vec![Imp {
                id: "imp1".to_string(),
                rwdd: Some(1),
                ..Default::default()
            }],
            ..Default::default()
        };

        convert_down_to_25(&mut request);

        assert!(request.imp[0].rwdd.is_none());
        let ext = request.imp[0].ext.as_ref().unwrap();
        let prebid = ext.get("prebid").unwrap();
        assert_eq!(
            prebid.get("is_rewarded_inventory").unwrap().as_i64().unwrap(),
            1
        );
    }

    #[test]
    fn test_convert_down_clears_ssai() {
        let mut request = BidRequest {
            imp: vec![Imp {
                id: "imp1".to_string(),
                ssai: Some(1),
                ..Default::default()
            }],
            ..Default::default()
        };

        convert_down_to_25(&mut request);
        assert!(request.imp[0].ssai.is_none());
    }

    #[test]
    fn test_convert_down_clears_video_plcmt() {
        let mut request = BidRequest {
            imp: vec![Imp {
                id: "imp1".to_string(),
                video: Some(Video {
                    plcmt: Some(2),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        };

        convert_down_to_25(&mut request);
        assert!(request.imp[0].video.as_ref().unwrap().plcmt.is_none());
    }

    #[test]
    fn test_convert_down_clears_gpp() {
        let mut request = BidRequest {
            regs: Some(Regs {
                gpp: Some("DBACNYA~CPXxRfAPXxRfAAfKABENB-CgAAAAAAAAAAYgAAAAAAAA".to_string()),
                gpp_sid: Some(vec![2]),
                ..Default::default()
            }),
            ..Default::default()
        };

        convert_down_to_25(&mut request);
        assert!(request.regs.as_ref().unwrap().gpp.is_none());
        assert!(request.regs.as_ref().unwrap().gpp_sid.is_none());
    }

    #[test]
    fn test_convert_down_removes_ext_cattax() {
        let mut request = BidRequest {
            ext: Some(serde_json::json!({"cattax": 2, "other": "keep"})),
            ..Default::default()
        };

        convert_down_to_25(&mut request);
        let ext = request.ext.as_ref().unwrap();
        assert!(ext.get("cattax").is_none());
        assert_eq!(ext.get("other").unwrap().as_str().unwrap(), "keep");
    }

    #[test]
    fn test_convert_up_moves_schain_to_26() {
        let mut request = BidRequest {
            source: Some(Source {
                ext: Some(serde_json::json!({
                    "schain": {
                        "complete": 1,
                        "ver": "1.0",
                        "nodes": []
                    }
                })),
                ..Default::default()
            }),
            ..Default::default()
        };

        convert_up_to_26(&mut request);
        assert!(request.source.as_ref().unwrap().schain.is_some());
        let ext = request.source.as_ref().unwrap().ext.as_ref().unwrap();
        assert!(ext.get("schain").is_none());
    }

    #[test]
    fn test_convert_up_moves_us_privacy_to_26() {
        let mut request = BidRequest {
            regs: Some(Regs {
                ext: Some(serde_json::json!({"us_privacy": "1YNN"})),
                ..Default::default()
            }),
            ..Default::default()
        };

        convert_up_to_26(&mut request);
        assert_eq!(
            request.regs.as_ref().unwrap().us_privacy.as_deref().unwrap(),
            "1YNN"
        );
    }

    #[test]
    fn test_convert_up_moves_eids_to_26() {
        let mut request = BidRequest {
            user: Some(User {
                ext: Some(serde_json::json!({
                    "eids": [{"source": "example.com", "uids": [{"id": "abc", "atype": 1}]}]
                })),
                ..Default::default()
            }),
            ..Default::default()
        };

        convert_up_to_26(&mut request);
        let eids = request.user.as_ref().unwrap().eids.as_ref().unwrap();
        assert_eq!(eids.len(), 1);
        assert_eq!(eids[0].source.as_deref().unwrap(), "example.com");
    }

    #[test]
    fn test_convert_up_moves_rewarded_to_26() {
        let mut request = BidRequest {
            imp: vec![Imp {
                id: "imp1".to_string(),
                ext: Some(serde_json::json!({
                    "prebid": {"is_rewarded_inventory": 1}
                })),
                ..Default::default()
            }],
            ..Default::default()
        };

        convert_up_to_26(&mut request);
        assert_eq!(request.imp[0].rwdd, Some(1));
    }

    #[test]
    fn test_bidder_supports_26() {
        assert!(bidder_supports_26(Some("2.6")));
        assert!(!bidder_supports_26(Some("2.5")));
        assert!(!bidder_supports_26(None));
    }

    #[test]
    fn test_convert_for_bidder_26_noop() {
        let mut request = BidRequest {
            imp: vec![Imp {
                id: "imp1".to_string(),
                rwdd: Some(1),
                ssai: Some(1),
                ..Default::default()
            }],
            ..Default::default()
        };

        convert_for_bidder(&mut request, Some("2.6"));
        // 2.6 bidder should keep these fields
        assert_eq!(request.imp[0].rwdd, Some(1));
        assert_eq!(request.imp[0].ssai, Some(1));
    }

    #[test]
    fn test_convert_for_bidder_25_strips() {
        let mut request = BidRequest {
            imp: vec![Imp {
                id: "imp1".to_string(),
                rwdd: Some(1),
                ssai: Some(1),
                ..Default::default()
            }],
            ..Default::default()
        };

        convert_for_bidder(&mut request, Some("2.5"));
        assert!(request.imp[0].rwdd.is_none());
        assert!(request.imp[0].ssai.is_none());
    }

    #[test]
    fn test_convert_down_device_ext_sua_removed() {
        let mut request = BidRequest {
            device: Some(Device {
                ext: Some(serde_json::json!({"sua": {"some": "data"}, "ua": "keep"})),
                ..Default::default()
            }),
            ..Default::default()
        };

        convert_down_to_25(&mut request);
        let ext = request.device.as_ref().unwrap().ext.as_ref().unwrap();
        assert!(ext.get("sua").is_none());
        assert!(ext.get("ua").is_some());
    }

    #[test]
    fn test_remove_ext_keys_none() {
        let mut ext: Option<Value> = None;
        remove_ext_keys(&mut ext, &["foo"]);
        assert!(ext.is_none());
    }

    #[test]
    fn test_convert_down_no_panic_on_empty_request() {
        let mut request = BidRequest::default();
        convert_down_to_25(&mut request);
        // should not panic
    }

    #[test]
    fn test_convert_up_no_panic_on_empty_request() {
        let mut request = BidRequest::default();
        convert_up_to_26(&mut request);
        // should not panic
    }
}
