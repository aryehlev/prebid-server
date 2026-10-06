//! Go `adapters/decenterads/decenterads.go`.

use std::collections::HashMap;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
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

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");

        let mut result = Vec::with_capacity(request.imp.len());
        let mut errs = Vec::new();
        let mut req = request.clone();
        req.imp = Vec::new();
        for impression in &request.imp {
            // Go unmarshals a nil ext (empty bytes) as an error.
            let ext_bytes = impression.ext.as_ref().map(|e| e.to_json()).unwrap_or_default();
            let imp_ext: HashMap<String, Ext> = match jsonutil::unmarshal(ext_bytes.as_bytes()) {
                Ok(m) => m,
                Err(e) => {
                    errs.push(BidderError::other(format!("unable to parse bidder parameers: {e}")));
                    continue;
                }
            };
            let Some(bidder_ext) = imp_ext.get("bidder") else {
                errs.push(BidderError::other("bidder parameters required"));
                continue;
            };
            let mut imp = impression.clone();
            imp.ext = Some(bidder_ext.clone());
            req.imp = vec![imp];
            let body = match crate::go_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::other(e.to_string()));
                    continue;
                }
            };
            result.push(RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers: headers.clone(),
                imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
            });
        }
        (result, errs)
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        match response_data.status_code {
            204 => return (None, vec![]),
            400 => {
                return (
                    None,
                    vec![BidderError::bad_input(format!(
                        "unexpected status code: {}",
                        response_data.status_code
                    ))],
                )
            }
            200 => {}
            _ => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "unexpected status code: {}",
                        response_data.status_code
                    ))],
                )
            }
        }
        let bid_response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![BidderError::bad_server_response(e.to_string())]),
        };
        let mut response = BidderResponse::with_bids_capacity(request.imp.len());
        for seat_bid in bid_response.seatbid {
            for bid in seat_bid.bid {
                let t = get_media_type_for_imp(&bid.impid, &request.imp);
                response.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(response), vec![])
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return BidType::Banner;
            } else if imp.video.is_some() {
                return BidType::Video;
            } else if imp.native.is_some() {
                return BidType::Native;
            } else if imp.audio.is_some() {
                return BidType::Audio;
            }
        }
    }
    BidType::Banner
}
