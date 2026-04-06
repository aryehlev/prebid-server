//! OpenRTB version conversion (2.5 <-> 2.6).
//! Mirrors Go `openrtb_ext/convert_up.go` and `openrtb_ext/convert_down.go`.
//!
//! OpenRTB 2.6 moved several fields from extensions to first-class fields.
//! These functions handle the bidirectional migration.

use openrtb::BidRequest;

// ---------------------------------------------------------------------------
// Convert Up: 2.5 -> 2.6
// ---------------------------------------------------------------------------

/// Convert an OpenRTB 2.5 request up to 2.6 by moving fields from ext to
/// first-class locations.
pub fn convert_up_to_26(req: &mut BidRequest) {
    move_supply_chain_from_25_to_26(req);
    move_gdpr_from_25_to_26(req);
    move_consent_from_25_to_26(req);
    move_us_privacy_from_25_to_26(req);
    move_eid_from_25_to_26(req);
    move_rewarded_from_ext_to_26(req);
}

/// Move source.ext.schain -> source.schain (2.5 -> 2.6).
fn move_supply_chain_from_25_to_26(req: &mut BidRequest) {
    let schain_val = req
        .source
        .as_mut()
        .and_then(|s| s.ext.as_mut())
        .and_then(|ext| ext.as_object_mut())
        .and_then(|map| map.remove("schain"));

    if let Some(schain_val) = schain_val {
        if let Some(source) = &mut req.source {
            if source.schain.is_none() {
                if let Ok(schain) = serde_json::from_value(schain_val) {
                    source.schain = Some(schain);
                }
            }
        }
    }
}

/// Move regs.ext.gdpr -> regs.gdpr (2.5 -> 2.6).
fn move_gdpr_from_25_to_26(req: &mut BidRequest) {
    let gdpr_val = req
        .regs
        .as_mut()
        .and_then(|r| r.ext.as_mut())
        .and_then(|ext| ext.as_object_mut())
        .and_then(|map| map.remove("gdpr"));

    if let Some(gdpr_val) = gdpr_val {
        if let Some(regs) = &mut req.regs {
            if regs.gdpr.is_none() {
                regs.gdpr = gdpr_val.as_i64().map(|v| v as i8);
            }
        }
    }
}

/// Move user.ext.consent -> user.consent (2.5 -> 2.6).
fn move_consent_from_25_to_26(req: &mut BidRequest) {
    let consent_val = req
        .user
        .as_mut()
        .and_then(|u| u.ext.as_mut())
        .and_then(|ext| ext.as_object_mut())
        .and_then(|map| map.remove("consent"));

    if let Some(consent_val) = consent_val {
        if let Some(user) = &mut req.user {
            if user.consent.is_none() {
                user.consent = consent_val.as_str().map(|s| s.to_string());
            }
        }
    }
}

/// Move regs.ext.us_privacy -> regs.us_privacy (2.5 -> 2.6).
fn move_us_privacy_from_25_to_26(req: &mut BidRequest) {
    let usp_val = req
        .regs
        .as_mut()
        .and_then(|r| r.ext.as_mut())
        .and_then(|ext| ext.as_object_mut())
        .and_then(|map| map.remove("us_privacy"));

    if let Some(usp_val) = usp_val {
        if let Some(regs) = &mut req.regs {
            if regs.us_privacy.is_none() {
                regs.us_privacy = usp_val.as_str().map(|s| s.to_string());
            }
        }
    }
}

/// Move user.ext.eids -> user.eids (2.5 -> 2.6).
fn move_eid_from_25_to_26(req: &mut BidRequest) {
    let eids_val = req
        .user
        .as_mut()
        .and_then(|u| u.ext.as_mut())
        .and_then(|ext| ext.as_object_mut())
        .and_then(|map| map.remove("eids"));

    if let Some(eids_val) = eids_val {
        if let Some(user) = &mut req.user {
            if user.eids.is_none() {
                if let Ok(eids) = serde_json::from_value(eids_val) {
                    user.eids = Some(eids);
                }
            }
        }
    }
}

/// Move imp.ext.prebid.is_rewarded_inventory -> imp.rwdd (2.5 -> 2.6).
fn move_rewarded_from_ext_to_26(req: &mut BidRequest) {
    for imp in &mut req.imp {
        let rewarded = imp
            .ext
            .as_mut()
            .and_then(|ext| ext.as_object_mut())
            .and_then(|map| map.get_mut("prebid"))
            .and_then(|prebid| prebid.as_object_mut())
            .and_then(|map| map.remove("is_rewarded_inventory"));

        if let Some(rewarded) = rewarded {
            if imp.rwdd.is_none() {
                imp.rwdd = rewarded.as_i64().map(|v| v as i32);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Convert Down: 2.6 -> 2.5
// ---------------------------------------------------------------------------

/// Convert an OpenRTB 2.6 request down to 2.5 by moving first-class fields
/// into extensions.
pub fn convert_down_to_25(req: &mut BidRequest) {
    move_supply_chain_from_26_to_25(req);
    move_gdpr_from_26_to_25(req);
    move_consent_from_26_to_25(req);
    move_us_privacy_from_26_to_25(req);
    move_eid_from_26_to_25(req);
    move_rewarded_from_26_to_ext(req);
    clear_26_fields(req);
}

/// Move source.schain -> source.ext.schain (2.6 -> 2.5).
fn move_supply_chain_from_26_to_25(req: &mut BidRequest) {
    if let Some(source) = &mut req.source {
        if let Some(schain) = source.schain.take() {
            if let Ok(schain_val) = serde_json::to_value(&schain) {
                let ext = source.ext.get_or_insert_with(|| serde_json::json!({}));
                if let Some(map) = ext.as_object_mut() {
                    map.entry("schain".to_string()).or_insert(schain_val);
                }
            }
        }
    }
}

/// Move regs.gdpr -> regs.ext.gdpr (2.6 -> 2.5).
fn move_gdpr_from_26_to_25(req: &mut BidRequest) {
    if let Some(regs) = &mut req.regs {
        if let Some(gdpr) = regs.gdpr.take() {
            let ext = regs.ext.get_or_insert_with(|| serde_json::json!({}));
            if let Some(map) = ext.as_object_mut() {
                map.entry("gdpr".to_string()).or_insert(serde_json::json!(gdpr));
            }
        }
    }
}

/// Move user.consent -> user.ext.consent (2.6 -> 2.5).
fn move_consent_from_26_to_25(req: &mut BidRequest) {
    if let Some(user) = &mut req.user {
        if let Some(consent) = user.consent.take() {
            let ext = user.ext.get_or_insert_with(|| serde_json::json!({}));
            if let Some(map) = ext.as_object_mut() {
                map.entry("consent".to_string())
                    .or_insert(serde_json::json!(consent));
            }
        }
    }
}

/// Move regs.us_privacy -> regs.ext.us_privacy (2.6 -> 2.5).
fn move_us_privacy_from_26_to_25(req: &mut BidRequest) {
    if let Some(regs) = &mut req.regs {
        if let Some(usp) = regs.us_privacy.take() {
            let ext = regs.ext.get_or_insert_with(|| serde_json::json!({}));
            if let Some(map) = ext.as_object_mut() {
                map.entry("us_privacy".to_string())
                    .or_insert(serde_json::json!(usp));
            }
        }
    }
}

/// Move user.eids -> user.ext.eids (2.6 -> 2.5).
fn move_eid_from_26_to_25(req: &mut BidRequest) {
    if let Some(user) = &mut req.user {
        if let Some(eids) = user.eids.take() {
            if let Ok(eids_val) = serde_json::to_value(&eids) {
                let ext = user.ext.get_or_insert_with(|| serde_json::json!({}));
                if let Some(map) = ext.as_object_mut() {
                    map.entry("eids".to_string()).or_insert(eids_val);
                }
            }
        }
    }
}

/// Move imp.rwdd -> imp.ext.prebid.is_rewarded_inventory (2.6 -> 2.5).
fn move_rewarded_from_26_to_ext(req: &mut BidRequest) {
    for imp in &mut req.imp {
        if let Some(rwdd) = imp.rwdd.take() {
            let ext = imp.ext.get_or_insert_with(|| serde_json::json!({}));
            if let Some(ext_map) = ext.as_object_mut() {
                let prebid = ext_map
                    .entry("prebid".to_string())
                    .or_insert(serde_json::json!({}));
                if let Some(prebid_map) = prebid.as_object_mut() {
                    prebid_map
                        .entry("is_rewarded_inventory".to_string())
                        .or_insert(serde_json::json!(rwdd));
                }
            }
        }
    }
}

/// Clear 2.6-specific first-class fields that were moved to ext.
fn clear_26_fields(req: &mut BidRequest) {
    if let Some(source) = &mut req.source {
        source.schain = None;
    }
    if let Some(regs) = &mut req.regs {
        regs.gdpr = None;
        regs.us_privacy = None;
    }
    if let Some(user) = &mut req.user {
        user.consent = None;
        user.eids = None;
    }
    for imp in &mut req.imp {
        imp.rwdd = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openrtb::*;

    #[test]
    fn test_convert_up_gdpr() {
        let mut req = BidRequest::default();
        req.regs = Some(Regs {
            ext: Some(serde_json::json!({"gdpr": 1})),
            ..Default::default()
        });
        convert_up_to_26(&mut req);
        assert_eq!(req.regs.as_ref().unwrap().gdpr, Some(1));
        // ext.gdpr should be removed
        let ext = req.regs.as_ref().unwrap().ext.as_ref().unwrap();
        assert!(ext.get("gdpr").is_none());
    }

    #[test]
    fn test_convert_up_consent() {
        let mut req = BidRequest::default();
        req.user = Some(User {
            ext: Some(serde_json::json!({"consent": "BOEFEAyOEFEAyAHABDENAI4AAAB9vABAASA"})),
            ..Default::default()
        });
        convert_up_to_26(&mut req);
        assert_eq!(
            req.user.as_ref().unwrap().consent.as_deref(),
            Some("BOEFEAyOEFEAyAHABDENAI4AAAB9vABAASA")
        );
    }

    #[test]
    fn test_convert_up_us_privacy() {
        let mut req = BidRequest::default();
        req.regs = Some(Regs {
            ext: Some(serde_json::json!({"us_privacy": "1YNN"})),
            ..Default::default()
        });
        convert_up_to_26(&mut req);
        assert_eq!(
            req.regs.as_ref().unwrap().us_privacy.as_deref(),
            Some("1YNN")
        );
    }

    #[test]
    fn test_convert_down_gdpr() {
        let mut req = BidRequest::default();
        req.regs = Some(Regs {
            gdpr: Some(1),
            ..Default::default()
        });
        convert_down_to_25(&mut req);
        assert!(req.regs.as_ref().unwrap().gdpr.is_none());
        let ext = req.regs.as_ref().unwrap().ext.as_ref().unwrap();
        assert_eq!(ext.get("gdpr").unwrap().as_i64(), Some(1));
    }

    #[test]
    fn test_convert_down_consent() {
        let mut req = BidRequest::default();
        req.user = Some(User {
            consent: Some("BOEFEAyOEFEAy".to_string()),
            ..Default::default()
        });
        convert_down_to_25(&mut req);
        assert!(req.user.as_ref().unwrap().consent.is_none());
        let ext = req.user.as_ref().unwrap().ext.as_ref().unwrap();
        assert_eq!(ext.get("consent").unwrap().as_str(), Some("BOEFEAyOEFEAy"));
    }

    #[test]
    fn test_convert_up_rewarded() {
        let mut req = BidRequest::default();
        req.imp.push(Imp {
            id: "1".to_string(),
            ext: Some(serde_json::json!({"prebid": {"is_rewarded_inventory": 1}})),
            ..Default::default()
        });
        convert_up_to_26(&mut req);
        assert_eq!(req.imp[0].rwdd, Some(1));
    }

    #[test]
    fn test_convert_down_rewarded() {
        let mut req = BidRequest::default();
        req.imp.push(Imp {
            id: "1".to_string(),
            rwdd: Some(1),
            ..Default::default()
        });
        convert_down_to_25(&mut req);
        assert!(req.imp[0].rwdd.is_none());
        let ext = req.imp[0].ext.as_ref().unwrap();
        assert_eq!(
            ext.get("prebid").unwrap().get("is_rewarded_inventory").unwrap().as_i64(),
            Some(1)
        );
    }

    #[test]
    fn test_round_trip() {
        let mut req = BidRequest::default();
        req.regs = Some(Regs {
            gdpr: Some(1),
            us_privacy: Some("1YNN".to_string()),
            ..Default::default()
        });
        req.user = Some(User {
            consent: Some("consent123".to_string()),
            eids: Some(vec![Eid {
                source: Some("adserver.org".to_string()),
                ..Default::default()
            }]),
            ..Default::default()
        });

        // Convert down to 2.5
        convert_down_to_25(&mut req);
        assert!(req.regs.as_ref().unwrap().gdpr.is_none());
        assert!(req.user.as_ref().unwrap().consent.is_none());

        // Convert back up to 2.6
        convert_up_to_26(&mut req);
        assert_eq!(req.regs.as_ref().unwrap().gdpr, Some(1));
        assert_eq!(req.user.as_ref().unwrap().consent.as_deref(), Some("consent123"));
        assert_eq!(req.regs.as_ref().unwrap().us_privacy.as_deref(), Some("1YNN"));
    }
}
