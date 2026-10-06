//! Go `adapters/unruly/unruly.go`.

use serde::{Deserialize, Serialize};

use crate::bid_types::{BidType, ExtBidPrebidVideo};
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

pub struct Adapter {
    end_point: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

/// Go `openrtb_ext.ExtImpUnruly`.
#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
struct ExtImpUnruly {
    #[serde(rename = "siteid")]
    site_id_old: i64,
    #[serde(rename = "siteId")]
    site_id: i64,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Result<Self, BidderError> {
        Ok(Self { end_point: endpoint.into() })
    }

    fn pre_process(&self, req: &BidRequest, errors: &mut Vec<BidderError>) -> Option<BidRequest> {
        let mut req = req.clone();
        for imp in req.imp.iter_mut() {
            let bidder_ext: Option<ExtImpBidder> = imp.ext.as_ref().and_then(|e| e.decode().ok());
            let Some(bidder_ext) = bidder_ext else {
                errors.push(BidderError::bad_input(format!(
                    "ext data not provided in imp id={}. Abort all Request",
                    imp.id
                )));
                return None;
            };
            let unruly: Option<ExtImpUnruly> = bidder_ext.bidder.as_ref().and_then(|e| e.decode().ok());
            let Some(unruly) = unruly else {
                errors.push(BidderError::bad_input(format!(
                    "siteid not provided in imp id={}. Abort all Request",
                    imp.id
                )));
                return None;
            };
            let bidder_json = match crate::go_json::to_vec(&unruly) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(BidderError::other(e.to_string()));
                    return None;
                }
            };
            let mut wrapped = b"{\"bidder\":".to_vec();
            wrapped.extend_from_slice(&bidder_json);
            wrapped.push(b'}');
            match Ext::from_slice(&wrapped) {
                Ok(e) => imp.ext = Some(e),
                Err(e) => {
                    errors.push(BidderError::other(e.to_string()));
                    return None;
                }
            }
        }
        Some(req)
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        if let Some(request) = self.pre_process(request, &mut errs) {
            let body = match crate::go_json::to_vec(&request) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::other(e.to_string()));
                    return (vec![], errs);
                }
            };
            if !self.end_point.is_empty() {
                let mut headers = Header::new();
                headers.add("Content-Type", "application/json;charset=utf-8");
                headers.add("Accept", "application/json");
                return (
                    vec![RequestData {
                        method: "POST".into(),
                        uri: self.end_point.clone(),
                        body,
                        headers,
                        imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
                    }],
                    errs,
                );
            }
        }
        (vec![], errs)
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 204 {
            return (None, vec![]);
        }
        if response.status_code == 400 {
            return (
                None,
                vec![BidderError::bad_input(format!(
                    "unexpected status code: {}. Run with request.debug = 1 for more info",
                    response.status_code
                ))],
            );
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "unexpected status code: {}. Run with request.debug = 1 for more info",
                    response.status_code
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            // Go formats the error pointer with `%d`, which prints `&{%!d(string=MSG)}`.
            Err(e) => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "bad server response: &{{%!d(string={})}}. ",
                        e.message()
                    ))],
                )
            }
        };
        let mut bid_response = BidderResponse::with_bids_capacity(internal_request.imp.len());
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_imp(&bid.impid, &internal_request.imp) {
                    Err(e) => errs.extend(e),
                    Ok(bid_type) => {
                        let dur = bid.dur;
                        let mut tb = TypedBid::new(bid, bid_type);
                        if bid_type == BidType::Video && dur > 0 {
                            tb.bid_video = Some(ExtBidPrebidVideo { duration: dur as i32, primary_category: String::new() });
                        }
                        bid_response.bids.push(tb);
                    }
                }
            }
        }
        (Some(bid_response), errs)
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> Result<BidType, Vec<BidderError>> {
    let mut no_matching = Vec::new();
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return Ok(BidType::Banner);
            } else if imp.video.is_some() {
                return Ok(BidType::Video);
            } else {
                // Go appends the error but still returns the default (banner) media type, and the
                // caller drops the bid on any error.
                return Err(vec![BidderError::other("bid responses mediaType didn't match supported mediaTypes")]);
            }
        } else {
            no_matching.push(imp.id.clone());
        }
    }
    Err(vec![BidderError::other(format!(
        "Bid response imp ID {imp_id} not found in bid request containing imps [{}]\n",
        no_matching.join(" ")
    ))])
}
