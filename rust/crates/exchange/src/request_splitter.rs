//! Request splitting and per-bidder request building.
//!
//! Mirrors Go `exchange/utils.go` (`cleanOpenRTBRequests`, `splitImps`).
//!
//! Splits a single auction request into per-bidder sanitized requests,
//! each containing only the impressions and parameters relevant to that bidder.

use std::collections::HashMap;

use serde_json::Value;

use openrtb::BidRequest;

// ---------------------------------------------------------------------------
// BidderRequest
// ---------------------------------------------------------------------------

/// A sanitized per-bidder request ready for adapter execution.
///
/// Mirrors Go `exchange.BidderRequest`.
#[derive(Debug, Clone)]
pub struct BidderRequest {
    /// The sanitized bid request for this bidder.
    pub bid_request: BidRequest,
    /// The bidder name (may be an alias).
    pub bidder_name: String,
    /// The core bidder name (resolved from alias).
    pub bidder_core_name: String,
    /// Whether this bidder is requested via an alias.
    pub is_request_alias: bool,
    /// Stored bid responses for impressions targeting this bidder.
    pub bidder_stored_responses: HashMap<String, Value>,
    /// Whether to replace imp IDs from stored responses.
    pub imp_replace_imp_id: HashMap<String, bool>,
}

// ---------------------------------------------------------------------------
// splitImps — split impressions by bidder
// ---------------------------------------------------------------------------

/// Split impressions by bidder name.
///
/// Parses each impression's `ext.prebid.bidder` map and creates separate
/// impression copies for each targeted bidder, with only that bidder's
/// params in the extension.
///
/// Returns a map from bidder name to the impressions targeting that bidder.
///
/// Mirrors Go `splitImps`.
pub fn split_imps(
    imps: &[openrtb::Imp],
    request_aliases: &HashMap<String, String>,
) -> Result<HashMap<String, Vec<openrtb::Imp>>, String> {
    let mut bidder_imps: HashMap<String, Vec<openrtb::Imp>> = HashMap::new();

    for imp in imps {
        let ext = match &imp.ext {
            Some(ext) => ext,
            None => continue,
        };

        // Extract imp.ext.prebid.bidder map
        let bidder_map = ext
            .get("prebid")
            .and_then(|p| p.get("bidder"))
            .and_then(|b| b.as_object())
            .or_else(|| {
                // Fallback: treat top-level ext keys as bidder params
                ext.as_object()
            });

        let bidder_map = match bidder_map {
            Some(m) => m,
            None => continue,
        };

        // Reserved keys that are not bidder names
        let reserved = ["prebid", "context", "data", "skadn", "gpid", "tid", "ae"];

        for (bidder_name, bidder_params) in bidder_map {
            if reserved.contains(&bidder_name.as_str()) {
                continue;
            }

            // Resolve alias to core bidder name
            let core_name = request_aliases
                .get(bidder_name)
                .cloned()
                .unwrap_or_else(|| bidder_name.clone());

            // Create a sanitized copy of the impression
            let mut imp_copy = imp.clone();

            // Build sanitized ext containing only this bidder's params
            let mut sanitized_ext = serde_json::Map::new();

            // Preserve non-bidder fields from original ext
            if let Some(obj) = ext.as_object() {
                for &key in &reserved {
                    if key == "prebid" {
                        continue; // Handle prebid specially
                    }
                    if let Some(val) = obj.get(key) {
                        sanitized_ext.insert(key.to_string(), val.clone());
                    }
                }
            }

            // Build prebid.bidder with only this bidder's params
            let mut prebid = serde_json::Map::new();
            let mut bidder_obj = serde_json::Map::new();
            bidder_obj.insert(bidder_name.clone(), bidder_params.clone());
            prebid.insert("bidder".to_string(), Value::Object(bidder_obj));

            // Preserve other prebid fields (e.g., is_rewarded_inventory, options)
            if let Some(orig_prebid) = ext.get("prebid").and_then(|p| p.as_object()) {
                for (key, val) in orig_prebid {
                    if key != "bidder" && key != "bidders" {
                        prebid.entry(key.clone()).or_insert_with(|| val.clone());
                    }
                }
            }

            sanitized_ext.insert("prebid".to_string(), Value::Object(prebid));
            imp_copy.ext = Some(Value::Object(sanitized_ext));

            bidder_imps
                .entry(core_name)
                .or_default()
                .push(imp_copy);
        }
    }

    Ok(bidder_imps)
}

/// Extract request-level bidder aliases from `req.ext.prebid.aliases`.
///
/// Returns a map from alias name to core bidder name.
///
/// Mirrors Go `getRequestAliases`.
pub fn get_request_aliases(request: &BidRequest) -> HashMap<String, String> {
    let mut aliases = HashMap::new();

    if let Some(ext) = &request.ext {
        if let Some(alias_map) = ext
            .get("prebid")
            .and_then(|p| p.get("aliases"))
            .and_then(|a| a.as_object())
        {
            for (alias, core) in alias_map {
                if let Some(core_str) = core.as_str() {
                    aliases.insert(alias.clone(), core_str.to_string());
                }
            }
        }
    }

    aliases
}

/// Build a sanitized request extension for a specific bidder.
///
/// Removes bidder-specific params from `ext.prebid.bidder` and retains
/// only bidder-agnostic fields.
///
/// Mirrors Go `buildRequestExtForBidder`.
pub fn build_request_ext_for_bidder(
    request_ext: &Value,
    bidder_name: &str,
    bidder_params: Option<&Value>,
) -> Value {
    let mut ext = request_ext.clone();

    if let Some(obj) = ext.as_object_mut() {
        if let Some(prebid) = obj.get_mut("prebid").and_then(|p| p.as_object_mut()) {
            // Remove the full bidder map — each bidder should only see its own params
            prebid.remove("bidder");

            // Set bidderparams for this specific bidder if available
            if let Some(params) = bidder_params {
                let mut bp = serde_json::Map::new();
                bp.insert(bidder_name.to_string(), params.clone());
                prebid.insert("bidderparams".to_string(), Value::Object(bp));
            }

            // Remove fields that shouldn't leak between bidders
            prebid.remove("data");
            prebid.remove("bidderconfig");
            prebid.remove("aliases");
            prebid.remove("aliasgvlids");
        }
    }

    ext
}

/// Extract buyer UIDs from `user.ext.prebid.buyeruids`.
///
/// Returns a map from bidder syncer key to UID.
///
/// Mirrors Go `extractAndCleanBuyerUIDs`.
pub fn extract_buyer_uids(request: &BidRequest) -> HashMap<String, String> {
    let mut uids = HashMap::new();

    if let Some(user) = &request.user {
        if let Some(ext) = &user.ext {
            if let Some(buyeruids) = ext
                .get("prebid")
                .and_then(|p| p.get("buyeruids"))
                .and_then(|b| b.as_object())
            {
                for (key, val) in buyeruids {
                    if let Some(uid) = val.as_str() {
                        uids.insert(key.clone(), uid.to_string());
                    }
                }
            }
        }
    }

    uids
}

/// Set the correct BuyerUID on a user for a specific bidder.
///
/// Checks buyer UIDs map first, then falls back to cookie syncs.
///
/// Mirrors Go `prepareUser`.
pub fn prepare_user_for_bidder(
    user: &mut Option<openrtb::User>,
    bidder_syncer_key: &str,
    buyer_uids: &HashMap<String, String>,
    cookie_uids: &HashMap<String, String>,
) {
    let uid = buyer_uids
        .get(bidder_syncer_key)
        .or_else(|| cookie_uids.get(bidder_syncer_key));

    if let Some(uid) = uid {
        let user = user.get_or_insert_with(Default::default);
        user.buyeruid = Some(uid.clone());
    }
}

/// Remove the `user.ext.prebid.buyeruids` field to prevent leakage.
pub fn clean_buyer_uids(request: &mut BidRequest) {
    if let Some(user) = &mut request.user {
        if let Some(ext) = &mut user.ext {
            if let Some(obj) = ext.as_object_mut() {
                if let Some(prebid) = obj.get_mut("prebid").and_then(|p| p.as_object_mut()) {
                    prebid.remove("buyeruids");
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_split_imps_single_bidder() {
        let imps = vec![openrtb::Imp {
            id: "imp-1".to_string(),
            ext: Some(json!({
                "prebid": {
                    "bidder": {
                        "appnexus": {"placementId": 12345}
                    }
                }
            })),
            ..Default::default()
        }];

        let aliases = HashMap::new();
        let result = split_imps(&imps, &aliases).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result.contains_key("appnexus"));
        assert_eq!(result["appnexus"].len(), 1);
        assert_eq!(result["appnexus"][0].id, "imp-1");
    }

    #[test]
    fn test_split_imps_multiple_bidders() {
        let imps = vec![openrtb::Imp {
            id: "imp-1".to_string(),
            ext: Some(json!({
                "prebid": {
                    "bidder": {
                        "appnexus": {"placementId": 12345},
                        "rubicon": {"accountId": 1001}
                    }
                }
            })),
            ..Default::default()
        }];

        let aliases = HashMap::new();
        let result = split_imps(&imps, &aliases).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains_key("appnexus"));
        assert!(result.contains_key("rubicon"));
    }

    #[test]
    fn test_split_imps_with_alias() {
        let imps = vec![openrtb::Imp {
            id: "imp-1".to_string(),
            ext: Some(json!({
                "prebid": {
                    "bidder": {
                        "districtm": {"placementId": 999}
                    }
                }
            })),
            ..Default::default()
        }];

        let mut aliases = HashMap::new();
        aliases.insert("districtm".to_string(), "appnexus".to_string());

        let result = split_imps(&imps, &aliases).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result.contains_key("appnexus"));
    }

    #[test]
    fn test_split_imps_sanitizes_ext() {
        let imps = vec![openrtb::Imp {
            id: "imp-1".to_string(),
            ext: Some(json!({
                "prebid": {
                    "bidder": {
                        "appnexus": {"placementId": 1},
                        "rubicon": {"accountId": 2}
                    }
                },
                "data": {"key": "value"},
                "gpid": "/1234/slot"
            })),
            ..Default::default()
        }];

        let aliases = HashMap::new();
        let result = split_imps(&imps, &aliases).unwrap();

        // Each bidder should only see its own params
        let an_imp = &result["appnexus"][0];
        let an_ext = an_imp.ext.as_ref().unwrap();
        let an_bidder = &an_ext["prebid"]["bidder"];
        assert!(an_bidder.get("appnexus").is_some());
        assert!(an_bidder.get("rubicon").is_none());

        // Non-bidder fields preserved
        assert!(an_ext.get("data").is_some());
        assert!(an_ext.get("gpid").is_some());
    }

    #[test]
    fn test_get_request_aliases() {
        let req = BidRequest {
            ext: Some(json!({
                "prebid": {
                    "aliases": {
                        "districtm": "appnexus",
                        "brightroll": "yahoo"
                    }
                }
            })),
            ..Default::default()
        };

        let aliases = get_request_aliases(&req);
        assert_eq!(aliases.len(), 2);
        assert_eq!(aliases["districtm"], "appnexus");
        assert_eq!(aliases["brightroll"], "yahoo");
    }

    #[test]
    fn test_get_request_aliases_empty() {
        let req = BidRequest::default();
        let aliases = get_request_aliases(&req);
        assert!(aliases.is_empty());
    }

    #[test]
    fn test_build_request_ext_for_bidder() {
        let ext = json!({
            "prebid": {
                "bidder": {"appnexus": {"p": 1}, "rubicon": {"a": 2}},
                "aliases": {"dm": "appnexus"},
                "targeting": {"pricegranularity": "medium"},
                "data": {"bidders": ["appnexus"]}
            }
        });

        let result = build_request_ext_for_bidder(&ext, "appnexus", Some(&json!({"p": 1})));
        let prebid = &result["prebid"];

        // Bidder map removed
        assert!(prebid.get("bidder").is_none());
        // Aliases removed
        assert!(prebid.get("aliases").is_none());
        // Data removed
        assert!(prebid.get("data").is_none());
        // Targeting preserved
        assert!(prebid.get("targeting").is_some());
        // Bidder params set
        assert_eq!(prebid["bidderparams"]["appnexus"]["p"], json!(1));
    }

    #[test]
    fn test_extract_buyer_uids() {
        let req = BidRequest {
            user: Some(openrtb::User {
                ext: Some(json!({
                    "prebid": {
                        "buyeruids": {
                            "appnexus": "uid-123",
                            "rubicon": "uid-456"
                        }
                    }
                })),
                ..Default::default()
            }),
            ..Default::default()
        };

        let uids = extract_buyer_uids(&req);
        assert_eq!(uids.len(), 2);
        assert_eq!(uids["appnexus"], "uid-123");
        assert_eq!(uids["rubicon"], "uid-456");
    }

    #[test]
    fn test_extract_buyer_uids_empty() {
        let req = BidRequest::default();
        assert!(extract_buyer_uids(&req).is_empty());
    }

    #[test]
    fn test_prepare_user_for_bidder_from_buyer_uids() {
        let mut user = Some(openrtb::User::default());
        let mut buyer_uids = HashMap::new();
        buyer_uids.insert("appnexus".to_string(), "uid-from-ext".to_string());

        prepare_user_for_bidder(&mut user, "appnexus", &buyer_uids, &HashMap::new());
        assert_eq!(
            user.as_ref().unwrap().buyeruid,
            Some("uid-from-ext".to_string())
        );
    }

    #[test]
    fn test_prepare_user_for_bidder_from_cookie() {
        let mut user = Some(openrtb::User::default());
        let mut cookie_uids = HashMap::new();
        cookie_uids.insert("appnexus".to_string(), "uid-from-cookie".to_string());

        prepare_user_for_bidder(&mut user, "appnexus", &HashMap::new(), &cookie_uids);
        assert_eq!(
            user.as_ref().unwrap().buyeruid,
            Some("uid-from-cookie".to_string())
        );
    }

    #[test]
    fn test_prepare_user_creates_user() {
        let mut user: Option<openrtb::User> = None;
        let mut buyer_uids = HashMap::new();
        buyer_uids.insert("appnexus".to_string(), "uid-new".to_string());

        prepare_user_for_bidder(&mut user, "appnexus", &buyer_uids, &HashMap::new());
        assert!(user.is_some());
        assert_eq!(user.unwrap().buyeruid, Some("uid-new".to_string()));
    }

    #[test]
    fn test_clean_buyer_uids() {
        let mut req = BidRequest {
            user: Some(openrtb::User {
                ext: Some(json!({
                    "prebid": {
                        "buyeruids": {"appnexus": "uid-123"},
                        "other": "kept"
                    }
                })),
                ..Default::default()
            }),
            ..Default::default()
        };

        clean_buyer_uids(&mut req);

        let ext = req.user.unwrap().ext.unwrap();
        assert!(ext["prebid"].get("buyeruids").is_none());
        assert_eq!(ext["prebid"]["other"], json!("kept"));
    }
}
