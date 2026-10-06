//! Go `adapters/bidstack/bidstack.go`.

#![allow(unused_imports, dead_code)]

use serde::{Deserialize, Serialize};

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp, MarkupType};
use crate::ortb::Ext;

/// Go `adapters.ExtImpBidder`.
#[derive(Deserialize, Default)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<Ext>,
}

fn ext_bytes(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

/// `Unmarshal(imp.Ext, &bidderExt)` then `Unmarshal(bidderExt.Bidder, &params)`.
fn parse_bidder_ext<T: serde::de::DeserializeOwned>(imp: &Imp) -> Result<T, BidderError> {
    let outer: ExtImpBidder = jsonutil::unmarshal(&ext_bytes(&imp.ext))?;
    jsonutil::unmarshal(&ext_bytes(&outer.bidder))
}

fn json_headers() -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");
    headers
}

/// Go `jsonutil.Unmarshal` of an absent `json.RawMessage`: jsoniter reports a NUL byte.
fn unmarshal_raw<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T, BidderError> {
    if data.iter().all(|b| b" \t\r\n".contains(b)) {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()));
    }
    jsonutil::unmarshal(data)
}

const CURRENCY_USD_ISO4217: &str = "USD";
const CONTENT_TYPE_APPLICATION_JSON: &str = "application/json";
const HEADER_VALUE_AUTHORIZATION_BEARER: &str = "Bearer ";

#[derive(Deserialize, Default)]
#[serde(default)]
struct ImpExtBidstack {
    #[serde(rename = "publisherId")]
    publisher_id: String,
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

fn get_bidder_ext(imp: &Imp) -> Result<ImpExtBidstack, BidderError> {
    let outer: ExtImpBidder = unmarshal_raw(&ext_bytes(&imp.ext))
        .map_err(|e| BidderError::other(format!("imp ext: {e}")))?;
    unmarshal_raw(&ext_bytes(&outer.bidder)).map_err(|e| BidderError::other(format!("bidder ext: {e}")))
}

fn prepare_headers(request: &BidRequest) -> Result<Header, BidderError> {
    // Go indexes `request.Imp[0]` and panics on an empty imp list; return an error instead.
    let first = request
        .imp
        .first()
        .ok_or_else(|| BidderError::other("get bidder ext: no impressions in the request"))?;
    let ext = get_bidder_ext(first).map_err(|e| BidderError::other(format!("get bidder ext: {e}")))?;
    let mut headers = Header::new();
    headers.add("Content-Type", CONTENT_TYPE_APPLICATION_JSON);
    headers.add("Authorization", format!("{HEADER_VALUE_AUTHORIZATION_BEARER}{}", ext.publisher_id));
    Ok(headers)
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let headers = match prepare_headers(request) {
            Ok(h) => h,
            Err(e) => return (vec![], vec![BidderError::other(format!("headers prepare: {e}"))]),
        };

        let mut req = request.clone();
        for imp in req.imp.iter_mut() {
            if imp.bidfloor > 0.0
                && !imp.bidfloorcur.is_empty()
                && imp.bidfloorcur.to_uppercase() != CURRENCY_USD_ISO4217
            {
                match req_info.convert_currency(imp.bidfloor, &imp.bidfloorcur, CURRENCY_USD_ISO4217) {
                    Ok(v) => {
                        imp.bidfloorcur = CURRENCY_USD_ISO4217.to_string();
                        imp.bidfloor = v;
                    }
                    Err(e) => return (vec![], vec![e]),
                }
            }
        }

        let body = match crate::go_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(format!("bid request marshal: {e}"))]),
        };
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
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        match response_data.status_code {
            204 => return (None, vec![]),
            400 => return (None, vec![BidderError::other("bad request from publisher")]),
            200 => {}
            code => {
                return (None, vec![BidderError::other(format!("unexpected response status code: {code}"))])
            }
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![BidderError::other(format!("bid response unmarshal: {e}"))]),
        };
        let mut out = BidderResponse::with_bids_capacity(request.imp.len());
        out.currency = response.cur.clone();
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                out.bids.push(TypedBid::new(bid, BidType::Video));
            }
        }
        (Some(out), vec![])
    }
}
