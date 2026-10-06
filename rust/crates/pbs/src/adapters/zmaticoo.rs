//! Go `adapters/zmaticoo/zmaticoo.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder,
    BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp, MarkupType};
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

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

/// Go `openrtb_ext.ExtImpZmaticoo`; `Option` fields so that a field missing from one imp keeps
/// the value of the previous imp, as Go reuses one struct across the loop.
#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpZmaticoo {
    #[serde(rename = "pubId")]
    pub_id: Option<String>,
    #[serde(rename = "zoneId")]
    zone_id: Option<String>,
}

fn ext_text(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

fn validate_zmaticoo_ext(request: &BidRequest) -> Vec<BidderError> {
    let mut pub_id = String::new();
    let mut zone_id = String::new();
    let mut errs = vec![];
    for imp in &request.imp {
        let ext_bidder: ExtImpBidder = match jsonutil::unmarshal(&ext_text(&imp.ext)) {
            Ok(e) => e,
            Err(e) => {
                errs.push(e);
                continue;
            }
        };
        let params: ExtImpZmaticoo = match jsonutil::unmarshal(&ext_text(&ext_bidder.bidder)) {
            Ok(e) => e,
            Err(e) => {
                errs.push(e);
                continue;
            }
        };
        if let Some(v) = params.pub_id {
            pub_id = v;
        }
        if let Some(v) = params.zone_id {
            zone_id = v;
        }
        if zone_id.is_empty() || pub_id.is_empty() {
            errs.push(BidderError::other("imp.ext.pubId or imp.ext.zoneId required"));
        }
    }
    errs
}

fn transform(request: &mut BidRequest) -> Result<(), BidderError> {
    for imp in request.imp.iter_mut() {
        if let Some(native) = imp.native.as_mut() {
            // serde_json maps are sorted by key, like Go's `map[string]interface{}`.
            let native_request: serde_json::Map<String, serde_json::Value> =
                jsonutil::unmarshal(native.request.as_bytes())?;
            if native_request.contains_key("native") {
                continue;
            }
            let mut copy = serde_json::Map::new();
            copy.insert("native".to_string(), serde_json::Value::Object(native_request));
            native.request = serde_json::to_string(&copy)
                .map_err(|e| BidderError::other(e.to_string()))?;
        }
    }
    Ok(())
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let errs = validate_zmaticoo_ext(request);
        if !errs.is_empty() {
            return (vec![], errs);
        }
        let mut req = request.clone();
        if let Err(e) = transform(&mut req) {
            return (vec![], vec![e]);
        }
        let body = match crate::go_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
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
        if is_response_status_code_no_content(response) {
            return (None, vec![]);
        }
        if check_response_status_code_for_errors(response).is_some() {
            return (
                None,
                vec![BidderError::bad_input(format!(
                    "Unexpected status code: {}.",
                    response.status_code
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut errs = vec![];
        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_bid(&bid) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        (Some(bid_response), errs)
    }
}

fn get_media_type_for_bid(bid: &Bid) -> Result<BidType, BidderError> {
    match bid.mtype {
        MarkupType::BANNER => Ok(BidType::Banner),
        MarkupType::NATIVE => Ok(BidType::Native),
        MarkupType::VIDEO => Ok(BidType::Video),
        _ => Err(BidderError::other(format!(
            "unrecognized bid type in response from zmaticoo for bid {}",
            bid.impid
        ))),
    }
}

#[allow(dead_code)]
fn _unused(_: &Imp) {}
