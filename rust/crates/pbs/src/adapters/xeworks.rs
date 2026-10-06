//! Go `adapters/xeworks/xeworks.go`.

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
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp, SeatBid};
use crate::ortb::Ext;

pub struct Adapter {
    endpoint: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: &str) -> Result<Self, String> {
        let endpoint = EndpointTemplate::parse(endpoint)
            .map_err(|e| format!("unable to parse endpoint URL template: {e}"))?;
        Ok(Self { endpoint })
    }

    fn build_endpoint_from_request(&self, imp: &Imp) -> Result<String, BidderError> {
        let imp_ext: ExtImpBidder = jsonutil::unmarshal(&ext_text(&imp.ext)).map_err(|e| {
            BidderError::bad_input(format!("Failed to deserialize bidder impression extension: {e}"))
        })?;
        let xeworks_ext: ExtXeworks = jsonutil::unmarshal(&ext_text(&imp_ext.bidder))
            .map_err(|e| BidderError::bad_input(format!("Failed to deserialize Xeworks extension: {e}")))?;
        let params = EndpointTemplateParams {
            host: xeworks_ext.env,
            source_id: xeworks_ext.pid,
            ..Default::default()
        };
        self.endpoint.resolve(&params).map_err(BidderError::other)
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtXeworks {
    env: String,
    pid: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidType_ {
    #[serde(rename = "type")]
    r#type: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidExt {
    prebid: BidType_,
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
        let mut requests = vec![];
        let mut errs = vec![];
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");

        let mut request_copy = request.clone();
        for imp in &request.imp {
            request_copy.imp = vec![imp.clone()];
            let endpoint = match self.build_endpoint_from_request(imp) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            let body = match crate::go_json::to_vec(&request_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::other(e.to_string()));
                    continue;
                }
            };
            requests.push(RequestData {
                method: "POST".into(),
                uri: endpoint,
                body,
                headers: headers.clone(),
                imp_ids: vec![imp.id.clone()],
            });
        }
        (requests, errs)
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response) {
            return (None, vec![]);
        }
        if response.status_code == 503 {
            return (
                None,
                vec![BidderError::bad_input("Bidder Xeworks is unavailable. Please contact the bidder support.")],
            );
        }
        if let Some(err) = check_response_status_code_for_errors(response) {
            return (None, vec![err]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        if bid_resp.seatbid.is_empty() {
            return (None, vec![BidderError::bad_server_response("Array SeatBid cannot be empty")]);
        }
        prepare_bid_response(bid_resp.seatbid)
    }
}

fn prepare_bid_response(seats: Vec<SeatBid>) -> (Option<BidderResponse>, Vec<BidderError>) {
    let mut errs = vec![];
    let mut bid_response = BidderResponse::with_bids_capacity(seats.len());
    for seat_bid in seats {
        for (bid_id, bid) in seat_bid.bid.into_iter().enumerate() {
            let bid_ext: BidExt = match jsonutil::unmarshal(&ext_text(&bid.ext)) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(BidderError::bad_server_response(format!(
                        "Failed to parse Bid[{bid_id}].Ext: {e}"
                    )));
                    continue;
                }
            };
            let bid_type = match BidType::parse(&bid_ext.prebid.r#type) {
                Ok(t) => t,
                Err(_) => {
                    errs.push(BidderError::bad_server_response(format!(
                        "Bid[{bid_id}].Ext.Prebid.Type expects one of the following values: 'banner', 'native', 'video', 'audio', got '{}'",
                        bid_ext.prebid.r#type
                    )));
                    continue;
                }
            };
            bid_response.bids.push(TypedBid::new(bid, bid_type));
        }
    }
    (Some(bid_response), errs)
}
