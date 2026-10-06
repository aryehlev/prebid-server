//! Go `adapters/mobfoxpb/mobfoxpb.go`.

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::ext_helpers::ext_get;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use sonic_rs::JsonValueTrait;

const ROUTE_NATIVE: &str = "o";
const ROUTE_RTB: &str = "rtb";
const METHOD_NATIVE: &str = "ortb";
const METHOD_RTB: &str = "req";
const MACROS_ROUTE: &str = "__route__";
const MACROS_METHOD: &str = "__method__";
const MACROS_KEY: &str = "__key__";

pub struct Adapter {
    uri: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { uri: endpoint.into() }
    }
}

/// Go `jsonparser.GetString(imp.Ext, "bidder", name)`: an error when missing or not a string.
fn get_string(imp: &Imp, name: &str) -> Option<String> {
    ext_get(imp.ext.as_ref(), &["bidder", name]).and_then(|v| v.as_str()).map(str::to_string)
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        // Go indexes `request.Imp[0]` and panics on an empty imp list; report an error instead.
        let Some(imp) = request.imp.first() else {
            return (
                vec![],
                vec![BidderError::bad_input(
                    "Invalid or non existing key and tagId, at least one should be present",
                )],
            );
        };
        let tag_id = get_string(imp, "TagID");
        let key = get_string(imp, "key");
        if tag_id.is_none() && key.is_none() {
            return (
                vec![],
                vec![BidderError::bad_input(
                    "Invalid or non existing key and tagId, at least one should be present",
                )],
            );
        }
        let key = key.unwrap_or_default();
        let tag_id = tag_id.unwrap_or_default();
        let (mut route, mut method) = ("", "");
        let mut request_uri = self.uri.clone();
        if !key.is_empty() {
            route = ROUTE_RTB;
            method = METHOD_RTB;
            request_uri = request_uri.replacen(MACROS_KEY, &key, 1);
        } else if !tag_id.is_empty() {
            method = METHOD_NATIVE;
            route = ROUTE_NATIVE;
        }
        request_uri = request_uri.replacen(MACROS_ROUTE, route, 1);
        request_uri = request_uri.replacen(MACROS_METHOD, method, 1);

        let mut req_copy = request.clone();
        req_copy.imp = vec![imp.clone()];
        let body = match crate::go_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: request_uri,
                body,
                headers,
                imp_ids: vec![imp.id.clone()],
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 204 {
            return (None, vec![]);
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {}. Run with request.debug = 1 for more info",
                    response.status_code
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut errs = vec![];
        let mut bid_response = BidderResponse::with_bids_capacity(1);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_imp(&bid.impid, &request.imp) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        (Some(bid_response), errs)
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            let mut media_type = BidType::Banner;
            if imp.banner.is_some() {
                media_type = BidType::Banner;
            } else if imp.video.is_some() {
                media_type = BidType::Video;
            } else if imp.native.is_some() {
                media_type = BidType::Native;
            }
            return Ok(media_type);
        }
    }
    Err(BidderError::bad_server_response(format!("Failed to find impression \"{imp_id}\"")))
}
