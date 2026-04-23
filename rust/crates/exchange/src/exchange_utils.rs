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

// ---------------------------------------------------------------------------
// Privacy enforcement per-bidder
// ---------------------------------------------------------------------------

/// Privacy enforcement signals relevant to a single bidder request.
///
/// Mirrors the Go privacy enforcement checks scattered across
/// `cleanOpenRTBRequests` in `exchange/utils.go`.
#[derive(Debug, Clone, Default)]
pub struct PrivacyEnforcement {
    /// Whether CCPA (US Privacy) enforcement applies.
    pub ccpa: bool,
    /// Whether COPPA (children's privacy) enforcement applies.
    pub coppa: bool,
    /// Whether Limit-Ad-Tracking is set on the device.
    pub lmt: bool,
    /// Whether GDPR applies to this request.
    pub gdpr_applies: bool,
    /// The GDPR consent string, if present.
    pub gdpr_consent: Option<String>,
}

/// Returns `true` if the bidder should be completely blocked from bidding
/// due to privacy regulations.
///
/// Current rule: a bidder is blocked when COPPA is active **and** GDPR
/// applies without any consent string — i.e. there is no legal basis to
/// process the request at all.
pub fn is_bidder_blocked_by_privacy(
    privacy_enforcement: &PrivacyEnforcement,
    _bidder: &str,
) -> bool {
    // COPPA is the strictest: if COPPA applies together with GDPR and no
    // consent at all, the bidder cannot participate.
    if privacy_enforcement.coppa
        && privacy_enforcement.gdpr_applies
        && privacy_enforcement
            .gdpr_consent
            .as_ref()
            .map_or(true, |c| c.is_empty())
    {
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// Apply privacy scrubbing to a per-bidder request
// ---------------------------------------------------------------------------

/// Scrub PII from a bid request according to the active privacy enforcement
/// signals.
///
/// Logic (mirrors Go `cleanOpenRTBRequests` privacy path):
/// - **COPPA**: full device + user scrub (strongest).
/// - **GDPR without consent**: scrub all user/device IDs.
/// - **CCPA**: scrub user IDs only.
/// - **LMT**: scrub device IDs.
///
/// When `is_amp` is true the scrubbing is slightly more aggressive for geo
/// (truncates lat/lon).
pub fn apply_privacy(
    req: &mut openrtb::BidRequest,
    enforcement: &PrivacyEnforcement,
    is_amp: bool,
) {
    if enforcement.coppa {
        // Full scrub — device and user
        scrub_device(req, true);
        scrub_user(req, true);
        if is_amp {
            scrub_geo_precision(req);
        }
        return;
    }

    let gdpr_no_consent = enforcement.gdpr_applies
        && enforcement
            .gdpr_consent
            .as_ref()
            .map_or(true, |c| c.is_empty());

    if gdpr_no_consent {
        // Scrub identifiers on both device and user
        scrub_device(req, false);
        scrub_user(req, true);
        if is_amp {
            scrub_geo_precision(req);
        }
        return;
    }

    if enforcement.ccpa {
        scrub_user(req, false);
    }

    if enforcement.lmt {
        scrub_device(req, false);
    }
}

/// Remove identifying fields from the device object.
/// When `full` is true, also clears device-level geo.
fn scrub_device(req: &mut openrtb::BidRequest, full: bool) {
    if let Some(device) = &mut req.device {
        device.ifa = None;
        device.macsha1 = None;
        device.macmd5 = None;
        device.dpidsha1 = None;
        device.dpidmd5 = None;
        device.didsha1 = None;
        device.didmd5 = None;
        device.ip = None;
        device.ipv6 = None;
        if full {
            device.geo = None;
        }
    }
}

/// Remove identifying fields from the user object.
/// When `scrub_ids` is true, also clears user.id, buyeruid, and eids.
fn scrub_user(req: &mut openrtb::BidRequest, scrub_ids: bool) {
    if let Some(user) = &mut req.user {
        if scrub_ids {
            user.id = None;
            user.buyeruid = None;
            user.eids = None;
        }
        // Always clear yob and gender when any user scrub applies
        user.yob = None;
        user.gender = None;
    }
}

/// Truncate geo lat/lon to ~100 m precision (2 decimal places).
fn scrub_geo_precision(req: &mut openrtb::BidRequest) {
    fn truncate(val: f64) -> f64 {
        (val * 100.0).trunc() / 100.0
    }

    if let Some(device) = &mut req.device {
        if let Some(geo) = &mut device.geo {
            geo.lat = geo.lat.map(truncate);
            geo.lon = geo.lon.map(truncate);
        }
    }
    if let Some(user) = &mut req.user {
        if let Some(geo) = &mut user.geo {
            geo.lat = geo.lat.map(truncate);
            geo.lon = geo.lon.map(truncate);
        }
    }
}

// ---------------------------------------------------------------------------
// Multi-bid configuration builder
// ---------------------------------------------------------------------------

/// Per-bidder multi-bid configuration.
///
/// Mirrors Go `openrtb_ext.ExtMultiBid` (the fields used downstream).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiBidConfig {
    /// Maximum number of bids the bidder may return per imp.
    pub max_bids: u32,
    /// Prefix used to generate per-bid targeting keys (e.g. "appnexus_").
    pub target_bidder_code_prefix: String,
}

/// Parse `req.ext.prebid.multibid` and build a bidder-name-keyed map.
///
/// The JSON shape expected is:
/// ```json
/// { "prebid": { "multibid": [
///     { "bidder": "appnexus", "maxBids": 3, "targetBidderCodePrefix": "an" },
///     { "bidders": ["rubicon","ix"], "maxBids": 2 }
/// ]}}
/// ```
///
/// Mirrors Go `buildMultiBidMap` in `exchange/exchange.go`.
pub fn build_multi_bid_map(
    ext: Option<&serde_json::Value>,
) -> HashMap<String, MultiBidConfig> {
    let mut map = HashMap::new();

    let arr = ext
        .and_then(|e| e.get("prebid"))
        .and_then(|p| p.get("multibid"))
        .and_then(|m| m.as_array());

    let arr = match arr {
        Some(a) => a,
        None => return map,
    };

    for entry in arr {
        let max_bids = entry
            .get("maxBids")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u32;

        let prefix = entry
            .get("targetBidderCodePrefix")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Single bidder form
        if let Some(bidder) = entry.get("bidder").and_then(|b| b.as_str()) {
            let bidder_lower = bidder.to_lowercase();
            map.insert(
                bidder_lower,
                MultiBidConfig {
                    max_bids,
                    target_bidder_code_prefix: prefix.clone(),
                },
            );
            continue;
        }

        // Multiple bidders form
        if let Some(bidders) = entry.get("bidders").and_then(|b| b.as_array()) {
            for b in bidders {
                if let Some(name) = b.as_str() {
                    let bidder_lower = name.to_lowercase();
                    map.insert(
                        bidder_lower,
                        MultiBidConfig {
                            max_bids,
                            target_bidder_code_prefix: prefix.clone(),
                        },
                    );
                }
            }
        }
    }

    map
}

// ---------------------------------------------------------------------------
// Bidder preferred media type map builder
// ---------------------------------------------------------------------------

/// Build a map of bidder name to preferred media type string.
///
/// Reads `req.ext.prebid.biddercontrols.<bidder>.preferredMediaType`.
///
/// Mirrors Go `getBidderPreferredMediaTypeMap` (request-level portion) in
/// `exchange/exchange.go`.
pub fn get_bidder_preferred_media_type(
    ext: Option<&serde_json::Value>,
) -> HashMap<String, String> {
    let mut map = HashMap::new();

    let controls = ext
        .and_then(|e| e.get("prebid"))
        .and_then(|p| p.get("biddercontrols"))
        .and_then(|bc| bc.as_object());

    if let Some(controls) = controls {
        for (bidder, val) in controls {
            if let Some(pref) = val
                .get("preferredMediaType")
                .and_then(|v| v.as_str())
            {
                if !pref.is_empty() {
                    map.insert(bidder.clone(), pref.to_string());
                }
            }
        }
    }

    map
}

// ---------------------------------------------------------------------------
// Ads cert check
// ---------------------------------------------------------------------------

/// Returns `true` only when **both** the request-level experiment flag and
/// the bidder-level config flag enable ads certification signing.
///
/// Mirrors Go `isAdsCertEnabled` in `exchange/exchange.go`.
pub fn is_ads_cert_enabled(
    ext: Option<&serde_json::Value>,
    bidder_ads_cert: bool,
) -> bool {
    if !bidder_ads_cert {
        return false;
    }

    let request_enabled = ext
        .and_then(|e| e.get("prebid"))
        .and_then(|p| p.get("experiment"))
        .and_then(|ex| ex.get("adsCert"))
        .and_then(|ac| ac.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    request_enabled
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

    // -----------------------------------------------------------------------
    // Privacy enforcement tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_bidder_blocked_coppa_gdpr_no_consent() {
        let enforcement = PrivacyEnforcement {
            coppa: true,
            gdpr_applies: true,
            gdpr_consent: None,
            ..Default::default()
        };
        assert!(is_bidder_blocked_by_privacy(&enforcement, "appnexus"));
    }

    #[test]
    fn test_is_bidder_blocked_coppa_gdpr_empty_consent() {
        let enforcement = PrivacyEnforcement {
            coppa: true,
            gdpr_applies: true,
            gdpr_consent: Some(String::new()),
            ..Default::default()
        };
        assert!(is_bidder_blocked_by_privacy(&enforcement, "appnexus"));
    }

    #[test]
    fn test_is_bidder_not_blocked_coppa_only() {
        let enforcement = PrivacyEnforcement {
            coppa: true,
            gdpr_applies: false,
            ..Default::default()
        };
        assert!(!is_bidder_blocked_by_privacy(&enforcement, "appnexus"));
    }

    #[test]
    fn test_is_bidder_not_blocked_gdpr_with_consent() {
        let enforcement = PrivacyEnforcement {
            coppa: true,
            gdpr_applies: true,
            gdpr_consent: Some("BOvalid".to_string()),
            ..Default::default()
        };
        assert!(!is_bidder_blocked_by_privacy(&enforcement, "appnexus"));
    }

    #[test]
    fn test_is_bidder_not_blocked_no_enforcement() {
        let enforcement = PrivacyEnforcement::default();
        assert!(!is_bidder_blocked_by_privacy(&enforcement, "appnexus"));
    }

    // -----------------------------------------------------------------------
    // apply_privacy tests
    // -----------------------------------------------------------------------

    fn make_request_with_user_and_device() -> BidRequest {
        BidRequest {
            device: Some(openrtb::Device {
                ifa: Some("ifa-123".to_string()),
                ip: Some("1.2.3.4".to_string()),
                didsha1: Some("sha1".to_string()),
                geo: Some(openrtb::Geo {
                    lat: Some(40.7128),
                    lon: Some(-74.0060),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            user: Some(openrtb::User {
                id: Some("user-1".to_string()),
                buyeruid: Some("buid-1".to_string()),
                yob: Some(1990),
                gender: Some("M".to_string()),
                eids: Some(vec![openrtb::Eid {
                    source: Some("test.com".to_string()),
                    ..Default::default()
                }]),
                geo: Some(openrtb::Geo {
                    lat: Some(51.5074),
                    lon: Some(-0.1278),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn test_apply_privacy_coppa_scrubs_all() {
        let mut req = make_request_with_user_and_device();
        let enforcement = PrivacyEnforcement {
            coppa: true,
            ..Default::default()
        };
        apply_privacy(&mut req, &enforcement, false);

        let device = req.device.as_ref().unwrap();
        assert!(device.ifa.is_none());
        assert!(device.ip.is_none());
        assert!(device.geo.is_none());

        let user = req.user.as_ref().unwrap();
        assert!(user.id.is_none());
        assert!(user.buyeruid.is_none());
        assert!(user.eids.is_none());
        assert!(user.yob.is_none());
        assert!(user.gender.is_none());
    }

    #[test]
    fn test_apply_privacy_coppa_amp_truncates_geo() {
        let mut req = make_request_with_user_and_device();
        let enforcement = PrivacyEnforcement {
            coppa: true,
            ..Default::default()
        };
        apply_privacy(&mut req, &enforcement, true);

        // Device geo is fully removed under COPPA
        let device = req.device.as_ref().unwrap();
        assert!(device.geo.is_none());

        // User geo should be truncated
        let user_geo = req.user.as_ref().unwrap().geo.as_ref().unwrap();
        assert_eq!(user_geo.lat, Some(51.50));
        assert_eq!(user_geo.lon, Some(-0.12));
    }

    #[test]
    fn test_apply_privacy_gdpr_no_consent() {
        let mut req = make_request_with_user_and_device();
        let enforcement = PrivacyEnforcement {
            gdpr_applies: true,
            gdpr_consent: None,
            ..Default::default()
        };
        apply_privacy(&mut req, &enforcement, false);

        let device = req.device.as_ref().unwrap();
        assert!(device.ifa.is_none());
        // Geo preserved (not full scrub)
        assert!(device.geo.is_some());

        let user = req.user.as_ref().unwrap();
        assert!(user.id.is_none());
        assert!(user.buyeruid.is_none());
    }

    #[test]
    fn test_apply_privacy_ccpa_scrubs_user_ids_only() {
        let mut req = make_request_with_user_and_device();
        let enforcement = PrivacyEnforcement {
            ccpa: true,
            ..Default::default()
        };
        apply_privacy(&mut req, &enforcement, false);

        // Device should be untouched
        let device = req.device.as_ref().unwrap();
        assert!(device.ifa.is_some());

        // User yob/gender scrubbed, but not full ID scrub
        let user = req.user.as_ref().unwrap();
        assert!(user.yob.is_none());
        assert!(user.gender.is_none());
        // id/buyeruid preserved under CCPA (only scrub_ids=false path)
        assert!(user.id.is_some());
    }

    #[test]
    fn test_apply_privacy_lmt_scrubs_device() {
        let mut req = make_request_with_user_and_device();
        let enforcement = PrivacyEnforcement {
            lmt: true,
            ..Default::default()
        };
        apply_privacy(&mut req, &enforcement, false);

        let device = req.device.as_ref().unwrap();
        assert!(device.ifa.is_none());
        assert!(device.ip.is_none());

        // User untouched
        let user = req.user.as_ref().unwrap();
        assert!(user.id.is_some());
        assert!(user.yob.is_some());
    }

    #[test]
    fn test_apply_privacy_none() {
        let mut req = make_request_with_user_and_device();
        let enforcement = PrivacyEnforcement::default();
        apply_privacy(&mut req, &enforcement, false);

        assert!(req.device.as_ref().unwrap().ifa.is_some());
        assert!(req.user.as_ref().unwrap().id.is_some());
    }

    // -----------------------------------------------------------------------
    // Multi-bid map tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_multi_bid_map_single_bidder() {
        let ext = serde_json::json!({
            "prebid": {
                "multibid": [
                    {
                        "bidder": "appnexus",
                        "maxBids": 3,
                        "targetBidderCodePrefix": "an"
                    }
                ]
            }
        });

        let map = build_multi_bid_map(Some(&ext));
        assert_eq!(map.len(), 1);
        let cfg = map.get("appnexus").unwrap();
        assert_eq!(cfg.max_bids, 3);
        assert_eq!(cfg.target_bidder_code_prefix, "an");
    }

    #[test]
    fn test_build_multi_bid_map_multiple_bidders() {
        let ext = serde_json::json!({
            "prebid": {
                "multibid": [
                    {
                        "bidders": ["rubicon", "ix"],
                        "maxBids": 2
                    }
                ]
            }
        });

        let map = build_multi_bid_map(Some(&ext));
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("rubicon").unwrap().max_bids, 2);
        assert_eq!(map.get("ix").unwrap().max_bids, 2);
    }

    #[test]
    fn test_build_multi_bid_map_none() {
        assert!(build_multi_bid_map(None).is_empty());
    }

    #[test]
    fn test_build_multi_bid_map_no_multibid_key() {
        let ext = serde_json::json!({"prebid": {}});
        assert!(build_multi_bid_map(Some(&ext)).is_empty());
    }

    #[test]
    fn test_build_multi_bid_map_case_insensitive() {
        let ext = serde_json::json!({
            "prebid": {
                "multibid": [
                    {"bidder": "AppNexus", "maxBids": 5}
                ]
            }
        });
        let map = build_multi_bid_map(Some(&ext));
        assert!(map.contains_key("appnexus"));
        assert_eq!(map.get("appnexus").unwrap().max_bids, 5);
    }

    // -----------------------------------------------------------------------
    // Bidder preferred media type tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_bidder_preferred_media_type_present() {
        let ext = serde_json::json!({
            "prebid": {
                "biddercontrols": {
                    "appnexus": {"preferredMediaType": "video"},
                    "rubicon": {"preferredMediaType": "banner"}
                }
            }
        });

        let map = get_bidder_preferred_media_type(Some(&ext));
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("appnexus").unwrap(), "video");
        assert_eq!(map.get("rubicon").unwrap(), "banner");
    }

    #[test]
    fn test_get_bidder_preferred_media_type_empty() {
        assert!(get_bidder_preferred_media_type(None).is_empty());
    }

    #[test]
    fn test_get_bidder_preferred_media_type_skips_empty_value() {
        let ext = serde_json::json!({
            "prebid": {
                "biddercontrols": {
                    "appnexus": {"preferredMediaType": ""},
                    "rubicon": {"preferredMediaType": "native"}
                }
            }
        });
        let map = get_bidder_preferred_media_type(Some(&ext));
        assert_eq!(map.len(), 1);
        assert!(!map.contains_key("appnexus"));
        assert_eq!(map.get("rubicon").unwrap(), "native");
    }

    #[test]
    fn test_get_bidder_preferred_media_type_no_controls() {
        let ext = serde_json::json!({"prebid": {}});
        assert!(get_bidder_preferred_media_type(Some(&ext)).is_empty());
    }

    // -----------------------------------------------------------------------
    // Ads cert tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_ads_cert_enabled_both_true() {
        let ext = serde_json::json!({
            "prebid": {
                "experiment": {
                    "adsCert": {
                        "enabled": true
                    }
                }
            }
        });
        assert!(is_ads_cert_enabled(Some(&ext), true));
    }

    #[test]
    fn test_is_ads_cert_disabled_request_false() {
        let ext = serde_json::json!({
            "prebid": {
                "experiment": {
                    "adsCert": {
                        "enabled": false
                    }
                }
            }
        });
        assert!(!is_ads_cert_enabled(Some(&ext), true));
    }

    #[test]
    fn test_is_ads_cert_disabled_bidder_false() {
        let ext = serde_json::json!({
            "prebid": {
                "experiment": {
                    "adsCert": {
                        "enabled": true
                    }
                }
            }
        });
        assert!(!is_ads_cert_enabled(Some(&ext), false));
    }

    #[test]
    fn test_is_ads_cert_disabled_no_ext() {
        assert!(!is_ads_cert_enabled(None, true));
    }

    #[test]
    fn test_is_ads_cert_disabled_missing_field() {
        let ext = serde_json::json!({"prebid": {}});
        assert!(!is_ads_cert_enabled(Some(&ext), true));
    }

    #[test]
    fn test_is_ads_cert_disabled_both_false() {
        let ext = serde_json::json!({
            "prebid": {
                "experiment": {
                    "adsCert": {
                        "enabled": false
                    }
                }
            }
        });
        assert!(!is_ads_cert_enabled(Some(&ext), false));
    }
}
