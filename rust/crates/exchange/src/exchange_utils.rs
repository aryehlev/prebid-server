//! Exchange utility functions.
//! Mirrors Go `exchange/utils.go` — request splitting and bidder preparation.
//!
//! Contains functions for splitting a bid request into per-bidder requests,
//! extracting buyer UIDs, and resolving bidder names/aliases.

use std::collections::HashMap;

use openrtb::BidRequest;

/// Map of request type to channel type for privacy enforcement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelType {
    Web,
    App,
    Amp,
    Video,
    Dooh,
}

/// A bidder request prepared for execution.
#[derive(Debug, Clone)]
pub struct BidderRequest {
    /// The bidder name (may be an alias).
    pub bidder_name: String,
    /// The core bidder name (resolved from alias).
    pub bidder_core_name: String,
    /// Whether this bidder is an alias.
    pub is_request_alias: bool,
    /// The bid request tailored for this bidder.
    pub bid_request: BidRequest,
    /// Bidder-specific labels for metrics.
    pub bidder_labels: BidderLabels,
}

/// Labels for bidder-level metrics.
#[derive(Debug, Clone, Default)]
pub struct BidderLabels {
    pub adapter: String,
    pub adapter_bids: String,
    pub cookie_flag: String,
}

/// Privacy labels tracked during request processing.
#[derive(Debug, Clone, Default)]
pub struct PrivacyLabels {
    pub ccpa_provided: bool,
    pub ccpa_enforced: bool,
    pub coppa_enforced: bool,
    pub gdpr_enforced: bool,
    pub gdpr_tcf_version: i32,
    pub lmt_enforced: bool,
}

/// Split imps by bidder name based on imp.ext.prebid.bidder configuration.
/// Returns a map of bidder name -> list of imps for that bidder.
/// Mirrors Go `splitImps`.
pub fn split_imps(
    imps: &[openrtb::Imp],
    aliases: &HashMap<String, String>,
) -> Result<HashMap<String, Vec<openrtb::Imp>>, String> {
    let mut imps_by_bidder: HashMap<String, Vec<openrtb::Imp>> = HashMap::new();

    for (i, imp) in imps.iter().enumerate() {
        let ext = match &imp.ext {
            Some(ext) => ext,
            None => {
                return Err(format!(
                    "request.imp[{}].ext is required",
                    i
                ));
            }
        };

        // Try imp.ext.prebid.bidder first (newer format)
        let bidders = if let Some(prebid) = ext.get("prebid") {
            if let Some(bidder_map) = prebid.get("bidder") {
                if let Some(map) = bidder_map.as_object() {
                    map.keys().cloned().collect::<Vec<_>>()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            // Fallback: look for bidder names directly in imp.ext (legacy format)
            if let Some(map) = ext.as_object() {
                map.keys()
                    .filter(|k| *k != "prebid" && *k != "context" && *k != "data")
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            }
        };

        if bidders.is_empty() {
            return Err(format!(
                "request.imp[{}].ext.prebid.bidder must contain at least one bidder",
                i
            ));
        }

        for bidder_name in bidders {
            // Resolve alias to core bidder
            let _core_bidder = resolve_bidder(&bidder_name, aliases);

            imps_by_bidder
                .entry(bidder_name)
                .or_default()
                .push(imp.clone());
        }
    }

    Ok(imps_by_bidder)
}

/// Resolve a bidder name through aliases.
/// Mirrors Go `resolveBidder`.
pub fn resolve_bidder(bidder: &str, aliases: &HashMap<String, String>) -> (String, bool) {
    if let Some(core) = aliases.get(bidder) {
        (core.clone(), true)
    } else {
        (bidder.to_string(), false)
    }
}

/// Extract buyer UIDs from user.ext.prebid.buyeruids.
/// Mirrors Go `extractAndCleanBuyerUIDs`.
pub fn extract_buyer_uids(req: &BidRequest) -> HashMap<String, String> {
    let mut buyer_uids = HashMap::new();

    if let Some(user) = &req.user {
        if let Some(ext) = &user.ext {
            if let Some(prebid) = ext.get("prebid") {
                if let Some(buyeruids) = prebid.get("buyeruids") {
                    if let Some(map) = buyeruids.as_object() {
                        for (bidder, uid) in map {
                            if let Some(uid_str) = uid.as_str() {
                                buyer_uids.insert(bidder.clone(), uid_str.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    buyer_uids
}

/// Extract request-level bidder params from request.ext.prebid.bidderparams.
/// Mirrors Go `ExtractReqExtBidderParamsMap`.
pub fn extract_req_ext_bidder_params(
    req: &BidRequest,
) -> HashMap<String, serde_json::Value> {
    let mut params = HashMap::new();

    if let Some(ext) = &req.ext {
        if let Some(prebid) = ext.get("prebid") {
            if let Some(bidder_params) = prebid.get("bidderparams") {
                if let Some(map) = bidder_params.as_object() {
                    for (bidder, param_val) in map {
                        params.insert(bidder.clone(), param_val.clone());
                    }
                }
            }
        }
    }

    params
}

/// Extract request aliases from request.ext.prebid.aliases.
/// Returns (aliases map, alias GVL IDs map).
/// Mirrors Go `getRequestAliases`.
pub fn get_request_aliases(
    req: &BidRequest,
) -> (HashMap<String, String>, HashMap<String, u16>) {
    let mut aliases = HashMap::new();
    let mut alias_gvl_ids = HashMap::new();

    if let Some(ext) = &req.ext {
        if let Some(prebid) = ext.get("prebid") {
            if let Some(alias_map) = prebid.get("aliases") {
                if let Some(map) = alias_map.as_object() {
                    for (alias, core) in map {
                        if let Some(core_str) = core.as_str() {
                            aliases.insert(alias.clone(), core_str.to_string());
                        }
                    }
                }
            }
            if let Some(gvl_map) = prebid.get("aliasgvlids") {
                if let Some(map) = gvl_map.as_object() {
                    for (alias, gvl_id) in map {
                        if let Some(id) = gvl_id.as_u64() {
                            alias_gvl_ids.insert(alias.clone(), id as u16);
                        }
                    }
                }
            }
        }
    }

    (aliases, alias_gvl_ids)
}

/// Check if a field represents a potential bidder name.
/// Returns true if the key is NOT a known non-bidder extension field.
pub fn is_potential_bidder_field(key: &str) -> bool {
    !matches!(
        key,
        "prebid" | "context" | "data" | "skadn" | "gpid" | "tid" | "ae"
    )
}

/// Get the media type for a bid based on imp extensions.
/// Mirrors Go `getMediaTypeForBid`.
pub fn get_media_type_for_bid(
    imp_id: &str,
    imps: &[openrtb::Imp],
) -> Option<String> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return Some("banner".to_string());
            }
            if imp.video.is_some() {
                return Some("video".to_string());
            }
            if imp.native.is_some() {
                return Some("native".to_string());
            }
            if imp.audio.is_some() {
                return Some("audio".to_string());
            }
        }
    }
    None
}

/// Remove EIDs that the bidder does not have permission to see.
/// Reads request.ext.prebid.data.eidpermissions.
/// Mirrors Go `removeUnpermissionedEids`.
pub fn remove_unpermissioned_eids(
    req: &mut BidRequest,
    bidder: &str,
) {
    let permissions = match &req.ext {
        Some(ext) => {
            ext.get("prebid")
                .and_then(|p| p.get("data"))
                .and_then(|d| d.get("eidpermissions"))
                .and_then(|e| e.as_array())
                .cloned()
        }
        None => None,
    };

    let permissions = match permissions {
        Some(p) if !p.is_empty() => p,
        _ => return,
    };

    if let Some(user) = &mut req.user {
        if let Some(eids) = &mut user.eids {
            eids.retain(|eid| {
                let source = eid.source.as_deref().unwrap_or("");
                if source.is_empty() {
                    return true;
                }

                // Check if this EID source has permission restrictions
                for perm in &permissions {
                    let perm_source = perm.get("source").and_then(|s| s.as_str()).unwrap_or("");
                    if perm_source != source {
                        continue;
                    }

                    let bidders = perm
                        .get("bidders")
                        .and_then(|b| b.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();

                    if bidders.is_empty() {
                        return true;
                    }

                    // Check if "*" (all bidders) or specific bidder is in the list
                    return bidders.iter().any(|b| *b == "*" || *b == bidder);
                }

                true
            });
        }
    }
}

/// Make the hb_size targeting string from width and height.
/// Returns empty string if either dimension is 0.
/// Mirrors Go `makeHbSize`.
pub fn make_hb_size(w: i64, h: i64) -> String {
    if w != 0 && h != 0 {
        format!("{}x{}", w, h)
    } else {
        String::new()
    }
}

/// Check if user EID data exists in first party data for a bidder.
pub fn fpd_user_eid_exists(
    req: &BidRequest,
    fpd: &Option<HashMap<String, serde_json::Value>>,
    bidder: &str,
) -> bool {
    // Check if bidder-specific FPD has user.eids
    if let Some(fpd_map) = fpd {
        if let Some(bidder_fpd) = fpd_map.get(bidder) {
            if let Some(user) = bidder_fpd.get("user") {
                if let Some(eids) = user.get("eids") {
                    if let Some(arr) = eids.as_array() {
                        return !arr.is_empty();
                    }
                }
            }
        }
    }

    // Check request-level user.eids
    if let Some(user) = &req.user {
        if let Some(eids) = &user.eids {
            return !eids.is_empty();
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_bidder_direct() {
        let aliases = HashMap::new();
        let (name, is_alias) = resolve_bidder("appnexus", &aliases);
        assert_eq!(name, "appnexus");
        assert!(!is_alias);
    }

    #[test]
    fn test_resolve_bidder_alias() {
        let mut aliases = HashMap::new();
        aliases.insert("myalias".to_string(), "appnexus".to_string());
        let (name, is_alias) = resolve_bidder("myalias", &aliases);
        assert_eq!(name, "appnexus");
        assert!(is_alias);
    }

    #[test]
    fn test_extract_buyer_uids() {
        let mut req = BidRequest::default();
        req.user = Some(openrtb::User {
            ext: Some(serde_json::json!({
                "prebid": {
                    "buyeruids": {
                        "appnexus": "uid-123",
                        "rubicon": "uid-456"
                    }
                }
            })),
            ..Default::default()
        });

        let uids = extract_buyer_uids(&req);
        assert_eq!(uids.get("appnexus").unwrap(), "uid-123");
        assert_eq!(uids.get("rubicon").unwrap(), "uid-456");
    }

    #[test]
    fn test_extract_buyer_uids_empty() {
        let req = BidRequest::default();
        let uids = extract_buyer_uids(&req);
        assert!(uids.is_empty());
    }

    #[test]
    fn test_get_request_aliases() {
        let mut req = BidRequest::default();
        req.ext = Some(serde_json::json!({
            "prebid": {
                "aliases": {
                    "myalias": "appnexus"
                },
                "aliasgvlids": {
                    "myalias": 32
                }
            }
        }));

        let (aliases, gvl_ids) = get_request_aliases(&req);
        assert_eq!(aliases.get("myalias").unwrap(), "appnexus");
        assert_eq!(*gvl_ids.get("myalias").unwrap(), 32u16);
    }

    #[test]
    fn test_extract_bidder_params() {
        let mut req = BidRequest::default();
        req.ext = Some(serde_json::json!({
            "prebid": {
                "bidderparams": {
                    "appnexus": {"member": "1234"}
                }
            }
        }));

        let params = extract_req_ext_bidder_params(&req);
        assert!(params.contains_key("appnexus"));
        assert_eq!(params["appnexus"]["member"], "1234");
    }

    #[test]
    fn test_split_imps() {
        let imps = vec![
            openrtb::Imp {
                id: "imp-1".to_string(),
                ext: Some(serde_json::json!({
                    "prebid": {
                        "bidder": {
                            "appnexus": {"placementId": 123},
                            "rubicon": {"accountId": 456}
                        }
                    }
                })),
                ..Default::default()
            },
        ];

        let aliases = HashMap::new();
        let result = split_imps(&imps, &aliases).unwrap();
        assert!(result.contains_key("appnexus"));
        assert!(result.contains_key("rubicon"));
        assert_eq!(result["appnexus"].len(), 1);
        assert_eq!(result["rubicon"].len(), 1);
    }

    #[test]
    fn test_split_imps_no_ext() {
        let imps = vec![openrtb::Imp {
            id: "imp-1".to_string(),
            ..Default::default()
        }];

        let aliases = HashMap::new();
        let result = split_imps(&imps, &aliases);
        assert!(result.is_err());
    }

    #[test]
    fn test_make_hb_size() {
        assert_eq!(make_hb_size(300, 250), "300x250");
        assert_eq!(make_hb_size(0, 250), "");
        assert_eq!(make_hb_size(300, 0), "");
    }

    #[test]
    fn test_is_potential_bidder_field() {
        assert!(!is_potential_bidder_field("prebid"));
        assert!(!is_potential_bidder_field("context"));
        assert!(is_potential_bidder_field("appnexus"));
        assert!(is_potential_bidder_field("rubicon"));
    }

    #[test]
    fn test_get_media_type_for_bid() {
        let imps = vec![
            openrtb::Imp {
                id: "imp-1".to_string(),
                banner: Some(openrtb::Banner::default()),
                ..Default::default()
            },
            openrtb::Imp {
                id: "imp-2".to_string(),
                video: Some(openrtb::Video::default()),
                ..Default::default()
            },
        ];

        assert_eq!(
            get_media_type_for_bid("imp-1", &imps),
            Some("banner".to_string())
        );
        assert_eq!(
            get_media_type_for_bid("imp-2", &imps),
            Some("video".to_string())
        );
        assert_eq!(get_media_type_for_bid("imp-3", &imps), None);
    }

    #[test]
    fn test_remove_unpermissioned_eids() {
        let mut req = BidRequest::default();
        req.ext = Some(serde_json::json!({
            "prebid": {
                "data": {
                    "eidpermissions": [
                        {
                            "source": "adserver.org",
                            "bidders": ["appnexus"]
                        }
                    ]
                }
            }
        }));
        req.user = Some(openrtb::User {
            eids: Some(vec![
                openrtb::Eid {
                    source: Some("adserver.org".to_string()),
                    ..Default::default()
                },
                openrtb::Eid {
                    source: Some("other.com".to_string()),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        });

        // Rubicon should not see adserver.org EIDs
        remove_unpermissioned_eids(&mut req, "rubicon");
        let eids = req.user.as_ref().unwrap().eids.as_ref().unwrap();
        assert_eq!(eids.len(), 1);
        assert_eq!(eids[0].source.as_deref(), Some("other.com"));
    }

    #[test]
    fn test_remove_unpermissioned_eids_allowed() {
        let mut req = BidRequest::default();
        req.ext = Some(serde_json::json!({
            "prebid": {
                "data": {
                    "eidpermissions": [
                        {
                            "source": "adserver.org",
                            "bidders": ["appnexus"]
                        }
                    ]
                }
            }
        }));
        req.user = Some(openrtb::User {
            eids: Some(vec![openrtb::Eid {
                source: Some("adserver.org".to_string()),
                ..Default::default()
            }]),
            ..Default::default()
        });

        // Appnexus should see adserver.org EIDs
        remove_unpermissioned_eids(&mut req, "appnexus");
        let eids = req.user.as_ref().unwrap().eids.as_ref().unwrap();
        assert_eq!(eids.len(), 1);
    }
}
