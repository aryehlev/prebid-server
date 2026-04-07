//! Bid response building utilities.
//!
//! Mirrors Go `endpoints/openrtb2/auction.go` response assembly and
//! `exchange/exchange.go` response building.
//!
//! Provides functions for assembling the final OpenRTB bid response
//! from auction results, applying targeting, and enriching extensions.

use std::collections::HashMap;

use rand::seq::SliceRandom;
use serde_json::{json, Value};

use openrtb::{Bid, BidResponse, Imp, SeatBid};

// ---------------------------------------------------------------------------
// Response assembly
// ---------------------------------------------------------------------------

/// Build a BidResponse from auction results.
///
/// Assembles seat bids from per-bidder responses and sets the response ID
/// and currency.
pub fn build_bid_response(
    request_id: &str,
    seat_bids: Vec<SeatBid>,
    currency: &str,
) -> BidResponse {
    BidResponse {
        id: request_id.to_string(),
        seatbid: seat_bids,
        cur: if currency.is_empty() {
            Some("USD".to_string())
        } else {
            Some(currency.to_string())
        },
        ..Default::default()
    }
}

/// Build a SeatBid from a bidder's bids.
pub fn build_seat_bid(bidder_name: &str, bids: Vec<Bid>) -> SeatBid {
    SeatBid {
        bid: bids,
        seat: Some(bidder_name.to_string()),
        ..Default::default()
    }
}

/// Set SeatNonBid data in the response extension.
///
/// Mirrors Go `setSeatNonBidRaw`.
pub fn set_seat_non_bid(
    response: &mut BidResponse,
    seat_non_bids: &[openrtb_ext::SeatNonBid],
) {
    if seat_non_bids.is_empty() {
        return;
    }

    let ext = response
        .ext
        .get_or_insert_with(|| Value::Object(Default::default()));

    if let Some(obj) = ext.as_object_mut() {
        if let Ok(val) = serde_json::to_value(seat_non_bids) {
            let mut prebid = obj
                .remove("prebid")
                .unwrap_or_else(|| Value::Object(Default::default()));
            if let Some(prebid_obj) = prebid.as_object_mut() {
                prebid_obj.insert("seatnonbid".to_string(), val);
            }
            obj.insert("prebid".to_string(), prebid);
        }
    }
}

/// Add response time and error information to the response extension.
///
/// Mirrors Go response extension enrichment.
pub fn set_response_ext_info(
    response: &mut BidResponse,
    response_time_millis: HashMap<String, u64>,
    errors: HashMap<String, Vec<String>>,
    warnings: HashMap<String, Vec<String>>,
) {
    let ext = response
        .ext
        .get_or_insert_with(|| Value::Object(Default::default()));

    if let Some(obj) = ext.as_object_mut() {
        // Set response time per bidder
        if !response_time_millis.is_empty() {
            let mut responsetimemillis = serde_json::Map::new();
            for (bidder, ms) in &response_time_millis {
                responsetimemillis.insert(bidder.clone(), json!(ms));
            }
            obj.insert(
                "responsetimemillis".to_string(),
                Value::Object(responsetimemillis),
            );
        }

        // Set errors per bidder
        if !errors.is_empty() {
            let mut errs_map = serde_json::Map::new();
            for (bidder, msgs) in &errors {
                let err_objs: Vec<Value> = msgs
                    .iter()
                    .map(|msg| {
                        json!({
                            "code": 999,
                            "message": msg
                        })
                    })
                    .collect();
                errs_map.insert(bidder.clone(), Value::Array(err_objs));
            }
            obj.insert("errors".to_string(), Value::Object(errs_map));
        }

        // Set warnings per bidder
        if !warnings.is_empty() {
            let mut warns_map = serde_json::Map::new();
            for (bidder, msgs) in &warnings {
                let warn_objs: Vec<Value> = msgs
                    .iter()
                    .map(|msg| {
                        json!({
                            "code": 999,
                            "message": msg
                        })
                    })
                    .collect();
                warns_map.insert(bidder.clone(), Value::Array(warn_objs));
            }
            obj.insert("warnings".to_string(), Value::Object(warns_map));
        }
    }
}

/// Add debug information to the response extension.
pub fn set_response_debug(
    response: &mut BidResponse,
    http_calls: HashMap<String, Vec<Value>>,
) {
    if http_calls.is_empty() {
        return;
    }

    let ext = response
        .ext
        .get_or_insert_with(|| Value::Object(Default::default()));

    if let Some(obj) = ext.as_object_mut() {
        let mut debug_obj = serde_json::Map::new();
        let mut httpcalls = serde_json::Map::new();
        for (bidder, calls) in &http_calls {
            httpcalls.insert(bidder.clone(), Value::Array(calls.clone()));
        }
        debug_obj.insert("httpcalls".to_string(), Value::Object(httpcalls));
        obj.insert("debug".to_string(), Value::Object(debug_obj));
    }
}

/// Enrich bid extension with prebid-specific fields.
///
/// Adds `ext.prebid.targeting`, `ext.prebid.type`, `ext.prebid.events`, etc.
pub fn enrich_bid_ext(
    bid: &mut Bid,
    bid_type: &str,
    targeting: Option<&HashMap<String, String>>,
    events: Option<&Value>,
) {
    let ext = bid
        .ext
        .get_or_insert_with(|| Value::Object(Default::default()));

    if let Some(obj) = ext.as_object_mut() {
        let prebid = obj
            .entry("prebid".to_string())
            .or_insert_with(|| Value::Object(Default::default()));

        if let Some(prebid_obj) = prebid.as_object_mut() {
            // Set bid type
            prebid_obj.insert("type".to_string(), json!(bid_type));

            // Set targeting
            if let Some(tgt) = targeting {
                if let Ok(val) = serde_json::to_value(tgt) {
                    prebid_obj.insert("targeting".to_string(), val);
                }
            }

            // Set events
            if let Some(ev) = events {
                prebid_obj.insert("events".to_string(), ev.clone());
            }
        }
    }
}

/// Set the "wurl" (win URL) on a bid's extension JSON.
///
/// Mirrors Go `modifyBidJSON` for non-video bids.
pub fn set_win_url(bid_ext: &mut Value, win_url: &str) {
    if win_url.is_empty() {
        return;
    }
    if let Some(obj) = bid_ext.as_object_mut() {
        obj.insert("wurl".to_string(), json!(win_url));
    }
}

// ---------------------------------------------------------------------------
// Bid extension helpers (ported from Go exchange/exchange.go)
// ---------------------------------------------------------------------------

/// Build the bid extension JSON by merging existing ext with prebid data.
///
/// Mirrors Go `makeBidExtJSON`. Adds `origbidcpm`, `origbidcur`, and merges
/// the `prebid` object. If the prebid value does not contain `meta` but the
/// existing ext does, the meta is carried forward.
pub fn make_bid_ext_json(
    ext: Option<&Value>,
    prebid: &Value,
    original_bid_cpm: f64,
    original_bid_cur: &str,
) -> Value {
    let mut ext_map = match ext {
        Some(v) if v.is_object() => v.clone(),
        _ => json!({}),
    };

    // ext.origbidcpm
    if original_bid_cpm >= 0.0 {
        ext_map["origbidcpm"] = json!(original_bid_cpm);
    }

    // ext.origbidcur
    if !original_bid_cur.is_empty() {
        ext_map["origbidcur"] = json!(original_bid_cur);
    }

    // Merge prebid – carry forward meta from existing ext if not in prebid
    let mut prebid_merged = prebid.clone();
    if prebid_merged.get("meta").is_none() {
        if let Some(existing_meta) = ext_map
            .get("prebid")
            .and_then(|p| p.get("meta"))
        {
            prebid_merged["meta"] = existing_meta.clone();
        }
    }

    // Ensure meta exists (even if empty)
    if prebid_merged.get("meta").is_none() {
        prebid_merged["meta"] = json!({});
    }

    ext_map["prebid"] = prebid_merged;
    ext_map
}

/// Returns `false` if bid dimensions exceed the maximum allowed size.
///
/// Mirrors Go `validateBannerCreativeSize`.
pub fn validate_banner_creative_size(
    bid_w: i64,
    bid_h: i64,
    max_w: i64,
    max_h: i64,
) -> bool {
    !(bid_w > max_w || bid_h > max_h)
}

/// Returns `false` if the AdM markup contains insecure HTTP references
/// without any secure HTTPS references.
///
/// Mirrors Go `validateBidAdM`.
pub fn validate_bid_adm_secure(adm: &str) -> bool {
    let has_insecure = adm.contains("http:") || adm.contains("http%3A");
    let has_secure = adm.contains("https:") || adm.contains("https%3A");

    if has_insecure && !has_secure {
        return false;
    }
    true
}

/// Construct a cache URL from its components.
///
/// Mirrors Go `buildCacheURL`. If host or path is empty, returns an empty
/// string. When scheme is empty the leading `//` is trimmed.
pub fn build_cache_url(scheme: &str, host: &str, path: &str, uuid: &str) -> String {
    if host.is_empty() || path.is_empty() {
        return String::new();
    }

    // Simple percent-encoding for the UUID (only alphanumeric and hyphens are expected)
    let encoded_uuid: String = uuid
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
                c.to_string()
            } else {
                format!("%{:02X}", c as u8)
            }
        })
        .collect();
    let query = format!("uuid={}", encoded_uuid);
    if scheme.is_empty() {
        format!("{path}?{query}", path = format!("{host}{path}"), query = query)
    } else {
        format!("{scheme}://{host}{path}?{query}")
    }
}

/// Return a shuffled copy of the bidder names list.
///
/// Mirrors Go `listBiddersWithRequests` which randomises the adapter order
/// to make the auction more fair.
pub fn list_bidders_with_requests(bidder_names: &[String]) -> Vec<String> {
    let mut names = bidder_names.to_vec();
    let mut rng = rand::thread_rng();
    names.shuffle(&mut rng);
    names
}

/// Extract the integration type from `req.ext.prebid.integration`.
///
/// Mirrors Go `getIntegrationType`. Returns an empty string when the field
/// is absent or not a string.
pub fn get_integration_type(ext: Option<&Value>) -> String {
    ext.and_then(|e| e.get("prebid"))
        .and_then(|p| p.get("integration"))
        .and_then(|i| i.as_str())
        .unwrap_or("")
        .to_string()
}

/// Return the list of media types present on an impression.
///
/// Checks for banner, video, audio, and native objects on the `Imp`.
pub fn get_imp_media_types(imp: &Imp) -> Vec<String> {
    let mut types = Vec::new();
    if imp.banner.is_some() {
        types.push("banner".to_string());
    }
    if imp.video.is_some() {
        types.push("video".to_string());
    }
    if imp.audio.is_some() {
        types.push("audio".to_string());
    }
    if imp.native.is_some() {
        types.push("native".to_string());
    }
    types
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_bid_response() {
        let bids = vec![Bid {
            id: "bid-1".to_string(),
            impid: "imp-1".to_string(),
            price: 2.5,
            ..Default::default()
        }];
        let seat_bids = vec![build_seat_bid("appnexus", bids)];
        let response = build_bid_response("req-1", seat_bids, "USD");

        assert_eq!(response.id, "req-1");
        assert_eq!(response.cur, Some("USD".to_string()));
        assert_eq!(response.seatbid.len(), 1);
        assert_eq!(
            response.seatbid[0].seat,
            Some("appnexus".to_string())
        );
    }

    #[test]
    fn test_build_bid_response_default_currency() {
        let response = build_bid_response("req-1", vec![], "");
        assert_eq!(response.cur, Some("USD".to_string()));
    }

    #[test]
    fn test_set_seat_non_bid() {
        let mut response = BidResponse {
            id: "req-1".to_string(),
            ..Default::default()
        };

        let seat_non_bids = vec![openrtb_ext::SeatNonBid {
            seat: "appnexus".to_string(),
            nonbid: vec![],
            ext: None,
        }];

        set_seat_non_bid(&mut response, &seat_non_bids);
        let ext = response.ext.unwrap();
        assert!(ext["prebid"]["seatnonbid"].is_array());
    }

    #[test]
    fn test_set_seat_non_bid_empty() {
        let mut response = BidResponse {
            id: "req-1".to_string(),
            ..Default::default()
        };
        set_seat_non_bid(&mut response, &[]);
        assert!(response.ext.is_none());
    }

    #[test]
    fn test_set_response_ext_info() {
        let mut response = BidResponse {
            id: "req-1".to_string(),
            ..Default::default()
        };

        let mut times = HashMap::new();
        times.insert("appnexus".to_string(), 150u64);
        times.insert("rubicon".to_string(), 200u64);

        let mut errors = HashMap::new();
        errors.insert(
            "appnexus".to_string(),
            vec!["timeout".to_string()],
        );

        set_response_ext_info(&mut response, times, errors, HashMap::new());

        let ext = response.ext.unwrap();
        assert_eq!(ext["responsetimemillis"]["appnexus"], json!(150));
        assert_eq!(ext["responsetimemillis"]["rubicon"], json!(200));
        assert!(ext["errors"]["appnexus"].is_array());
    }

    #[test]
    fn test_set_response_debug() {
        let mut response = BidResponse {
            id: "req-1".to_string(),
            ..Default::default()
        };

        let mut calls = HashMap::new();
        calls.insert(
            "appnexus".to_string(),
            vec![json!({"uri": "https://example.com", "status": 200})],
        );

        set_response_debug(&mut response, calls);

        let ext = response.ext.unwrap();
        assert!(ext["debug"]["httpcalls"]["appnexus"].is_array());
    }

    #[test]
    fn test_enrich_bid_ext() {
        let mut bid = Bid {
            id: "bid-1".to_string(),
            impid: "imp-1".to_string(),
            price: 2.5,
            ..Default::default()
        };

        let mut targeting = HashMap::new();
        targeting.insert("hb_pb".to_string(), "2.50".to_string());
        targeting.insert("hb_bidder".to_string(), "appnexus".to_string());

        enrich_bid_ext(&mut bid, "banner", Some(&targeting), None);

        let ext = bid.ext.unwrap();
        assert_eq!(ext["prebid"]["type"], json!("banner"));
        assert_eq!(ext["prebid"]["targeting"]["hb_pb"], json!("2.50"));
    }

    #[test]
    fn test_set_win_url() {
        let mut ext = json!({});
        set_win_url(&mut ext, "https://example.com/win");
        assert_eq!(ext["wurl"], json!("https://example.com/win"));
    }

    #[test]
    fn test_set_win_url_empty() {
        let mut ext = json!({});
        set_win_url(&mut ext, "");
        assert!(ext.get("wurl").is_none());
    }

    // -----------------------------------------------------------------------
    // make_bid_ext_json
    // -----------------------------------------------------------------------

    #[test]
    fn test_make_bid_ext_json_no_existing_ext() {
        let prebid = json!({"type": "banner"});
        let result = make_bid_ext_json(None, &prebid, 1.23, "USD");
        assert_eq!(result["origbidcpm"], json!(1.23));
        assert_eq!(result["origbidcur"], json!("USD"));
        assert_eq!(result["prebid"]["type"], json!("banner"));
        assert!(result["prebid"]["meta"].is_object());
    }

    #[test]
    fn test_make_bid_ext_json_with_existing_ext() {
        let existing = json!({
            "foo": "bar",
            "prebid": {"meta": {"advertiserDomains": ["example.com"]}}
        });
        let prebid = json!({"type": "video"});
        let result = make_bid_ext_json(Some(&existing), &prebid, 2.0, "EUR");
        assert_eq!(result["foo"], json!("bar"));
        assert_eq!(result["origbidcpm"], json!(2.0));
        assert_eq!(result["origbidcur"], json!("EUR"));
        assert_eq!(
            result["prebid"]["meta"]["advertiserDomains"],
            json!(["example.com"])
        );
    }

    #[test]
    fn test_make_bid_ext_json_prebid_has_meta() {
        let existing = json!({"prebid": {"meta": {"old": true}}});
        let prebid = json!({"type": "banner", "meta": {"new": true}});
        let result = make_bid_ext_json(Some(&existing), &prebid, 1.0, "");
        assert_eq!(result["prebid"]["meta"]["new"], json!(true));
        assert!(result["prebid"]["meta"].get("old").is_none());
    }

    #[test]
    fn test_make_bid_ext_json_negative_cpm() {
        let result = make_bid_ext_json(None, &json!({}), -1.0, "USD");
        assert!(result.get("origbidcpm").is_none());
    }

    // -----------------------------------------------------------------------
    // validate_banner_creative_size
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_banner_creative_size_within_limits() {
        assert!(validate_banner_creative_size(300, 250, 1024, 768));
    }

    #[test]
    fn test_validate_banner_creative_size_exceeds_width() {
        assert!(!validate_banner_creative_size(1025, 250, 1024, 768));
    }

    #[test]
    fn test_validate_banner_creative_size_exceeds_height() {
        assert!(!validate_banner_creative_size(300, 769, 1024, 768));
    }

    #[test]
    fn test_validate_banner_creative_size_exact() {
        assert!(validate_banner_creative_size(1024, 768, 1024, 768));
    }

    // -----------------------------------------------------------------------
    // validate_bid_adm_secure
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_bid_adm_secure_clean() {
        assert!(validate_bid_adm_secure("https://example.com/ad"));
    }

    #[test]
    fn test_validate_bid_adm_secure_insecure_only() {
        assert!(!validate_bid_adm_secure("http://example.com/ad"));
    }

    #[test]
    fn test_validate_bid_adm_secure_encoded_insecure() {
        assert!(!validate_bid_adm_secure("http%3A//example.com/ad"));
    }

    #[test]
    fn test_validate_bid_adm_secure_mixed() {
        assert!(validate_bid_adm_secure(
            "https://example.com/ad http://tracker.com"
        ));
    }

    #[test]
    fn test_validate_bid_adm_secure_empty() {
        assert!(validate_bid_adm_secure(""));
    }

    // -----------------------------------------------------------------------
    // build_cache_url
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_cache_url_full() {
        let url = build_cache_url("https", "cache.example.com", "/cache", "abc-123");
        assert_eq!(url, "https://cache.example.com/cache?uuid=abc-123");
    }

    #[test]
    fn test_build_cache_url_no_scheme() {
        let url = build_cache_url("", "cache.example.com", "/cache", "abc-123");
        assert_eq!(url, "cache.example.com/cache?uuid=abc-123");
    }

    #[test]
    fn test_build_cache_url_empty_host() {
        assert_eq!(build_cache_url("https", "", "/cache", "abc-123"), "");
    }

    #[test]
    fn test_build_cache_url_empty_path() {
        assert_eq!(
            build_cache_url("https", "cache.example.com", "", "abc-123"),
            ""
        );
    }

    // -----------------------------------------------------------------------
    // list_bidders_with_requests
    // -----------------------------------------------------------------------

    #[test]
    fn test_list_bidders_with_requests_preserves_elements() {
        let names: Vec<String> =
            vec!["appnexus".into(), "rubicon".into(), "pubmatic".into()];
        let result = list_bidders_with_requests(&names);
        assert_eq!(result.len(), 3);
        for n in &names {
            assert!(result.contains(n));
        }
    }

    #[test]
    fn test_list_bidders_with_requests_empty() {
        assert!(list_bidders_with_requests(&[]).is_empty());
    }

    // -----------------------------------------------------------------------
    // get_integration_type
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_integration_type_present() {
        let ext = json!({"prebid": {"integration": "pbjs"}});
        assert_eq!(get_integration_type(Some(&ext)), "pbjs");
    }

    #[test]
    fn test_get_integration_type_missing() {
        let ext = json!({"prebid": {}});
        assert_eq!(get_integration_type(Some(&ext)), "");
    }

    #[test]
    fn test_get_integration_type_none() {
        assert_eq!(get_integration_type(None), "");
    }

    // -----------------------------------------------------------------------
    // get_imp_media_types
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_imp_media_types_all() {
        let imp = Imp {
            id: "imp-1".to_string(),
            banner: Some(Default::default()),
            video: Some(Default::default()),
            audio: Some(Default::default()),
            native: Some(Default::default()),
            ..Default::default()
        };
        let types = get_imp_media_types(&imp);
        assert_eq!(types, vec!["banner", "video", "audio", "native"]);
    }

    #[test]
    fn test_get_imp_media_types_none() {
        let imp = Imp {
            id: "imp-1".to_string(),
            ..Default::default()
        };
        assert!(get_imp_media_types(&imp).is_empty());
    }

    #[test]
    fn test_get_imp_media_types_partial() {
        let imp = Imp {
            id: "imp-1".to_string(),
            video: Some(Default::default()),
            native: Some(Default::default()),
            ..Default::default()
        };
        let types = get_imp_media_types(&imp);
        assert_eq!(types, vec!["video", "native"]);
    }
}
