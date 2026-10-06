//! Go `adapters/ix/ix.go`.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::bid_types::{BidType, ExtBidPrebidVideo};
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::native1::response::Response as NativeResponse;
use crate::ortb::native1::{EventTrackingMethod, EventType};
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Format, Imp, MarkupType, Publisher};
use crate::ortb::Ext;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sonic_rs::{JsonContainerTrait, JsonValueTrait};

/// Go `version.Ver` is stamped at build time; the version module is not ported (PLAN.md), so
/// the value Go reports for an unstamped build is used.
const VERSION: &str = "";

/// Go `openrtb_ext.ExtImpIx`.
#[derive(Debug, Default, Deserialize)]
struct ExtImpIx {
    #[serde(rename = "siteId", default)]
    site_id: String,
    #[serde(default)]
    #[allow(dead_code)]
    size: Option<Vec<i64>>,
    #[serde(default)]
    sid: String,
}

#[derive(Debug, Default, Deserialize)]
struct ExtBidPrebidVideoIn {
    #[serde(default)]
    duration: i32,
    #[serde(default)]
    primary_category: String,
}

#[derive(Debug, Default, Deserialize)]
struct ExtBidPrebidIn {
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    video: Option<ExtBidPrebidVideoIn>,
}

/// Go `openrtb_ext.ExtBid` (the parts ix reads).
#[derive(Debug, Default, Deserialize)]
struct ExtBid {
    #[serde(default)]
    prebid: Option<ExtBidPrebidIn>,
}

/// Go `Native11Wrapper`.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Native11Wrapper {
    #[serde(default)]
    native: NativeResponse,
}
/// Go `jsonutil.Unmarshal(ext, &target)` on a `json.RawMessage`; the failure text is the
/// json-iterator top-level one (`expect { or n, but found X`).
fn decode_ext<T: serde::de::DeserializeOwned>(ext: Option<&crate::ortb::Ext>) -> Result<T, BidderError> {
    let Some(ext) = ext else {
        // Go: Unmarshal of an empty RawMessage fails (never happens: PBS core validates imp.ext).
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into()));
    };
    use sonic_rs::JsonValueTrait;
    // json-iterator picks the object decoder from the first byte: anything but `{` / `null`
    // fails with its top-level message, while serde would accept an array for a struct.
    if ext.0.is_object() || ext.0.is_null() {
        if let Ok(v) = ext.decode::<T>() {
            return Ok(v);
        }
    }
    jsonutil::unmarshal::<T>(ext.to_json().as_bytes())
}
/// Go `adapters.ExtImpBidder` (only `bidder` is used).
#[derive(Debug, Default, serde::Deserialize)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<crate::ortb::Ext>,
}
pub struct Adapter {
    uri: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { uri: endpoint.into() }
    }
}

/// First character of the JSON text of `v` (json-iterator's "but found X").
fn found(v: &sonic_rs::Value) -> char {
    v.to_string().chars().next().unwrap_or(' ')
}

/// Go `jsonutil.Unmarshal` of a string field: json-iterator rejects anything but a string or null
/// with `cannot unmarshal {path}: expects " or n, but found X`; serde would take numbers.
fn check_string_field(obj: &sonic_rs::Value, key: &str, path: &str) -> Result<(), BidderError> {
    if let Some(v) = obj.get(key) {
        if !v.is_str() && !v.is_null() {
            return Err(BidderError::FailedToUnmarshal(format!(
                "cannot unmarshal {path}: expects \" or n, but found {}",
                found(v)
            )));
        }
    }
    Ok(())
}

fn unmarshal_to_ix_ext(imp: &Imp) -> Result<ExtImpIx, BidderError> {
    let bidder_ext: ExtImpBidder = decode_ext(imp.ext.as_ref())?;
    // Go: `adapters.ExtImpBidder.Prebid` is a struct pointer.
    if let Some(prebid) = imp.ext.as_ref().and_then(|e| e.0.get("prebid")) {
        if !prebid.is_object() && !prebid.is_null() {
            return Err(BidderError::FailedToUnmarshal(format!(
                "cannot unmarshal adapters.ExtImpBidder.Prebid: expect {{ or n, but found {}",
                found(prebid)
            )));
        }
    }
    let bidder = bidder_ext.bidder.as_ref();
    if let Some(b) = bidder.filter(|b| b.0.is_object()) {
        check_string_field(&b.0, "siteId", "openrtb_ext.ExtImpIx.SiteId")?;
        check_string_field(&b.0, "sid", "openrtb_ext.ExtImpIx.Sid")?;
    }
    decode_ext(bidder)
}

/// Go `moveSid`: moves sid from imp[].ext.bidder.sid to imp[].ext.sid.
fn move_sid(imp: &mut Imp, ix_ext: &ExtImpIx) -> Result<(), BidderError> {
    if !ix_ext.sid.is_empty() {
        let mut m: Map<String, Value> = match imp.ext.as_ref() {
            Some(e) if e.0.is_object() || e.0.is_null() => {
                serde_json::from_str(&e.to_json()).unwrap_or_default()
            }
            Some(e) => return Err(jsonutil::unmarshal::<Map<String, Value>>(e.to_json().as_bytes()).unwrap_err()),
            None => Map::new(),
        };
        m.insert("sid".into(), Value::String(ix_ext.sid.clone()));
        imp.ext = Some(Ext::from_serialize(&m).map_err(|e| BidderError::other(e.to_string()))?);
    }
    Ok(())
}

fn set_publisher_id(request: &mut BidRequest, unique_site_ids: &BTreeSet<String>, ix_diag_fields: &mut Map<String, Value>) {
    let site_ids: Vec<&String> = unique_site_ids.iter().collect();
    if let Some(site) = request.site.as_mut() {
        let publisher = site.publisher.get_or_insert_with(Publisher::default);
        if site_ids.len() == 1 {
            publisher.id = site_ids[0].clone();
        }
    }
    if let Some(app) = request.app.as_mut() {
        let publisher = app.publisher.get_or_insert_with(Publisher::default);
        if site_ids.len() == 1 {
            publisher.id = site_ids[0].clone();
        }
    }
    if site_ids.len() > 1 {
        // BTreeSet iterates sorted, as Go sorts the keys for predictable output.
        let joined = site_ids.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
        ix_diag_fields.insert("multipleSiteIds".into(), Value::String(joined));
    }
}

/// Go `extractVersionWithoutCommitHash`.
fn extract_version_without_commit_hash(ver: &str) -> &str {
    match ver.find('-') {
        Some(i) => &ver[..i],
        None => ver,
    }
}

/// Go `setIxDiagIntoExtRequest`: rewrites request.ext through `ExtRequest{prebid, schain, ixdiag}`
/// (other top-level keys are dropped, as in Go). `prebid` and `schain` are carried over as the
/// raw JSON since the typed Go structs are not ported.
fn set_ix_diag_into_ext_request(
    request: &mut BidRequest,
    mut fields: Map<String, Value>,
    ver: &str,
) -> Result<(), BidderError> {
    let mut prebid: Option<String> = None;
    let mut schain: Option<String> = None;
    let mut ix_diag_raw: Option<&sonic_rs::Value> = None;
    if let Some(ext) = request.ext.as_ref() {
        if !(ext.0.is_object() || ext.0.is_null()) {
            return Err(jsonutil::unmarshal::<Map<String, Value>>(ext.to_json().as_bytes())
                .err()
                .unwrap_or_else(|| BidderError::FailedToUnmarshal("invalid request.ext".into())));
        }
        if let Some(p) = ext.0.get("prebid").filter(|p| !p.is_null()) {
            if !p.is_object() {
                return Err(BidderError::FailedToUnmarshal(format!(
                    "cannot unmarshal ix.ExtRequest.Prebid: expect {{ or n, but found {}",
                    found(p)
                )));
            }
            prebid = Some(p.to_string());
            if let Some(channel) = p.get("channel").filter(|c| !c.is_null()) {
                let version = channel.get("version").and_then(|v| v.as_str()).unwrap_or_default();
                fields.insert("pbjsv".into(), Value::String(version.to_string()));
            }
        }
        if let Some(s) = ext.0.get("schain").filter(|s| !s.is_null()) {
            schain = Some(s.to_string());
        }
        ix_diag_raw = ext.0.get("ixdiag").filter(|d| !d.is_null());
    }
    // Slice commit hash out of version
    let server_version = if ver.is_empty() { "unknown" } else { extract_version_without_commit_hash(ver) };
    fields.insert("pbsv".into(), Value::String(server_version.to_string()));
    fields.insert("pbsp".into(), Value::String("go".into())); // indicate prebid server implementation use Go version

    let mut ix_diag_map: Map<String, Value> = match ix_diag_raw {
        Some(d) => {
            jsonutil::unmarshal::<Map<String, Value>>(d.to_string().as_bytes())?
        }
        None => Map::new(),
    };
    for (k, v) in fields {
        ix_diag_map.insert(k, v);
    }
    let ix_diag_json = serde_json::to_string(&ix_diag_map).map_err(|e| BidderError::other(e.to_string()))?;
    let mut out = String::from("{");
    if let Some(p) = prebid {
        out.push_str("\"prebid\":");
        out.push_str(&p);
        out.push(',');
    }
    if let Some(s) = schain {
        out.push_str("\"schain\":");
        out.push_str(&s);
        out.push(',');
    }
    out.push_str("\"ixdiag\":");
    out.push_str(&ix_diag_json);
    out.push('}');
    request.ext = Some(Ext::from_slice(out.as_bytes()).map_err(|e| BidderError::other(e.to_string()))?);
    Ok(())
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");

        let mut unique_site_ids = BTreeSet::new();
        let mut filtered_imps = Vec::with_capacity(request.imp.len());
        let mut request_copy = request.clone();
        let mut ix_diag_fields = Map::new();

        for imp in std::mem::take(&mut request_copy.imp) {
            let mut imp = imp;
            let ix_ext = match unmarshal_to_ix_ext(&imp) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            if !ix_ext.site_id.is_empty() {
                unique_site_ids.insert(ix_ext.site_id.clone());
            }
            if let Err(e) = move_sid(&mut imp, &ix_ext) {
                errs.push(e);
            }
            if let Some(banner) = imp.banner.as_mut() {
                if banner.format.is_empty() {
                    if let (Some(w), Some(h)) = (banner.w, banner.h) {
                        banner.format = vec![Format { w, h, ..Default::default() }];
                    }
                }
                if banner.format.len() == 1 {
                    banner.w = Some(banner.format[0].w);
                    banner.h = Some(banner.format[0].h);
                }
            }
            filtered_imps.push(imp);
        }
        request_copy.imp = filtered_imps;
        set_publisher_id(&mut request_copy, &unique_site_ids, &mut ix_diag_fields);
        if let Err(e) = set_ix_diag_into_ext_request(&mut request_copy, ix_diag_fields, VERSION) {
            errs.push(e);
        }
        if !request_copy.imp.is_empty() {
            match crate::go_json::to_vec(&request_copy) {
                Ok(body) => requests.push(RequestData {
                    method: "POST".into(),
                    uri: self.uri.clone(),
                    body,
                    headers,
                    imp_ids: request_copy.imp.iter().map(|i| i.id.clone()).collect(),
                }),
                Err(e) => errs.push(BidderError::other(e.to_string())),
            }
        }
        (requests, errs)
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        _external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        let msg = || {
            format!("Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code)
        };
        match response.status_code {
            204 => return (None, vec![]),
            400 => return (None, vec![BidderError::bad_input(msg())]),
            200 => {}
            _ => return (None, vec![BidderError::bad_server_response(msg())]),
        }
        let bid_response: BidResponse = match parse_bid_response(&response.body) {
            Ok(r) => r,
            Err(e) => {
                return (None, vec![BidderError::bad_server_response(format!("JSON parsing error: {e}"))]);
            }
        };

        // Store media type per impression in a map for later use to set in bid.ext.prebid.type
        // Won't work for multiple bid case with a multi-format ad unit. We expect to get type from exchange on such case.
        let mut imp_media_type_req: HashMap<&str, BidType> = HashMap::new();
        for imp in &internal_request.imp {
            if imp.banner.is_some() {
                imp_media_type_req.insert(&imp.id, BidType::Banner);
            } else if imp.video.is_some() {
                imp_media_type_req.insert(&imp.id, BidType::Video);
            } else if imp.native.is_some() {
                imp_media_type_req.insert(&imp.id, BidType::Native);
            } else if imp.audio.is_some() {
                imp_media_type_req.insert(&imp.id, BidType::Audio);
            }
        }

        let mut bidder_response = BidderResponse::with_bids_capacity(0);
        bidder_response.currency = bid_response.cur.clone();
        let mut errs = Vec::new();
        let resp_ext = bid_response.ext.clone();
        for seat_bid in bid_response.seatbid {
            for mut bid in seat_bid.bid {
                let bid_type = match get_media_type_for_bid(&bid, &imp_media_type_req) {
                    Ok(t) => t,
                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                };
                let mut bid_ext_video = None;
                if bid_type == BidType::Video {
                    if let Ok(ext) = decode_ext::<ExtBid>(bid.ext.as_ref()) {
                        if let Some(video) = ext.prebid.and_then(|p| p.video) {
                            bid_ext_video = Some(ExtBidPrebidVideo {
                                duration: video.duration,
                                primary_category: String::new(),
                            });
                            if bid.cat.is_empty() {
                                bid.cat = vec![video.primary_category];
                            }
                        }
                    }
                }
                if bid_type == BidType::Native {
                    if let Ok(mut wrapper) = jsonutil::unmarshal::<Native11Wrapper>(bid.adm.as_bytes()) {
                        if !wrapper.native.eventtrackers.is_empty() {
                            merge_native_imp_trackers(&mut wrapper.native);
                            if let Ok(json) = serde_json::to_string(&wrapper) {
                                bid.adm = json;
                            }
                        }
                    }
                    if let Ok(mut native) = jsonutil::unmarshal::<NativeResponse>(bid.adm.as_bytes()) {
                        if !native.eventtrackers.is_empty() {
                            merge_native_imp_trackers(&mut native);
                            if let Ok(json) = serde_json::to_string(&native) {
                                bid.adm = json;
                            }
                        }
                    }
                }
                let mut typed = TypedBid::new(bid, bid_type);
                typed.bid_video = bid_ext_video;
                bidder_response.bids.push(typed);
            }
        }

        if let Some(ext) = resp_ext.as_ref() {
            match parse_fledge(ext) {
                Ok(configs) => bidder_response.fledge_auction_configs = configs,
                Err(e) => {
                    errs.push(e);
                    return (None, errs);
                }
            }
        }
        (Some(bidder_response), errs)
    }
}

/// `jsonutil::unmarshal` plus json-iterator's wording for a non-string top-level `id`.
fn parse_bid_response(body: &[u8]) -> Result<BidResponse, BidderError> {
    if let Ok(root) = sonic_rs::from_slice::<sonic_rs::Value>(body) {
        if root.is_object() {
            check_string_field(&root, "id", "openrtb2.BidResponse.ID")?;
        }
    }
    jsonutil::unmarshal(body)
}

/// Go `ixRespExt` / `auctionConfig` handling: `protectedAudienceAuctionConfigs` becomes
/// `FledgeAuctionConfigs` (`[{impid, config}]`), serialized as JSON.
fn parse_fledge(ext: &Ext) -> Result<Option<Ext>, BidderError> {
    if !(ext.0.is_object() || ext.0.is_null()) {
        return Err(jsonutil::unmarshal::<Map<String, Value>>(ext.to_json().as_bytes())
            .err()
            .unwrap_or_else(|| BidderError::FailedToUnmarshal("invalid ext".into())));
    }
    let Some(list) = ext.0.get("protectedAudienceAuctionConfigs").filter(|l| !l.is_null()) else {
        return Ok(None);
    };
    let Some(items) = list.as_array() else {
        return Err(BidderError::FailedToUnmarshal(format!(
            "cannot unmarshal ix.ixRespExt.AuctionConfig: expect [ or n, but found {}",
            found(list)
        )));
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items.iter() {
        if !item.is_object() {
            return Err(BidderError::FailedToUnmarshal(format!("expect {{ or n, but found {}", found(item))));
        }
        check_string_field(item, "bidId", "ix.auctionConfig.BidId")?;
        let bid_id = item.get("bidId").and_then(|v| v.as_str()).unwrap_or_default();
        if let Some(config) = item.get("config") {
            let config: Value = serde_json::from_str(&config.to_string())
                .map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))?;
            let mut m = Map::new();
            m.insert("impid".into(), Value::String(bid_id.to_string()));
            m.insert("config".into(), config);
            out.push(Value::Object(m));
        }
    }
    Ext::from_serialize(&out).map(Some).map_err(|e| BidderError::other(e.to_string()))
}

fn get_media_type_for_bid(bid: &Bid, imp_media_type_req: &HashMap<&str, BidType>) -> Result<BidType, BidderError> {
    match bid.mtype {
        MarkupType::BANNER => return Ok(BidType::Banner),
        MarkupType::VIDEO => return Ok(BidType::Video),
        MarkupType::AUDIO => return Ok(BidType::Audio),
        MarkupType::NATIVE => return Ok(BidType::Native),
        _ => {}
    }
    if bid.ext.is_some() {
        if let Ok(ext) = decode_ext::<ExtBid>(bid.ext.as_ref()) {
            if let Some(prebid) = ext.prebid {
                if !prebid.r#type.is_empty() {
                    return BidType::parse(&prebid.r#type).map_err(BidderError::other);
                }
            }
        }
    }
    match imp_media_type_req.get(bid.impid.as_str()) {
        Some(t) => Ok(*t),
        None => Err(BidderError::other(format!("unmatched impression id: {}", bid.impid))),
    }
}

/// Go `mergeNativeImpTrackers`: native 1.2 to 1.1 tracker compatibility handling.
fn merge_native_imp_trackers(native: &mut NativeResponse) {
    // create unique list of imp pixels urls from `imptrackers` and `eventtrackers`
    let mut unique: BTreeSet<String> = BTreeSet::new();
    for v in &native.imptrackers {
        unique.insert(v.clone());
    }
    for v in &native.eventtrackers {
        if v.event == EventType::IMPRESSION && v.method == EventTrackingMethod::IMAGE {
            unique.insert(v.url.clone());
        }
    }
    // rewrite `imptrackers` with the deduped list of imp pixels (sorted so tests pass)
    native.imptrackers = unique.into_iter().collect();
}

#[allow(dead_code)]
type _Unused = BTreeMap<(), ()>;
