//! Go `adapters/qt/qt.go`.

use serde::{Deserialize, Serialize};

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp, MarkupType};
use crate::ortb::Ext;

/// Go `adapters.ExtImpBidder`.
#[derive(Deserialize, Default)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<Ext>,
}

fn ext_bytes(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

/// `jsonutil.Unmarshal` of an absent `json.RawMessage`: jsoniter reads a NUL byte and reports
/// `expect { or n, but found \u{0}` where serde would say EOF.
fn unmarshal_raw<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T, BidderError> {
    if data.iter().all(|b| b" \t\r\n".contains(b)) {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()));
    }
    jsonutil::unmarshal(data)
}

/// `Unmarshal(imp.Ext, &bidderExt)` then `Unmarshal(bidderExt.Bidder, &params)`.
fn parse_bidder_ext<T: serde::de::DeserializeOwned>(imp: &Imp) -> Result<T, BidderError> {
    let outer: ExtImpBidder = unmarshal_raw(&ext_bytes(&imp.ext))?;
    unmarshal_raw(&ext_bytes(&outer.bidder))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ImpExt {
    #[serde(rename = "placementId")]
    placement_id: String,
    #[serde(rename = "endpointId")]
    endpoint_id: String,
}

#[derive(Serialize)]
struct ReqBodyExt {
    bidder: ReqBodyExtBidder,
}

#[derive(Serialize)]
struct ReqBodyExtBidder {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "placementId", skip_serializing_if = "String::is_empty")]
    placement_id: String,
    #[serde(rename = "endpointId", skip_serializing_if = "String::is_empty")]
    endpoint_id: String,
}

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }

    fn make_request(&self, request: &BidRequest) -> Result<RequestData, BidderError> {
        let body = crate::go_json::to_vec(request).map_err(|e| BidderError::other(e.to_string()))?;
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        Ok(RequestData {
            method: "POST".into(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
        })
    }

    /// Builds the per-imp request with the rewritten `imp.ext`.
    fn request_for_imp(&self, request: &BidRequest, imp: &Imp) -> Result<RequestData, BidderError> {
        let params: ImpExt = parse_bidder_ext(imp)?;
        let mut body = ReqBodyExt {
            bidder: ReqBodyExtBidder { kind: String::new(), placement_id: String::new(), endpoint_id: String::new() },
        };
        if !params.placement_id.is_empty() {
            body.bidder.placement_id = params.placement_id;
            body.bidder.kind = "publisher".into();
        } else if !params.endpoint_id.is_empty() {
            body.bidder.endpoint_id = params.endpoint_id;
            body.bidder.kind = "network".into();
        }
        let bytes = crate::go_json::to_vec(&body).map_err(|e| BidderError::other(e.to_string()))?;
        let mut new_imp = imp.clone();
        new_imp.ext = Some(Ext::from_slice(&bytes).map_err(|e| BidderError::other(e.to_string()))?);
        let mut req_copy = BidRequest { imp: vec![new_imp], ..request_shallow(request) };
        let _ = &mut req_copy;
        self.make_request(&req_copy)
    }
}

/// Copy of the request without its imps (Go `reqCopy := *request`, then `Imp` replaced).
fn request_shallow(request: &BidRequest) -> BidRequest {
    let mut r = request.clone();
    r.imp = Vec::new();
    r
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut reqs = Vec::new();
        for imp in &request.imp {
            // Go drops the per-imp errors (`return adapterRequests, nil`).
            if let Ok(r) = self.request_for_imp(request, imp) {
                reqs.push(r);
            }
        }
        (reqs, vec![])
    }

    fn make_bids(
        &self,
        request: &BidRequest,
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
        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        if !response.cur.is_empty() {
            bid_response.currency = response.cur.clone();
        }
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                let bid_type = match get_bid_type(&bid) {
                    Ok(t) => t,
                    Err(e) => return (None, vec![e]),
                };
                bid_response.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_bid_type(bid: &crate::ortb::openrtb2::Bid) -> Result<BidType, BidderError> {
    // determinate media type by bid response field mtype
    match bid.mtype {
        MarkupType::BANNER => Ok(BidType::Banner),
        MarkupType::VIDEO => Ok(BidType::Video),

        MarkupType::NATIVE => Ok(BidType::Native),
        _ => Err(BidderError::other(format!("could not define media type for impression: {}", bid.impid))),
    }
}
