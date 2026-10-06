//! Go `adapters/aax/aax.go`.

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp};
use serde::Deserialize;

/// Go `openrtb_ext.ExtImpAax` (never read by the adapter).
#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
pub struct ExtImpAax {
    pub cid: String,
    pub crid: String,
}

#[derive(Debug, Default, Deserialize)]
struct AaxResponseBidExt {
    #[serde(rename = "adCodeType", default)]
    ad_code_type: String,
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
pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`: `endpoint` is `config.Endpoint`, `extra_adapter_info` is
    /// `config.ExtraAdapterInfo`.
    pub fn new(endpoint: impl Into<String>, extra_adapter_info: impl Into<String>) -> Self {
        Self { endpoint: build_endpoint(&endpoint.into(), &extra_adapter_info.into()) }
    }
}

/// Go `buildEndpoint`: adds `src=<hostUrl>` to the query (Go `url.Values.Encode` sorts keys).
fn build_endpoint(aax_url: &str, host_url: &str) -> String {
    if host_url.is_empty() {
        return aax_url.to_string();
    }
    let Ok(mut u) = url::Url::parse(aax_url) else {
        return aax_url.to_string();
    };
    let mut pairs: Vec<(String, String)> =
        u.query_pairs().map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
    pairs.push(("src".into(), host_url.into()));
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    let q = url::form_urlencoded::Serializer::new(String::new()).extend_pairs(pairs).finish();
    u.set_query(Some(&q));
    u.to_string()
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let body = match crate::go_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        let data = RequestData {
            method: "POST".into(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
        };
        (vec![data], vec![])
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
        if response.status_code == 204 {
            return (None, vec![]);
        }
        if response.status_code == 400 {
            return (None, vec![BidderError::bad_input(msg())]);
        }
        if response.status_code != 200 {
            return (None, vec![BidderError::bad_server_response(msg())]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut errs = Vec::new();
        let mut bid_response = BidderResponse::new();
        for seat_bid in bid_resp.seatbid {
            for bid in seat_bid.bid {
                match get_media_type_for_imp(&bid, &internal_request.imp) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        (Some(bid_response), errs)
    }
}

fn get_media_type_for_imp(bid: &Bid, imps: &[Imp]) -> Result<BidType, BidderError> {
    if let Ok(ext) = decode_ext::<AaxResponseBidExt>(bid.ext.as_ref()) {
        match ext.ad_code_type.as_str() {
            "banner" => return Ok(BidType::Banner),
            "native" => return Ok(BidType::Native),
            "video" => return Ok(BidType::Video),
            _ => {}
        }
    }
    let mut media_type = BidType::default();
    let mut type_cnt = 0;
    for imp in imps {
        if imp.id == bid.impid {
            if imp.banner.is_some() {
                type_cnt += 1;
                media_type = BidType::Banner;
            }
            if imp.native.is_some() {
                type_cnt += 1;
                media_type = BidType::Native;
            }
            if imp.video.is_some() {
                type_cnt += 1;
                media_type = BidType::Video;
            }
        }
    }
    if type_cnt == 1 {
        return Ok(media_type);
    }
    Err(BidderError::other(format!("unable to fetch mediaType in multi-format: {}", bid.impid)))
}
