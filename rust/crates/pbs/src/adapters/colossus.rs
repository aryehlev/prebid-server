//! Go `adapters/colossus/colossus.go`.

use serde::Deserialize;
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp};

pub struct Adapter {
    uri: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { uri: endpoint.into() }
    }

    fn make_request(&self, request: &BidRequest) -> Result<RequestData, BidderError> {
        let body = crate::go_json::to_vec(request).map_err(|e| BidderError::other(e.to_string()))?;
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        Ok(RequestData {
            method: "POST".into(),
            uri: self.uri.clone(),
            body,
            headers,
            imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
        })
    }
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct ColossusResponseBidExt {
    media_type: String,
}

/// Go `jsonparser.GetString(ext, "bidder", "TagID")`: `Ok(None)` is `KeyPathNotFoundError`.
fn get_tag_id(imp: &Imp) -> Result<Option<String>, BidderError> {
    let Some(ext) = &imp.ext else { return Ok(None) };
    let Some(v) = ext.0.get("bidder").and_then(|b| b.get("TagID")) else {
        return Ok(None);
    };
    match v.as_str() {
        Some(s) => Ok(Some(s.to_string())),
        None => Err(BidderError::other(format!("Value is not a string: {}", v))),
    }
}

fn get_media_type_for_imp(bid: &Bid, imps: &[Imp]) -> Result<BidType, BidderError> {
    // Go ignores the unmarshal error and falls through to the imp lookup.
    if let Some(ext) = &bid.ext {
        if let Ok(bid_ext) = ext.decode::<ColossusResponseBidExt>() {
            match bid_ext.media_type.as_str() {
                "banner" => return Ok(BidType::Banner),
                "native" => return Ok(BidType::Native),
                "video" => return Ok(BidType::Video),
                _ => {}
            }
        }
    }
    for imp in imps {
        if imp.id == bid.impid {
            if imp.banner.is_some() {
                return Ok(BidType::Banner);
            }
            if imp.video.is_some() {
                return Ok(BidType::Video);
            }
            if imp.native.is_some() {
                return Ok(BidType::Native);
            }
        }
    }
    Err(BidderError::bad_input(format!("Failed to find impression \"{}\"", bid.impid)))
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut adapter_requests = Vec::new();
        for imp in &request.imp {
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];
            match get_tag_id(imp) {
                Ok(Some(tag_id)) => req_copy.imp[0].tagid = tag_id,
                Ok(None) => {}
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            }
            match self.make_request(&req_copy) {
                Ok(r) => adapter_requests.push(r),
                Err(e) => errs.push(e),
            }
        }
        (adapter_requests, errs)
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        _external_request: &RequestData,
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
        let mut out = BidderResponse::with_bids_capacity(1);
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_imp(&bid, &internal_request.imp) {
                    Ok(t) => out.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        (Some(out), errs)
    }
}
