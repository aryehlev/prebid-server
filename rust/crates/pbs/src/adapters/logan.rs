//! Go `adapters/logan/logan.go`.

use serde::{Deserialize, Serialize};
use sonic_rs::JsonValueTrait;

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

#[derive(Serialize, Deserialize, Default)]
struct ReqBodyExt {
    #[serde(rename = "bidder", default)]
    logan_bidder_ext: ReqBodyExtBidder,
}

#[derive(Serialize, Deserialize, Default)]
struct ReqBodyExtBidder {
    #[serde(rename = "type", default)]
    r#type: String,
    #[serde(rename = "placementId", default, skip_serializing_if = "String::is_empty")]
    placement_id: String,
}

/// Go `jsonutil.Unmarshal(raw, &v)` on a `json.RawMessage` that may be absent or not an object:
/// json-iterator reports `expect { or n, but found X` for anything but an object or `null`.
fn unmarshal_ext<T: serde::de::DeserializeOwned + Default>(ext: Option<&Ext>) -> Result<T, BidderError> {
    let Some(ext) = ext else {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into()));
    };
    if ext.0.is_null() {
        return Ok(T::default());
    }
    if !ext.0.is_object() {
        let text = ext.to_json();
        let first = text.chars().next().unwrap_or('\0');
        return Err(BidderError::FailedToUnmarshal(format!("expect {{ or n, but found {first}")));
    }
    ext.decode().map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> Result<BidType, BidderError> {
    if let Some(imp) = imps.iter().find(|i| i.id == imp_id) {
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

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors = Vec::new();
        let mut adapter_requests = Vec::new();
        for imp in &request.imp {
            let mut bidder_ext: ReqBodyExt = match unmarshal_ext(imp.ext.as_ref()) {
                Ok(e) => e,
                Err(e) => {
                    errors.push(BidderError::bad_input(e.to_string()));
                    continue;
                }
            };
            bidder_ext.logan_bidder_ext.r#type = "publisher".into();
            let final_imp_ext = match Ext::from_serialize(&bidder_ext) {
                Ok(e) => e,
                Err(e) => {
                    errors.push(BidderError::other(e.to_string()));
                    return (vec![], errors);
                }
            };
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];
            req_copy.imp[0].ext = Some(final_imp_ext);
            match self.make_request(&req_copy) {
                Ok(r) => adapter_requests.push(r),
                Err(e) => {
                    errors.push(e);
                    return (vec![], errors);
                }
            }
        }
        // Go returns `nil` errors on the success path even when some imps were skipped.
        (adapter_requests, vec![])
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response_data.status_code == 204 {
            return (None, vec![]);
        }
        if response_data.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {}. Run with request.debug = 1 for more info.",
                    response_data.status_code
                ))],
            );
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut out = BidderResponse::with_bids_capacity(request.imp.len());
        out.currency = response.cur.clone();
        for sb in response.seatbid {
            for bid in sb.bid {
                match get_media_type_for_imp(&bid.impid, &request.imp) {
                    Ok(t) => out.bids.push(TypedBid::new(bid, t)),
                    Err(e) => return (None, vec![e]),
                }
            }
        }
        (Some(out), vec![])
    }
}
