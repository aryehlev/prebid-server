//! Go `adapters/adyoulike/adyoulike.go`.

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
    pub fn new(endpoint: impl Into<String>) -> Result<Self, BidderError> {
        Ok(Self { endpoint: endpoint.into() })
    }
}

/// Go `jsonparser.GetString(ext, "bidder", "placement")`.
fn get_placement(ext: Option<&Ext>) -> Result<String, BidderError> {
    let missing = || BidderError::other("Key path not found");
    let ext = ext.ok_or_else(missing)?;
    let v: serde_json::Value = serde_json::from_str(&ext.to_json()).map_err(|_| missing())?;
    match v.get("bidder").and_then(|b| b.get("placement")) {
        None => Err(missing()),
        Some(serde_json::Value::String(s)) => Ok(s.clone()),
        Some(other) => Err(BidderError::other(format!("Value is not a string: {other}"))),
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut req_copy = request.clone();
        req_copy.imp = Vec::new();
        for (ind, imp) in request.imp.iter().enumerate() {
            let mut imp = imp.clone();
            if imp.bidfloor > 0.0 && !imp.bidfloorcur.is_empty() && imp.bidfloorcur.to_uppercase() != "USD" {
                match req_info.convert_currency(imp.bidfloor, &imp.bidfloorcur, "USD") {
                    Ok(v) => {
                        imp.bidfloorcur = "USD".into();
                        imp.bidfloor = v;
                    }
                    Err(e) => return (vec![], vec![e]),
                }
            }
            req_copy.cur = vec!["USD".into()];
            req_copy.imp.push(imp);
            match get_placement(req_copy.imp[ind].ext.as_ref()) {
                Ok(tag) => req_copy.imp[ind].tagid = tag,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            }
        }
        let body = match crate::go_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::other(e.to_string()));
                Vec::new()
            }
        };
        if !errs.is_empty() {
            return (vec![], errs);
        }
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        headers.add("x-openrtb-version", "2.5");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: req_copy.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            errs,
        )
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        let msg = || {
            format!("Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code)
        };
        match response.status_code {
            200 => {}
            204 => return (None, vec![]),
            400 => return (None, vec![BidderError::bad_input(msg())]),
            _ => return (None, vec![BidderError::bad_server_response(msg())]),
        }
        let parsed: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        bid_response.currency = "USD".into();
        for seat_bid in parsed.seatbid {
            for bid in seat_bid.bid {
                let t = get_media_type_for_imp(&bid.impid, &request.imp);
                bid_response.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> BidType {
    let mut media_type = BidType::Banner;
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_none() && imp.video.is_some() {
                media_type = BidType::Video;
            } else if imp.banner.is_none() && imp.native.is_some() {
                media_type = BidType::Native;
            }
        }
    }
    media_type
}
