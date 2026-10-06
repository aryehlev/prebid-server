//! Go `adapters/smrtconnect/smrtconnect.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder,
    BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp, MarkupType};
use crate::ortb::Ext;

pub struct Adapter {
    endpoint: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: &str) -> Result<Self, String> {
        let endpoint = EndpointTemplate::parse(endpoint)
            .map_err(|e| format!("unable to parse endpoint url template: {e}"))?;
        Ok(Self { endpoint })
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtSmrtconnect {
    supply_id: String,
}

fn ext_text(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

fn get_impression_ext(imp: &Imp) -> Result<ExtSmrtconnect, BidderError> {
    let not_provided = || BidderError::bad_input("ext.bidder not provided");
    let bidder_ext: ExtImpBidder = jsonutil::unmarshal(&ext_text(&imp.ext)).map_err(|_| not_provided())?;
    jsonutil::unmarshal(&ext_text(&bidder_ext.bidder)).map_err(|_| not_provided())
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = vec![];
        let mut request_copy = request.clone();
        for imp in &request.imp {
            let ext = match get_impression_ext(imp) {
                Ok(e) => e,
                Err(e) => return (vec![], vec![e]),
            };
            let params = EndpointTemplateParams { supply_id: ext.supply_id, ..Default::default() };
            let url = match self.endpoint.resolve(&params) {
                Ok(u) => u,
                Err(e) => return (vec![], vec![BidderError::other(e)]),
            };
            // Go's `getImpressionExt` clears `imp.Ext` on the loop's copy before it is sent.
            let mut imp = imp.clone();
            imp.ext = None;
            request_copy.imp = vec![imp];
            let body = match crate::go_json::to_vec(&request_copy) {
                Ok(b) => b,
                Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
            };
            requests.push(RequestData {
                method: "POST".into(),
                uri: url,
                body,
                headers: Header::new(),
                imp_ids: request_copy.imp.iter().map(|i| i.id.clone()).collect(),
            });
        }
        (requests, vec![])
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
            Err(_) => return (None, vec![BidderError::bad_server_response("Bad Server Response")]),
        };
        if response.seatbid.is_empty() {
            return (None, vec![BidderError::bad_server_response("Empty SeatBid array")]);
        }
        let mut bid_errs = vec![];
        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        bid_response.currency = response.cur;
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                match get_bid_type(&bid) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => bid_errs.push(e),
                }
            }
        }
        (Some(bid_response), bid_errs)
    }
}

fn get_bid_type(bid: &Bid) -> Result<BidType, BidderError> {
    match bid.mtype {
        MarkupType::BANNER => Ok(BidType::Banner),
        MarkupType::VIDEO => Ok(BidType::Video),
        MarkupType::AUDIO => Ok(BidType::Audio),
        MarkupType::NATIVE => Ok(BidType::Native),
        _ => Err(BidderError::bad_input(format!(
            "Could not define media type for impression: {}",
            bid.impid
        ))),
    }
}
