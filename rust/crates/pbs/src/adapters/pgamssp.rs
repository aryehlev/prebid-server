//! Go `adapters/pgamssp/pgamssp.go`.

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, MarkupType};
use crate::ortb::Ext;
use serde::{Deserialize, Serialize};

/// Go `openrtb_ext.ImpExtPgamSsp`.
#[derive(Debug, Default, Deserialize)]
struct ImpExtPgamSsp {
    #[serde(rename = "placementId", default)]
    placement_id: String,
    #[serde(rename = "endpointId", default)]
    endpoint_id: String,
}

#[derive(Serialize)]
struct ReqBodyExt {
    bidder: ReqBodyExtBidder,
}

#[derive(Serialize)]
struct ReqBodyExtBidder {
    r#type: String,
    #[serde(rename = "placementId", skip_serializing_if = "String::is_empty")]
    placement_id: String,
    #[serde(rename = "endpointId", skip_serializing_if = "String::is_empty")]
    endpoint_id: String,
}

/// Go `jsonutil.Unmarshal(ext, &target)` on a `json.RawMessage`; the failure text is the
/// json-iterator top-level one (`expect { or n, but found X`).
fn decode_ext<T: serde::de::DeserializeOwned>(ext: Option<&crate::ortb::Ext>) -> Result<T, BidderError> {
    let Some(ext) = ext else {
        // Go: Unmarshal of an empty RawMessage fails (never happens: PBS core validates imp.ext).
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into()));
    };
    use sonic_rs::JsonValueTrait;
    // json-iterator picks the object decoder from the first byte: anything but `{` / `null`
    // fails with its top-level message, while serde would accept an array for a struct.
    if ext.0.is_object() || ext.0.is_null() {
        if let Ok(v) = ext.decode::<T>() {
            return Ok(v);
        }
    }
    jsonutil::unmarshal::<T>(ext.to_json().as_bytes())
}

/// Go `adapters.ExtImpBidder` (only `bidder` is used).
#[derive(Debug, Default, serde::Deserialize)]
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

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut out = Vec::new();
        for imp in &request.imp {
            let mut imp = imp.clone();
            if imp.bidfloor > 0.0 && !imp.bidfloorcur.is_empty() && imp.bidfloorcur.to_uppercase() != "USD" {
                if let Ok(v) = req_info.convert_currency(imp.bidfloor, &imp.bidfloorcur, "USD") {
                    imp.bidfloorcur = "USD".into();
                    imp.bidfloor = v;
                }
            }
            let bidder_ext: ExtImpBidder = match decode_ext(imp.ext.as_ref()) {
                Ok(v) => v,
                Err(e) => return (vec![], vec![e]),
            };
            let pgam_ext: ImpExtPgamSsp = match decode_ext(bidder_ext.bidder.as_ref()) {
                Ok(v) => v,
                Err(e) => return (vec![], vec![e]),
            };
            let mut temp = ReqBodyExt {
                bidder: ReqBodyExtBidder { r#type: String::new(), placement_id: String::new(), endpoint_id: String::new() },
            };
            if !pgam_ext.placement_id.is_empty() {
                temp.bidder.placement_id = pgam_ext.placement_id;
                temp.bidder.r#type = "publisher".into();
            } else if !pgam_ext.endpoint_id.is_empty() {
                temp.bidder.endpoint_id = pgam_ext.endpoint_id;
                temp.bidder.r#type = "network".into();
            }
            imp.ext = match Ext::from_serialize(&temp) {
                Ok(e) => Some(e),
                Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
            };
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp];
            match self.make_request(&req_copy) {
                Ok(r) => out.push(r),
                Err(e) => return (vec![], vec![e]),
            }
        }
        (out, vec![])
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
        bid_response.currency = response.cur.clone();
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                let t = match get_bid_media_type(&bid) {
                    Ok(t) => t,
                    Err(e) => return (None, vec![e]),
                };
                bid_response.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_bid_media_type(bid: &Bid) -> Result<BidType, BidderError> {
    match bid.mtype {
        MarkupType::BANNER => Ok(BidType::Banner),
        MarkupType::VIDEO => Ok(BidType::Video),
        MarkupType::AUDIO => Ok(BidType::Audio),
        MarkupType::NATIVE => Ok(BidType::Native),
        _ => Err(BidderError::other(format!("Unable to fetch mediaType in multi-format: {}", bid.impid))),
    }
}
