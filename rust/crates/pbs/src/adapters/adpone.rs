//! Go `adapters/adpone/adpone.go`.

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse};

/// Go `openrtb_ext.ExtAdpone`.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ExtAdpone {
    #[serde(rename = "placementId", default)]
    pub placement_id: String,
}

/// Go `adapters.ExtImpBidder`.
#[derive(Debug, Clone, Default, serde::Deserialize)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<crate::ortb::Ext>,
}

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        if let Some(imp) = request.imp.first() {
            let bidder_ext: ExtImpBidder = match imp.ext.as_ref() {
                // Go: Unmarshal of empty RawMessage fails with "unexpected end of JSON input"-style error.
                None => {
                    errs.push(BidderError::bad_input("expect { or n, but found \u{0}"));
                    ExtImpBidder::default()
                }
                Some(e) => match e.decode::<ExtImpBidder>() {
                    Ok(v) => v,
                    Err(err) => {
                        errs.push(BidderError::bad_input(err.to_string()));
                        ExtImpBidder::default()
                    }
                },
            };
            match bidder_ext.bidder.as_ref() {
                None => errs.push(BidderError::bad_input("expect { or n, but found \u{0}")),
                Some(b) => {
                    if let Err(err) = b.decode::<ExtAdpone>() {
                        errs.push(BidderError::bad_input(err.to_string()));
                    }
                }
            }
        }
        if request.imp.is_empty() {
            errs.push(BidderError::bad_input("No impression in the bid request"));
            return (vec![], errs);
        }
        let body = match crate::go_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::other(e.to_string()));
                return (vec![], errs);
            }
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        headers.add("x-openrtb-version", "2.5");
        let data = RequestData {
            method: "POST".into(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
        };
        (vec![data], errs)
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        let msg = || {
            format!("Unexpected status code: {}. Run with request.debug = 1 for more info", response_data.status_code)
        };
        match response_data.status_code {
            200 => {}
            204 => return (None, vec![]),
            400 => return (None, vec![BidderError::bad_input(msg())]),
            _ => return (None, vec![BidderError::bad_server_response(msg())]),
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        // Go indexes `SeatBid[0]` and panics on an empty seatbid; report an error instead.
        let Some(first) = response.seatbid.first() else {
            return (None, vec![BidderError::bad_server_response("no seatbid in the response")]);
        };
        let mut bid_response = BidderResponse::with_bids_capacity(first.bid.len());
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                bid_response.bids.push(TypedBid::new(bid, BidType::Banner));
            }
        }
        (Some(bid_response), vec![])
    }
}
