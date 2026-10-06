//! Go `adapters/medianet/medianet.go`.

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, MarkupType};

/// Go `openrtb_ext.ExtImpMedianet` (never read by the adapter).
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[allow(dead_code)]
pub struct ExtImpMedianet {
    pub cid: String,
    pub crid: String,
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
fn build_endpoint(mnet_url: &str, host_url: &str) -> String {
    if host_url.is_empty() {
        return mnet_url.to_string();
    }
    let Ok(mut u) = url::Url::parse(mnet_url) else {
        return mnet_url.to_string();
    };
    let mut pairs: Vec<(String, String)> =
        u.query_pairs().map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
    pairs.push(("src".into(), host_url.into()));
    // Stable sort by key, like Go's Values.Encode.
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
        _request: &BidRequest,
        _request_data: &RequestData,
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
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_bid_media_type_from_mtype(&bid) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        (Some(bid_response), errs)
    }
}

fn get_bid_media_type_from_mtype(bid: &Bid) -> Result<BidType, BidderError> {
    match bid.mtype {
        MarkupType::BANNER => Ok(BidType::Banner),
        MarkupType::VIDEO => Ok(BidType::Video),
        MarkupType::NATIVE => Ok(BidType::Native),
        _ => Err(BidderError::other(format!("Unable to fetch mediaType for imp: {}", bid.impid))),
    }
}
