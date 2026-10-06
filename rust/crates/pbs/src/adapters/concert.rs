//! Go `adapters/concert/concert.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder,
    BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::ext_helpers::ext_insert;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp, MarkupType};
use crate::ortb::Ext;

const ADAPTER_VERSION: &str = "1.0.0";

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

/// Go `openrtb_ext.ImpExtConcert` (only `partnerId` is used).
#[derive(Deserialize, Default)]
#[serde(default)]
struct ImpExtConcert {
    #[serde(rename = "partnerId")]
    partner_id: String,
}

fn get_bidder_ext(imp: &Imp) -> Result<ImpExtConcert, String> {
    let ext_bytes = imp.ext.as_ref().map(|e| e.to_json()).unwrap_or_default();
    let imp_ext: ExtImpBidder =
        jsonutil::unmarshal(ext_bytes.as_bytes()).map_err(|e| format!("imp ext: {e}"))?;
    let bidder_bytes = imp_ext.bidder.as_ref().map(|e| e.to_json()).unwrap_or_default();
    jsonutil::unmarshal(bidder_bytes.as_bytes()).map_err(|e| format!("bidder ext: {e}"))
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        // Go indexes `request.Imp[0]` and panics on an empty imp list; report an error instead.
        let Some(first) = request.imp.first() else {
            return (vec![], vec![BidderError::other("get bidder ext: no imps in request")]);
        };
        let bidder_imp_ext = match get_bidder_ext(first) {
            Ok(e) => e,
            Err(e) => return (vec![], vec![BidderError::other(format!("get bidder ext: {e}"))]),
        };

        // Go round-trips through a map (keys sorted alphabetically); the fixtures compare JSON
        // semantically, so key order is not observable.
        let mut req = request.clone();
        // `requestMap["ext"] == nil` also replaces a JSON null ext with an empty object.
        if req.ext.is_none() {
            req.ext = Some(Ext::from_slice(b"{}").unwrap());
        }
        let r1 = ext_insert(&mut req.ext, "adapterVersion", ADAPTER_VERSION);
        let r2 = ext_insert(&mut req.ext, "partnerId", &bidder_imp_ext.partner_id);
        if let Err(e) = r1.and(r2) {
            return (vec![], vec![BidderError::other(e.to_string())]);
        }
        let body = match crate::go_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };

        let mut headers = Header::new();
        headers.add("Content-Type", "application/json");
        headers.add("Accept", "application/json");
        let data = RequestData {
            method: "POST".into(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
        };
        (vec![data], vec![])
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
        if bid_response.bids.is_empty() {
            return (None, vec![BidderError::other("no bids returned")]);
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type_for_bid(bid: &Bid) -> Result<BidType, BidderError> {
    match bid.mtype {
        MarkupType::BANNER => Ok(BidType::Banner),
        MarkupType::VIDEO => Ok(BidType::Video),
        MarkupType::AUDIO => Ok(BidType::Audio),
        MarkupType::NATIVE => Err(BidderError::other("native media types are not yet supported")),
        _ => Err(BidderError::bad_server_response(format!(
            "Failed to parse media type for bid: \"{}\"",
            bid.impid
        ))),
    }
}
