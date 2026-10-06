//! Go `adapters/mgidX/mgidX.go`.

use serde::{Deserialize, Serialize};

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder,
    BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse};
use crate::ortb::Ext;

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

#[derive(Serialize)]
struct ReqBodyExt {
    bidder: ReqBodyExtBidder,
}

#[derive(Serialize, Default)]
struct ReqBodyExtBidder {
    #[serde(rename = "type")]
    r#type: String,
    #[serde(rename = "placementId", skip_serializing_if = "String::is_empty")]
    placement_id: String,
    #[serde(rename = "endpointId", skip_serializing_if = "String::is_empty")]
    endpoint_id: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ImpExtMgidX {
    #[serde(rename = "placementId")]
    placement_id: String,
    #[serde(rename = "endpointId")]
    endpoint_id: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtBid {
    prebid: Option<ExtBidPrebid>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtBidPrebid {
    #[serde(rename = "type")]
    r#type: String,
}

fn ext_text(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut adapter_requests = vec![];
        let mut req_copy = request.clone();
        for imp in &request.imp {
            let bidder_ext: ExtImpBidder = match jsonutil::unmarshal(&ext_text(&imp.ext)) {
                Ok(e) => e,
                Err(e) => return (vec![], vec![e]),
            };
            let mgid_ext: ImpExtMgidX = match jsonutil::unmarshal(&ext_text(&bidder_ext.bidder)) {
                Ok(e) => e,
                Err(e) => return (vec![], vec![e]),
            };
            let mut imp_ext = ReqBodyExt { bidder: ReqBodyExtBidder::default() };
            if !mgid_ext.placement_id.is_empty() {
                imp_ext.bidder.placement_id = mgid_ext.placement_id;
                imp_ext.bidder.r#type = "publisher".into();
            } else if !mgid_ext.endpoint_id.is_empty() {
                imp_ext.bidder.endpoint_id = mgid_ext.endpoint_id;
                imp_ext.bidder.r#type = "network".into();
            } else {
                continue;
            }
            let final_ext = match Ext::from_serialize(&imp_ext) {
                Ok(e) => e,
                Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
            };
            let mut imp = imp.clone();
            imp.ext = Some(final_ext);
            req_copy.imp = vec![imp];
            let body = match crate::go_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
            };
            let mut headers = Header::new();
            headers.add("Content-Type", "application/json;charset=utf-8");
            headers.add("Accept", "application/json");
            adapter_requests.push(RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: req_copy.imp.iter().map(|i| i.id.clone()).collect(),
            });
        }
        if adapter_requests.is_empty() {
            return (vec![], vec![BidderError::other("found no valid impressions")]);
        }
        (adapter_requests, vec![])
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
        bid_response.currency = response.cur;
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                match get_bid_media_type(&bid) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => return (None, vec![e]),
                }
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_bid_media_type(bid: &Bid) -> Result<BidType, BidderError> {
    let ext_bid: ExtBid = jsonutil::unmarshal(&ext_text(&bid.ext))
        .map_err(|_| BidderError::other(format!("unable to deserialize imp {} bid.ext", bid.impid)))?;
    let Some(prebid) = ext_bid.prebid else {
        return Err(BidderError::other(format!("imp {} with unknown media type", bid.impid)));
    };
    // Go's `BidType` is a plain string, so any value passes through; the typed Rust enum can
    // only hold the four known types, so anything else is reported as unknown.
    BidType::parse(&prebid.r#type)
        .map_err(|_| BidderError::other(format!("imp {} with unknown media type", bid.impid)))
}
