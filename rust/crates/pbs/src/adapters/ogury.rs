//! Go `adapters/ogury/ogury.go`.

use serde_json::{Map, Value};

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder,
    BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, MarkupType};
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

fn build_headers(request: &BidRequest) -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    if let Some(d) = &request.device {
        headers.add("X-Forwarded-For", d.ip.clone());
        headers.add("X-Forwarded-For", d.ipv6.clone());
        headers.add("User-Agent", d.ua.clone());
        headers.add("Accept-Language", d.language.clone());
    }
    headers
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request_in: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut request = request_in.clone();
        let headers = build_headers(&request);
        let mut imps_with_ogury_params = vec![];

        for i in 0..request.imp.len() {
            let ext_bytes = request.imp[i].ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default();
            let mut imp_ext: Map<String, Value> = match jsonutil::unmarshal(&ext_bytes) {
                Ok(m) => m,
                Err(_) => {
                    return (
                        vec![],
                        vec![BidderError::bad_input("Bidder extension not provided or can't be unmarshalled")],
                    )
                }
            };
            let mut hoist: Map<String, Value> = Map::new();
            if let Some(bidder) = imp_ext.get("bidder") {
                if !bidder.is_null() {
                    match jsonutil::unmarshal(bidder.to_string().as_bytes()) {
                        Ok(m) => hoist = m,
                        Err(_) => {
                            return (
                                vec![],
                                vec![BidderError::bad_input(
                                    "Ogury bidder extension not provided or can't be unmarshalled",
                                )],
                            )
                        }
                    }
                }
            }
            // Extract every value from imp[].ext.bidder to imp[].ext.
            for (k, v) in &hoist {
                imp_ext.insert(k.clone(), v.clone());
            }
            imp_ext.remove("bidder");
            match Ext::from_serialize(&imp_ext) {
                Ok(e) => request.imp[i].ext = Some(e),
                Err(_) => {
                    return (
                        vec![],
                        vec![BidderError::bad_input("Error while marshaling Imp.Ext bidder extension")],
                    )
                }
            }
            // Save adUnitCode.
            let id = request.imp[i].id.clone();
            request.imp[i].tagid = id;

            // Currency conversion of a foreign-currency floor.
            let (floor, floor_cur) = (request.imp[i].bidfloor, request.imp[i].bidfloorcur.clone());
            if floor > 0.0 && !floor_cur.is_empty() && floor_cur.to_uppercase() != "USD" {
                match req_info.convert_currency(floor, &floor_cur, "USD") {
                    Ok(v) => {
                        request.imp[i].bidfloorcur = "USD".to_string();
                        request.imp[i].bidfloor = v;
                    }
                    Err(e) => return (vec![], vec![e]),
                }
            }

            if hoist.contains_key("assetKey") && hoist.contains_key("adUnitId") {
                imps_with_ogury_params.push(request.imp[i].clone());
            }
        }

        if imps_with_ogury_params.is_empty() {
            if let Some(site) = &request.site {
                if site.publisher.as_ref().map_or(true, |p| p.id.is_empty()) {
                    return (
                        vec![],
                        vec![BidderError::bad_input(
                            "Invalid request. assetKey/adUnitId or request.site.publisher.id required",
                        )],
                    );
                }
            } else if request.app.is_some() {
                return (
                    vec![],
                    vec![BidderError::bad_input("Invalid request. assetKey/adUnitId required")],
                );
            }
        } else {
            request.imp = imps_with_ogury_params;
        }

        let body = match crate::go_json::to_vec(&request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::FailedToMarshal(e.to_string())]),
        };
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
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
        let mut errors = vec![];
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                match get_media_type_for_bid(&bid) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errors.push(e),
                }
            }
        }
        if !errors.is_empty() {
            return (None, errors);
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type_for_bid(bid: &Bid) -> Result<BidType, BidderError> {
    match bid.mtype {
        MarkupType::BANNER => Ok(BidType::Banner),
        MarkupType::AUDIO => Ok(BidType::Audio),
        MarkupType::NATIVE => Ok(BidType::Native),
        MarkupType::VIDEO => Ok(BidType::Video),
        other => Err(BidderError::bad_server_response(format!(
            "Unsupported MType \"{}\", for impression \"{}\"",
            other.0, bid.impid
        ))),
    }
}
