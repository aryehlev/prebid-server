//! Go `adapters/frvradn/frvradn.go`.

use serde::{Deserialize, Serialize};

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder,
    BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

pub struct Adapter {
    uri: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

/// Go `openrtb_ext.ImpExtFRVRAdn`.
#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
struct ImpExtFrvrAdn {
    publisher_id: String,
    ad_unit_id: String,
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
    ty: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Result<Self, BidderError> {
        let endpoint = endpoint.into();
        if endpoint.is_empty() {
            return Err(BidderError::other("missing endpoint adapter parameter"));
        }
        Ok(Self { uri: endpoint })
    }
}

fn get_impression_ext(imp: &Imp) -> Result<ImpExtFrvrAdn, BidderError> {
    let ext_imp_bidder: ExtImpBidder = imp
        .ext
        .as_ref()
        .and_then(|e| e.decode().ok())
        .ok_or_else(|| BidderError::bad_input("missing ext"))?;
    let ext: ImpExtFrvrAdn = ext_imp_bidder
        .bidder
        .as_ref()
        .and_then(|e| e.decode().ok())
        .ok_or_else(|| BidderError::bad_input("missing ext.bidder"))?;
    if ext.publisher_id.is_empty() || ext.ad_unit_id.is_empty() {
        return Err(BidderError::bad_input("publisher_id and ad_unit_id are required"));
    }
    Ok(ext)
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        let mut request_copy = request.clone();
        for imp in &request.imp {
            let ext = match get_impression_ext(imp) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            let mut imp = imp.clone();
            if imp.bidfloor > 0.0 && !imp.bidfloorcur.is_empty() && imp.bidfloorcur.to_uppercase() != "USD" {
                match req_info.convert_currency(imp.bidfloor, &imp.bidfloorcur, "USD") {
                    Ok(v) => {
                        imp.bidfloorcur = "USD".into();
                        imp.bidfloor = v;
                    }
                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                }
            }
            let ext_json = match crate::go_json::to_vec(&ext).ok().and_then(|b| Ext::from_slice(&b).ok()) {
                Some(e) => e,
                None => {
                    errs.push(BidderError::other("failed to marshal imp ext"));
                    continue;
                }
            };
            imp.ext = Some(ext_json);
            request_copy.imp = vec![imp];
            match crate::go_json::to_vec(&request_copy) {
                Ok(body) => requests.push(RequestData {
                    method: "POST".into(),
                    uri: self.uri.clone(),
                    body,
                    headers: Header::new(),
                    imp_ids: request_copy.imp.iter().map(|i| i.id.clone()).collect(),
                }),
                Err(e) => errs.push(BidderError::other(e.to_string())),
            }
        }
        (requests, errs)
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
        if response_data.body.is_empty() {
            return (None, vec![]);
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        bid_response.currency = response.cur.clone();
        let mut errs = Vec::new();
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                match get_bid_media_type(&bid) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        (Some(bid_response), errs)
    }
}

fn get_bid_media_type(bid: &Bid) -> Result<BidType, BidderError> {
    let ext_bid: ExtBid = match bid.ext.as_ref().and_then(|e| e.decode().ok()) {
        Some(e) => e,
        None => {
            return Err(BidderError::other(format!("unable to deserialize imp {} bid.ext", bid.impid)))
        }
    };
    let Some(prebid) = ext_bid.prebid else {
        return Err(BidderError::other(format!("imp {} with unknown media type", bid.impid)));
    };
    // Go returns the raw string as a BidType; an unknown value cannot be represented here.
    BidType::parse(&prebid.ty).map_err(BidderError::other)
}
