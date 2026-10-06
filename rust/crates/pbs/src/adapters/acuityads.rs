//! Go `adapters/acuityads/acuityads.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

pub struct Adapter {
    endpoint: EndpointTemplate,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

/// Go `openrtb_ext.ExtAcuityAds`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtAcuityAds {
    host: String,
    accountid: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Result<Self, BidderError> {
        let endpoint = endpoint.into();
        let t = EndpointTemplate::parse(&endpoint)
            .map_err(|e| BidderError::other(format!("unable to parse endpoint url template: {e}")))?;
        Ok(Self { endpoint: t })
    }
}

fn get_headers(request: &BidRequest) -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");
    headers.add("X-Openrtb-Version", "2.5");
    if let Some(device) = &request.device {
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

fn check_response_status_codes(response: &ResponseData) -> Option<BidderError> {
    match response.status_code {
        204 => None,
        400 => Some(BidderError::bad_input(format!("Unexpected status code: [ {} ]", response.status_code))),
        503 => Some(BidderError::bad_input(format!(
            "Something went wrong, please contact your Account Manager. Status Code: [ {} ] ",
            response.status_code
        ))),
        200 => None,
        _ => Some(BidderError::bad_input(format!(
            "Unexpected status code: [ {} ]. Run with request.debug = 1 for more info",
            response.status_code
        ))),
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        // Go indexes `Imp[0]` (panics when empty): error instead.
        let Some(imp0) = request.imp.first() else {
            return (vec![], vec![BidderError::other("index out of range [0] with length 0")]);
        };
        let ext: Option<ExtAcuityAds> = imp0
            .ext
            .as_ref()
            .and_then(|e| e.decode::<ExtImpBidder>().ok())
            .and_then(|b| b.bidder)
            .and_then(|b| b.decode().ok());
        let Some(ext) = ext else {
            return (vec![], vec![BidderError::bad_input("ext.bidder not provided")]);
        };
        let params = EndpointTemplateParams { host: ext.host, account_id: ext.accountid, ..Default::default() };
        let url = match self.endpoint.resolve(&params) {
            Ok(u) => u,
            Err(e) => return (vec![], vec![BidderError::other(e)]),
        };
        // Go clears `Imp[0].Ext` on the original request before marshalling.
        let mut req = request.clone();
        req.imp[0].ext = None;
        let body = match crate::go_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        (
            vec![RequestData {
                method: "POST".into(),
                uri: url,
                body,
                headers: get_headers(request),
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
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
        if let Some(e) = check_response_status_codes(response) {
            return (None, vec![e]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(_) => return (None, vec![BidderError::bad_server_response("Bad Server Response")]),
        };
        let Some(sb) = bid_resp.seatbid.into_iter().next() else {
            return (None, vec![BidderError::bad_server_response("Empty SeatBid array")]);
        };
        let mut bid_response = BidderResponse::with_bids_capacity(sb.bid.len());
        for bid in sb.bid {
            let t = get_media_type_for_imp(&bid.impid, &request.imp);
            bid_response.bids.push(TypedBid::new(bid, t));
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.video.is_some() {
                return BidType::Video;
            } else if imp.native.is_some() {
                return BidType::Native;
            }
            return BidType::Banner;
        }
    }
    BidType::Banner
}
