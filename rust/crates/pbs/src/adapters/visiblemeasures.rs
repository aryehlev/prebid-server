//! Go `adapters/visiblemeasures/visiblemeasures.go`.

use serde::{Deserialize, Serialize};

use crate::bid_types::BidType;
use crate::bidder::{
    is_response_status_code_no_content, Bidder, BidderResponse, ExtraRequestInfo, RequestData,
    ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

pub struct Adapter {
    endpoint: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

/// Go `openrtb_ext.ImpExtVisibleMeasures`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct ImpExtVisibleMeasures {
    #[serde(rename = "placementId")]
    placement_id: String,
    #[serde(rename = "endpointId")]
    endpoint_id: String,
}

#[derive(Serialize)]
struct ReqBodyExt {
    bidder: ReqBodyExtBidder,
}

#[derive(Serialize, Default)]
struct ReqBodyExtBidder {
    #[serde(rename = "type")]
    ty: String,
    #[serde(rename = "placementId", skip_serializing_if = "String::is_empty")]
    placement_id: String,
    #[serde(rename = "endpointId", skip_serializing_if = "String::is_empty")]
    endpoint_id: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Result<Self, BidderError> {
        Ok(Self { endpoint: endpoint.into() })
    }
}

const EOF_MSG: &str = "unexpected end of JSON input";

fn decode<T: serde::de::DeserializeOwned>(ext: Option<&Ext>) -> Result<T, BidderError> {
    match ext {
        Some(e) => e.decode().map_err(|e| BidderError::FailedToUnmarshal(e.to_string())),
        None => Err(BidderError::FailedToUnmarshal(EOF_MSG.into())),
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut out = Vec::new();
        let mut req_copy = request.clone();
        for imp in &request.imp {
            req_copy.imp = vec![imp.clone()];

            let bidder_ext: ExtImpBidder = match decode(req_copy.imp[0].ext.as_ref()) {
                Ok(v) => v,
                Err(e) => return (vec![], vec![e]),
            };
            let vm: ImpExtVisibleMeasures = match decode(bidder_ext.bidder.as_ref()) {
                Ok(v) => v,
                Err(e) => return (vec![], vec![e]),
            };

            let mut temp = ReqBodyExt { bidder: ReqBodyExtBidder::default() };
            if !vm.placement_id.is_empty() {
                temp.bidder.placement_id = vm.placement_id;
                temp.bidder.ty = "publisher".into();
            } else if !vm.endpoint_id.is_empty() {
                temp.bidder.endpoint_id = vm.endpoint_id;
                temp.bidder.ty = "network".into();
            }
            let ext = match crate::go_json::to_vec(&temp).ok().and_then(|b| Ext::from_slice(&b).ok()) {
                Some(e) => e,
                None => return (vec![], vec![BidderError::other("failed to marshal imp ext")]),
            };
            req_copy.imp[0].ext = Some(ext);

            let body = match crate::go_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
            };
            let mut headers = Header::new();
            headers.add("Content-Type", "application/json;charset=utf-8");
            headers.add("Accept", "application/json");
            out.push(RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: req_copy.imp.iter().map(|i| i.id.clone()).collect(),
            });
        }
        (out, vec![])
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
        if response_data.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {}. Run with request.debug = 1 for more info.",
                    response_data.status_code
                ))],
            );
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        bid_response.currency = response.cur.clone();
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                match get_media_type_for_imp(&bid.impid, &request.imp) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => return (None, vec![e]),
                }
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> Result<BidType, BidderError> {
    // Go maps imps by id (the last imp with a given id wins).
    if let Some(imp) = imps.iter().rev().find(|i| i.id == imp_id) {
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
    Err(BidderError::bad_input(format!("Failed to find impression \"{imp_id}\"")))
}
