//! Go `adapters/emtv/emtv.go`.

use serde::{Deserialize, Serialize};
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

/// Go `openrtb_ext.ImpExt*`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ImpExt {
    #[serde(rename = "placementId")]
    placement_id: String,
    #[serde(rename = "endpointId")]
    endpoint_id: String,
}

#[derive(Serialize)]
struct ReqBodyExt {
    bidder: ReqBodyExtBidder,
}

#[derive(Serialize, Default)]
struct ReqBodyExtBidder {
    r#type: String,
    #[serde(rename = "placementId", skip_serializing_if = "String::is_empty")]
    placement_id: String,
    #[serde(rename = "endpointId", skip_serializing_if = "String::is_empty")]
    endpoint_id: String,
}

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }

    fn make_request(&self, request: &BidRequest) -> Result<RequestData, BidderError> {
        let body = crate::go_json::to_vec(request).map_err(|e| BidderError::other(e.to_string()))?;
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        Ok(RequestData {
            method: "POST".into(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
        })
    }
}

/// Go: `jsonutil.Unmarshal(imp.Ext, &ExtImpBidder)` then `jsonutil.Unmarshal(bidderExt.Bidder, &T)`.
/// Returns the Go error message text of whichever step fails.
fn imp_bidder_params<T: serde::de::DeserializeOwned>(ext: Option<&Ext>) -> Result<T, String> {
    fn not_obj(v: &sonic_rs::Value) -> Option<String> {
        if v.is_object() || v.is_null() {
            return None;
        }
        let first = sonic_rs::to_string(v).ok()?.chars().next().unwrap_or('\u{0}');
        Some(format!("expect {{ or n, but found {first}"))
    }
    let Some(ext) = ext else {
        return Err("expect { or n, but found \u{0}".to_string());
    };
    if let Some(m) = not_obj(&ext.0) {
        return Err(m);
    }
    match ext.0.get("bidder") {
        None => Err("expect { or n, but found \u{0}".to_string()),
        Some(b) => {
            if let Some(m) = not_obj(b) {
                return Err(m);
            }
            sonic_rs::from_value::<T>(b).map_err(|e| e.to_string())
        }
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        
        let mut adapter_requests = Vec::new();

        for imp in &request.imp {
            let ext: ImpExt = match imp_bidder_params(imp.ext.as_ref()) {
                Ok(e) => e,
                Err(m) => { return (vec![], vec![BidderError::FailedToUnmarshal(m)]) }
            };

            let mut bidder = ReqBodyExtBidder::default();
            if !ext.placement_id.is_empty() {
                bidder.placement_id = ext.placement_id;
                bidder.r#type = "publisher".into();
            } else if !ext.endpoint_id.is_empty() {
                bidder.endpoint_id = ext.endpoint_id;
                bidder.r#type = "network".into();
            } else { continue; }

            let ext_json = match serde_json::to_vec(&ReqBodyExt { bidder })
                .map_err(|e| e.to_string())
                .and_then(|b| Ext::from_slice(&b).map_err(|e| e.to_string()))
            {
                Ok(x) => x,
                Err(e) => { return (vec![], vec![BidderError::other(e)]) }
            };

            let mut imp = imp.clone();
            imp.ext = Some(ext_json);
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp];

            match self.make_request(&req_copy) {
                Ok(r) => adapter_requests.push(r),
                Err(e) => { return (vec![], vec![e]) }
            }
        }

        if adapter_requests.is_empty() {
            return (vec![], vec![BidderError::other("found no valid impressions")]);
        }
        (adapter_requests, vec![])
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

        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                match get_media_type_for_imp(&bid.impid, &request.imp) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => return (None, vec![e]),
                }
            }
        }
        (Some(bid_response), vec![])
    }
}

/// Go looks the imp up in a map keyed by ID, so the last imp with a duplicate ID wins.
fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> Result<BidType, BidderError> {
    if let Some(imp) = imps.iter().rev().find(|i| i.id == imp_id) {
        if imp.banner.is_some() {
            return Ok(BidType::Banner);
        }
        if imp.video.is_some() {
            return Ok(BidType::Video);
        }
        if imp.native.is_some() {
            return Ok(BidType::Native);
        }
    }
    Err(BidderError::bad_input(format!("Failed to find impression \"{imp_id}\"")))
}
