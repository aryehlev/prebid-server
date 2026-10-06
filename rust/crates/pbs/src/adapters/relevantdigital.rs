//! Go `adapters/relevantdigital/relevantdigital.go`.

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::ext_helpers::ext_remove;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp, MarkupType};
use crate::ortb::Ext;
use serde::Deserialize;
use serde_json::{json, Map, Value};

const RELEVANT_DOMAIN: &str = ".relevant-digital.com";
const DEFAULT_TIMEOUT: f64 = 1000.0;
const DEFAULT_BUFFER_MS: i64 = 250;

/// Go `openrtb_ext.ExtRelevantDigital`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct ExtRelevantDigital {
    #[serde(rename = "accountId")]
    account_id: String,
    #[serde(rename = "placementId")]
    placement_id: String,
    #[serde(rename = "pbsHost")]
    host: String,
    #[serde(rename = "pbsBufferMs")]
    pbs_buffer_ms: i64,
}

impl Default for ExtRelevantDigital {
    fn default() -> Self {
        Self {
            account_id: String::new(),
            placement_id: String::new(),
            host: String::new(),
            pbs_buffer_ms: DEFAULT_BUFFER_MS,
        }
    }
}

/// Go `relevantExt` (read side; the write side is built by hand in `patch_bid_request_ext`).
#[derive(Debug, Default, Deserialize)]
struct RelevantExt {
    #[serde(default)]
    relevant: RelevantInner,
    #[serde(default)]
    prebid: PrebidExt,
}

#[derive(Debug, Default, Deserialize)]
struct RelevantInner {
    #[serde(default)]
    count: i64,
}

#[derive(Debug, Default, Deserialize)]
struct PrebidExt {
    #[serde(default)]
    debug: bool,
}

#[derive(Debug, Default, Deserialize)]
struct ExtBidPrebid {
    #[serde(default)]
    r#type: String,
}

/// Go `openrtb_ext.ExtBid` (only `prebid.type` is read).
#[derive(Debug, Default, Deserialize)]
struct ExtBid {
    #[serde(default)]
    prebid: Option<ExtBidPrebid>,
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
    endpoint: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: &str) -> Result<Self, String> {
        let tmpl = EndpointTemplate::parse(endpoint)
            .map_err(|e| format!("unable to parse endpoint url template: {e}"))?;
        Ok(Self { endpoint: tmpl })
    }
}

/// RFC 7386 JSON merge patch (`jsonpatch.MergePatch`).
fn merge_patch(target: Value, patch: Value) -> Value {
    match patch {
        Value::Object(p) => {
            let mut t = match target {
                Value::Object(t) => t,
                _ => Map::new(),
            };
            for (k, v) in p {
                if v.is_null() {
                    t.remove(&k);
                } else {
                    let cur = t.remove(&k).unwrap_or(Value::Null);
                    t.insert(k, merge_patch(cur, v));
                }
            }
            Value::Object(t)
        }
        other => other,
    }
}

fn patch_bid_request_ext(request: &mut BidRequest, id: &str) -> Result<(), BidderError> {
    let mut count = 0;
    let mut debug = false;
    if let Some(ext) = request.ext.as_ref() {
        match decode_ext::<RelevantExt>(Some(ext)) {
            Ok(e) => {
                count = e.relevant.count;
                debug = e.prebid.debug;
            }
            Err(_) => {
                return Err(BidderError::failed_to_request_bids(format!(
                    "failed to unmarshal ext, {}",
                    ext.to_json()
                )));
            }
        }
    }
    if count >= 5 {
        return Err(BidderError::failed_to_request_bids("too many requests"));
    }
    // Go keeps the other fields of relevantExt (all zero) and marshals them; `debug` is not
    // omitempty so the request's own value is written back.
    let patch = json!({
        "relevant": {"count": count + 1, "adapterType": "server"},
        "prebid": {"storedrequest": {"id": id}, "debug": debug},
    });
    let merged = match request.ext.as_ref() {
        None => patch,
        Some(existing) => {
            let existing: Value = serde_json::from_str(&existing.to_json())
                .map_err(|e| BidderError::failed_to_request_bids(format!("failed patch ext, {e}")))?;
            merge_patch(existing, patch)
        }
    };
    request.ext = Some(
        Ext::from_serialize(&merged).map_err(|_| BidderError::failed_to_request_bids("failed to marshal"))?,
    );
    Ok(())
}

fn set_tmax(request: &mut BidRequest, pbs_buffer_ms: i64) {
    let mut timeout = request.tmax as f64;
    if timeout <= 0.0 {
        timeout = DEFAULT_TIMEOUT;
    }
    let buffer = pbs_buffer_ms as f64;
    request.tmax = (timeout - buffer).max(buffer).min(timeout) as i64;
}

fn create_bid_request(prebid: &BidRequest, params: &[ExtRelevantDigital]) -> Result<Vec<u8>, BidderError> {
    let mut copy = prebid.clone();
    patch_bid_request_ext(&mut copy, &params[0].account_id)
        .map_err(|e| BidderError::bad_input(format!("failed to create bidRequest, error: {e}")))?;
    set_tmax(&mut copy, params[0].pbs_buffer_ms);
    for (idx, imp) in copy.imp.iter_mut().enumerate() {
        let ext = json!({"prebid": {"storedrequest": {"id": params[idx].placement_id}}});
        imp.ext = Ext::from_serialize(&ext).ok();
    }
    create_json_request(&mut copy)
}

/// Go `createJSONRequest`: marshal, then scrub previous Relevant data with `jsonparser.Delete`.
/// The same keys are removed from the typed request before it is serialized once.
fn create_json_request(request: &mut BidRequest) -> Result<Vec<u8>, BidderError> {
    // imp[].[banner/native/video/audio].ext.relevant; imp[].ext.context.relevant was just
    // replaced by `patch_bid_imp_ext`, so there is nothing to remove there.
    for imp in request.imp.iter_mut() {
        if let Some(b) = imp.banner.as_mut() {
            let _ = ext_remove(&mut b.ext, "relevant");
        }
        if let Some(v) = imp.video.as_mut() {
            let _ = ext_remove(&mut v.ext, "relevant");
        }
        if let Some(n) = imp.native.as_mut() {
            let _ = ext_remove(&mut n.ext, "relevant");
        }
        if let Some(a) = imp.audio.as_mut() {
            let _ = ext_remove(&mut a.ext, "relevant");
        }
    }
    // ext.prebid.[cache/targeting/aliases]
    if let Some(ext) = request.ext.as_mut() {
        use sonic_rs::JsonValueTrait;
        if let Some(prebid) = ext.0.get("prebid").filter(|p| p.is_object()) {
            let mut prebid: Value = serde_json::from_str(&prebid.to_string()).unwrap_or(Value::Null);
            if let Value::Object(m) = &mut prebid {
                for k in ["cache", "targeting", "aliases"] {
                    m.remove(k);
                }
            }
            let mut whole: Value = serde_json::from_str(&ext.to_json()).unwrap_or(Value::Null);
            if let Value::Object(w) = &mut whole {
                w.insert("prebid".into(), prebid);
            }
            if let Ok(e) = Ext::from_serialize(&whole) {
                *ext = e;
            }
        }
    }
    crate::go_json::to_vec(request).map_err(|e| BidderError::other(e.to_string()))
}

fn get_impression_ext(imp: &Imp) -> Result<ExtRelevantDigital, BidderError> {
    let bidder_ext: ExtImpBidder =
        decode_ext(imp.ext.as_ref()).map_err(|_| BidderError::bad_input("imp.ext not provided"))?;
    // Unmarshal into a pre-filled struct keeps the default pbsBufferMs when the key is absent.
    decode_ext(bidder_ext.bidder.as_ref()).map_err(|_| BidderError::bad_input("ext.bidder not provided"))
}

fn get_headers(request: &BidRequest) -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");
    headers.add("X-Openrtb-Version", "2.5");
    if let Some(device) = request.device.as_ref() {
        if !device.ua.is_empty() {
            headers.add("User-Agent", device.ua.clone());
        }
        if !device.ipv6.is_empty() {
            headers.add("X-Forwarded-For", device.ipv6.clone());
        }
        if !device.ip.is_empty() {
            headers.add("X-Forwarded-For", device.ip.clone());
        }
    }
    headers
}

impl Adapter {
    fn build_endpoint_url(&self, params: &mut ExtRelevantDigital) -> Result<String, BidderError> {
        params.host = params.host.replace("http://", "");
        params.host = params.host.replace("https://", "");
        params.host = params.host.replace(RELEVANT_DOMAIN, "");
        self.endpoint
            .resolve(&EndpointTemplateParams { host: params.host.clone(), ..Default::default() })
            .map_err(BidderError::other)
    }

    fn build_adapter_request(
        &self,
        prebid: &BidRequest,
        params: &mut [ExtRelevantDigital],
    ) -> Result<RequestData, BidderError> {
        let req_json = create_bid_request(prebid, params)?;
        let url = self.build_endpoint_url(&mut params[0])?;
        Ok(RequestData {
            method: "POST".into(),
            uri: url,
            body: req_json,
            headers: get_headers(prebid),
            imp_ids: prebid.imp.iter().map(|i| i.id.clone()).collect(),
        })
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut params = Vec::new();
        let mut errs = Vec::new();
        for imp in &request.imp {
            match get_impression_ext(imp) {
                Ok(e) => params.push(e),
                Err(e) => errs.push(e),
            }
        }
        if !errs.is_empty() {
            return (vec![], errs);
        }
        // Go indexes `params[0]` and panics with no imps; report an error instead.
        if params.is_empty() {
            return (vec![], vec![BidderError::bad_input("No impression in the bid request")]);
        }
        match self.build_adapter_request(request, &mut params) {
            Ok(r) => (vec![r], vec![]),
            Err(e) => (vec![], vec![e]),
        }
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response_data) {
            return (None, vec![]);
        }
        if let Some(err) = check_response_status_code_for_errors(response_data) {
            return (None, vec![err]);
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(response.seatbid.len());
        bid_response.currency = response.cur.clone();
        let mut errs = Vec::new();
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                match get_media_type_for_bid(&bid) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        (Some(bid_response), errs)
    }
}

fn get_media_type_for_bid_from_ext(bid: &Bid) -> Result<BidType, BidderError> {
    if bid.ext.is_some() {
        if let Ok(ext) = decode_ext::<ExtBid>(bid.ext.as_ref()) {
            if let Some(prebid) = ext.prebid {
                return BidType::parse(&prebid.r#type).map_err(BidderError::other);
            }
        }
    }
    Err(BidderError::other(format!("failed to parse bid type, missing ext: {}", bid.impid)))
}

fn get_media_type_for_bid(bid: &Bid) -> Result<BidType, BidderError> {
    match bid.mtype {
        MarkupType::BANNER => Ok(BidType::Banner),
        MarkupType::VIDEO => Ok(BidType::Video),
        MarkupType::AUDIO => Ok(BidType::Audio),
        MarkupType::NATIVE => Ok(BidType::Native),
        _ => get_media_type_for_bid_from_ext(bid),
    }
}
