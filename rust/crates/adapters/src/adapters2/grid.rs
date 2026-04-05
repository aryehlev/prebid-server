use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde_json::Value;

pub struct GridAdapter {
    pub endpoint: String,
}

impl GridAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp], content_type: Option<&str>) -> Result<BidType, BidderError> {
    // If the bid itself carries a content_type field, use it directly
    if let Some(ct) = content_type {
        if !ct.is_empty() {
            return match ct {
                "banner" => Ok(BidType::Banner),
                "video" => Ok(BidType::Video),
                "native" => Ok(BidType::Native),
                _ => Ok(BidType::Banner),
            };
        }
    }
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return Ok(BidType::Banner);
            } else if imp.video.is_some() {
                return Ok(BidType::Video);
            } else if imp.native.is_some() {
                return Ok(BidType::Native);
            }
            return Err(BidderError::BadServerResponse(format!(
                "Unknown impression type for ID: \"{}\"", imp_id
            )));
        }
    }
    Err(BidderError::BadServerResponse(format!(
        "Failed to find impression for ID: \"{}\"", imp_id
    )))
}

fn get_bid_meta(ext: &Option<Value>) -> Option<openrtb_ext::ExtBidPrebidMeta> {
    let ext = ext.as_ref()?;
    let demand_source = ext
        .get("bidder")
        .and_then(|b| b.get("grid"))
        .and_then(|g| g.get("demandSource"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !demand_source.is_empty() {
        Some(openrtb_ext::ExtBidPrebidMeta {
            network_name: Some(demand_source.to_string()),
            ..Default::default()
        })
    } else {
        None
    }
}

/// fixNative: for each impression with a native block, move native.request (a string)
/// to native.request_native (a parsed object or passthrough string).
/// This is required by Grid's API format.
fn fix_native(mut req: Value) -> Value {
    if let Some(imps) = req.get_mut("imp").and_then(|v| v.as_array_mut()) {
        for imp in imps.iter_mut() {
            if let Some(imp_obj) = imp.as_object_mut() {
                if let Some(native) = imp_obj.get_mut("native").and_then(|v| v.as_object_mut()) {
                    if let Some(request_str) = native.remove("request") {
                        // Try to parse the request string as JSON
                        if let Some(s) = request_str.as_str() {
                            let parsed: Value = serde_json::from_str(s)
                                .unwrap_or(Value::String(s.to_string()));
                            native.insert("request_native".to_string(), parsed);
                        } else {
                            // Put it back as-is under request_native
                            native.insert("request_native".to_string(), request_str);
                        }
                    }
                }
            }
        }
    }
    req
}

// ─── keyword enrichment types ───────────────────────────────────────────────

#[derive(Debug, Clone)]
struct KeywordSegment {
    name: String,
    value: String,
}

/// publisher key → list of publisher items
type KeywordsPublisher = HashMap<String, Vec<KeywordsPublisherItem>>;

#[derive(Debug, Clone)]
struct KeywordsPublisherItem {
    name: String,
    segments: Vec<KeywordSegment>,
}

/// section ("site"|"user") → KeywordsPublisher
type Keywords = HashMap<String, KeywordsPublisher>;

/// Convert a KeywordsPublisherItem to a serde_json::Value suitable for Grid's wire format.
fn publisher_item_to_value(item: &KeywordsPublisherItem) -> Value {
    let segments: Vec<Value> = item.segments.iter().map(|s| {
        serde_json::json!({"name": s.name, "value": s.value})
    }).collect();
    serde_json::json!({"name": item.name, "segments": segments})
}

/// Convert a Keywords map to a serde_json::Value object.
fn keywords_to_value(keywords: &Keywords) -> Value {
    let mut obj = serde_json::Map::new();
    for (section, publisher_map) in keywords {
        let mut pub_obj = serde_json::Map::new();
        for (pub_key, items) in publisher_map {
            let items_val: Vec<Value> = items.iter().map(publisher_item_to_value).collect();
            pub_obj.insert(pub_key.clone(), Value::Array(items_val));
        }
        obj.insert(section.clone(), Value::Object(pub_obj));
    }
    Value::Object(obj)
}

fn parse_ext_to_map(ext: &Value) -> serde_json::Map<String, Value> {
    if let Value::Object(m) = ext {
        m.clone()
    } else {
        serde_json::Map::new()
    }
}

fn extract_keywords_map(ext_map: &serde_json::Map<String, Value>) -> serde_json::Map<String, Value> {
    ext_map.get("keywords")
        .and_then(|v| if let Value::Object(m) = v { Some(m.clone()) } else { None })
        .unwrap_or_default()
}

fn extract_bidder_keywords_map(ext_map: &serde_json::Map<String, Value>) -> serde_json::Map<String, Value> {
    ext_map.get("bidder")
        .and_then(|v| if let Value::Object(m) = v { Some(m) } else { None })
        .map(|bidder_map| extract_keywords_map(bidder_map))
        .unwrap_or_default()
}

/// Parse a `segments` array from a publisher item map.
fn parse_segments_from_item(item_map: &serde_json::Map<String, Value>) -> Vec<KeywordSegment> {
    let mut segments = Vec::new();

    // First pass: extract explicit {name, value} objects from "segments" array
    if let Some(Value::Array(segs)) = item_map.get("segments") {
        for seg in segs {
            if let Value::Object(seg_map) = seg {
                let name = seg_map.get("name").and_then(|v| v.as_str());
                let value = seg_map.get("value").and_then(|v| v.as_str());
                if let (Some(n), Some(v)) = (name, value) {
                    segments.push(KeywordSegment { name: n.to_string(), value: v.to_string() });
                }
            }
        }
    }

    // Second pass: for each key in the item map (sorted), treat string-array values as segments
    let mut keys: Vec<&str> = item_map.keys().map(|k| k.as_str()).collect();
    keys.sort();
    for key in keys {
        if let Some(Value::Array(vals)) = item_map.get(key) {
            for v in vals {
                if let Some(s) = v.as_str() {
                    segments.push(KeywordSegment { name: key.to_string(), value: s.to_string() });
                }
            }
        }
    }

    segments
}

fn parse_keywords_from_section(section: &serde_json::Map<String, Value>) -> KeywordsPublisher {
    let mut publisher_map: KeywordsPublisher = HashMap::new();
    for (pub_key, pub_val) in section {
        if let Value::Array(items) = pub_val {
            for item_val in items {
                if let Value::Object(item_map) = item_val {
                    let name = match item_map.get("name").and_then(|v| v.as_str()) {
                        Some(n) => n.to_string(),
                        None => continue,
                    };
                    let segments = parse_segments_from_item(item_map);
                    if !segments.is_empty() {
                        publisher_map.entry(pub_key.clone())
                            .or_default()
                            .push(KeywordsPublisherItem { name, segments });
                    }
                }
            }
        }
    }
    publisher_map
}

fn parse_keywords_from_map(ext_keywords: &serde_json::Map<String, Value>) -> Keywords {
    let mut keywords: Keywords = HashMap::new();
    for (k, v) in ext_keywords {
        if k != "site" && k != "user" {
            continue;
        }
        if let Value::Object(section) = v {
            let pub_map = parse_keywords_from_section(section);
            keywords.insert(k.clone(), pub_map);
        }
    }
    keywords
}

/// Parse comma-separated openrtb keywords string into a Keywords structure.
fn parse_keywords_from_openrtb(keywords_str: &str, section: &str) -> Keywords {
    if keywords_str.is_empty() {
        return HashMap::new();
    }
    let segments: Vec<KeywordSegment> = keywords_str
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|v| KeywordSegment { name: "keywords".to_string(), value: v.to_string() })
        .collect();
    if segments.is_empty() {
        return HashMap::new();
    }
    let item = KeywordsPublisherItem { name: "keywords".to_string(), segments };
    let mut pub_map: KeywordsPublisher = HashMap::new();
    pub_map.insert("ortb2".to_string(), vec![item]);
    let mut kw: Keywords = HashMap::new();
    kw.insert(section.to_string(), pub_map);
    kw
}

/// Merge keywords from b into a. b items are prepended (match Go: `append(publisherValues, a[key][publisherKey]...)`).
fn merge_keywords(a: &mut Keywords, b: Keywords) {
    for (section, pub_map) in b {
        let section_entry = a.entry(section).or_default();
        for (pub_key, mut pub_values) in pub_map {
            let existing = section_entry.entry(pub_key).or_default();
            // Go does: a[key][publisherKey] = append(publisherValues, a[key][publisherKey]...)
            // i.e., new values (from b) come first
            pub_values.extend(existing.drain(..));
            *existing = pub_values;
        }
    }
}

/// Build consolidated keywords request ext, merging from:
/// - request.ext.keywords
/// - request.imp[0].ext.bidder.keywords
/// - request.user.keywords (comma-separated)
/// - request.site.keywords (comma-separated)
fn build_consolidated_keywords_req_ext(
    user_keywords: &str,
    site_keywords: &str,
    first_imp_ext: Option<&Value>,
    request_ext: Option<&Value>,
) -> Result<Option<Value>, BidderError> {
    let request_ext_val = request_ext.cloned().unwrap_or(Value::Object(serde_json::Map::new()));
    let first_imp_ext_val = first_imp_ext.cloned().unwrap_or(Value::Object(serde_json::Map::new()));

    let mut request_ext_map = parse_ext_to_map(&request_ext_val);
    let first_imp_ext_map = parse_ext_to_map(&first_imp_ext_val);

    let request_ext_keywords_map = extract_keywords_map(&request_ext_map);
    let first_imp_ext_keywords_map = extract_bidder_keywords_map(&first_imp_ext_map);

    // Parse and merge keywords
    let mut keywords = parse_keywords_from_map(&request_ext_keywords_map); // request.ext.keywords
    merge_keywords(&mut keywords, parse_keywords_from_map(&first_imp_ext_keywords_map)); // imp[0].ext.bidder.keywords
    merge_keywords(&mut keywords, parse_keywords_from_openrtb(user_keywords, "user")); // request.user.keywords
    merge_keywords(&mut keywords, parse_keywords_from_openrtb(site_keywords, "site")); // request.site.keywords

    // Build updated keywords map to write back into request.ext
    let mut updated_keywords_map: serde_json::Map<String, Value> = request_ext_keywords_map;

    if let Some(site_kw) = keywords.get("site") {
        if !site_kw.is_empty() {
            let site_val = keywords_to_value(&{
                let mut m = Keywords::new();
                m.insert("site".to_string(), site_kw.clone());
                m
            });
            // Extract the "site" value
            if let Value::Object(ref obj) = site_val {
                if let Some(sv) = obj.get("site") {
                    updated_keywords_map.insert("site".to_string(), sv.clone());
                }
            }
        } else {
            updated_keywords_map.remove("site");
        }
    } else {
        updated_keywords_map.remove("site");
    }

    if let Some(user_kw) = keywords.get("user") {
        if !user_kw.is_empty() {
            let user_val = keywords_to_value(&{
                let mut m = Keywords::new();
                m.insert("user".to_string(), user_kw.clone());
                m
            });
            if let Value::Object(ref obj) = user_val {
                if let Some(uv) = obj.get("user") {
                    updated_keywords_map.insert("user".to_string(), uv.clone());
                }
            }
        } else {
            updated_keywords_map.remove("user");
        }
    } else {
        updated_keywords_map.remove("user");
    }

    // Reconcile keywords with request.ext
    if !updated_keywords_map.is_empty() {
        request_ext_map.insert("keywords".to_string(), Value::Object(updated_keywords_map));
    } else {
        request_ext_map.remove("keywords");
    }

    if request_ext_map.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Value::Object(request_ext_map)))
    }
}

impl Bidder for GridAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut valid_imps = Vec::new();

        for imp in &request.imp {
            // Validate: uid must be non-zero in bidder ext
            let uid_ok = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("uid"))
                .and_then(|v| v.as_i64())
                .map(|uid| uid != 0)
                .unwrap_or(false);

            if !uid_ok {
                errs.push(BidderError::BadInput("uid is empty".to_string()));
                continue;
            }

            // setImpExtData: if data.adserver.adslot is set, copy it to gpid
            let mut imp = imp.clone();
            if let Some(ext) = imp.ext.as_mut() {
                if let Some(obj) = ext.as_object_mut() {
                    let adslot = obj.get("data")
                        .and_then(|d| d.get("adserver"))
                        .and_then(|a| a.get("adslot"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    if let Some(slot) = adslot {
                        if !slot.is_empty() {
                            obj.insert("gpid".to_string(), Value::String(slot));
                        }
                    }
                }
            }

            valid_imps.push(imp);
        }

        if valid_imps.is_empty() {
            errs.push(BidderError::BadInput("No valid impressions for grid".to_string()));
            return (vec![], errs);
        }

        let mut grid_request = request.clone();
        grid_request.imp = valid_imps;

        // setImpExtKeywords: enrich request.ext with consolidated keywords from
        // request.ext.keywords, imp[0].ext.bidder.keywords, user.keywords, site.keywords
        let user_keywords = grid_request.user.as_ref()
            .and_then(|u| u.keywords.as_deref())
            .unwrap_or("");
        let site_keywords = grid_request.site.as_ref()
            .and_then(|s| s.keywords.as_deref())
            .unwrap_or("");
        let first_imp_ext = grid_request.imp.first().and_then(|imp| imp.ext.as_ref());
        let request_ext = grid_request.ext.as_ref();

        match build_consolidated_keywords_req_ext(user_keywords, site_keywords, first_imp_ext, request_ext) {
            Ok(Some(new_ext)) => {
                grid_request.ext = Some(new_ext);
            }
            Ok(None) => {
                grid_request.ext = None;
            }
            Err(e) => {
                errs.push(e);
                return (vec![], errs);
            }
        }

        // fixNative: for any imp with native.request, move the parsed content to native.request_native
        // This adapts from OpenRTB native request string to Grid's expected format.
        let body_val: Value = match serde_json::to_value(&grid_request) {
            Ok(v) => v,
            Err(e) => {
                errs.push(BidderError::BadInput(e.to_string()));
                return (vec![], errs);
            }
        };
        let body_val = fix_native(body_val);
        let body = match serde_json::to_vec(&body_val) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::BadInput(e.to_string()));
                return (vec![], errs);
            }
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

        let imp_ids = get_imp_ids(&grid_request.imp);
        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids,
            }],
            errs,
        )
    }

    fn make_bids(
        &self,
        internal: &openrtb::BidRequest,
        _external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        // Grid returns a custom response where bids may have content_type and adm_native
        let bid_response: Value = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(1);

        if let Some(seatbids) = bid_response.get("seatbid").and_then(|v| v.as_array()) {
            for sb in seatbids {
                if let Some(bids) = sb.get("bid").and_then(|v| v.as_array()) {
                    for bid_val in bids {
                        // Extract content_type for bid type resolution
                        let content_type = bid_val.get("content_type").and_then(|v| v.as_str());

                        let imp_id = bid_val.get("impid").and_then(|v| v.as_str()).unwrap_or("");
                        let bid_type = match get_media_type_for_imp(imp_id, &internal.imp, content_type) {
                            Ok(t) => t,
                            Err(e) => return Err(vec![e]),
                        };

                        // Build a standard openrtb::Bid from the value
                        // If adm is empty but adm_native is set, use adm_native as adm
                        let mut bid_obj = bid_val.clone();
                        let adm_is_empty = bid_obj.get("adm")
                            .and_then(|v| v.as_str())
                            .map(|s| s.is_empty())
                            .unwrap_or(true);
                        if adm_is_empty {
                            if let Some(adm_native) = bid_obj.get("adm_native").cloned() {
                                if let Ok(adm_str) = serde_json::to_string(&adm_native) {
                                    if let Some(obj) = bid_obj.as_object_mut() {
                                        obj.insert("adm".to_string(), Value::String(adm_str));
                                    }
                                }
                            }
                        }

                        let bid: openrtb::Bid = match serde_json::from_value(bid_obj) {
                            Ok(b) => b,
                            Err(e) => return Err(vec![BidderError::BadServerResponse(e.to_string())]),
                        };

                        let bid_meta = get_bid_meta(&bid.ext);
                        let mut typed_bid = TypedBid::new(bid, bid_type);
                        typed_bid.bid_meta = bid_meta;
                        result.bids.push(typed_bid);
                    }
                }
            }
        }

        Ok(result)
    }
}
