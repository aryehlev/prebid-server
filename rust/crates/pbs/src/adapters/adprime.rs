//! Go `adapters/adprime/adprime.go`.

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

use crate::ortb::openrtb2::User;

/// Go `jsonutil.Unmarshal` of an absent `json.RawMessage`: jsoniter reports a NUL byte.
fn unmarshal_raw<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T, BidderError> {
    if data.iter().all(|b| b" \t\r\n".contains(b)) {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()));
    }
    jsonutil::unmarshal(data)
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpAdprime {
    #[serde(rename = "TagID")]
    tag_id: String,
    keywords: Vec<String>,
    audiences: Vec<String>,
}

#[derive(serde::Serialize)]
struct NewExt {
    bidder: NewExtBidder,
}

// Go marshals a `map[string]interface{}`, so keys are sorted: `TagID` before `placementId`.
#[derive(serde::Serialize)]
struct NewExtBidder {
    #[serde(rename = "TagID")]
    tag_id: String,
    #[serde(rename = "placementId")]
    placement_id: String,
}

pub struct Adapter {
    uri: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { uri: endpoint.into() }
    }

    fn make_request(&self, request: &BidRequest) -> Result<RequestData, BidderError> {
        let body = crate::go_json::to_vec(request).map_err(|e| BidderError::other(e.to_string()))?;
        Ok(RequestData {
            method: "POST".into(),
            uri: self.uri.clone(),
            body,
            headers: json_headers(),
            imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
        })
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut adapter_requests = Vec::new();
        for imp in &request.imp {
            // Go copies the request shallowly; Site/User copies are made before mutation.
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];

            let outer: ExtImpBidder = match unmarshal_raw(&ext_bytes(&req_copy.imp[0].ext)) {
                Ok(o) => o,
                Err(e) => return (vec![], vec![e]),
            };
            let ext: ExtImpAdprime = match unmarshal_raw(&ext_bytes(&outer.bidder)) {
                Ok(o) => o,
                Err(e) => return (vec![], vec![e]),
            };

            let tag_id = ext.tag_id;
            req_copy.imp[0].tagid = tag_id.clone();

            let new_ext = NewExt { bidder: NewExtBidder { tag_id: tag_id.clone(), placement_id: tag_id } };
            let bytes = match crate::go_json::to_vec(&new_ext) {
                Ok(b) => b,
                Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
            };
            req_copy.imp[0].ext = match Ext::from_slice(&bytes) {
                Ok(e) => Some(e),
                Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
            };

            if let Some(site) = req_copy.site.as_mut() {
                if !ext.keywords.is_empty() {
                    site.keywords = ext.keywords.join(",");
                }
            }
            if req_copy.site.is_some() && !ext.audiences.is_empty() {
                let user = req_copy.user.get_or_insert_with(User::default);
                user.customdata = ext.audiences.join(",");
            }

            match self.make_request(&req_copy) {
                Ok(r) => adapter_requests.push(r),
                Err(e) => errs.push(e),
            }
        }
        (adapter_requests, errs)
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        let code = response_data.status_code;
        if code == 204 {
            return (None, vec![]);
        }
        if code == 404 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Page not found: {code}. Run with request.debug = 1 for more info"
                ))],
            );
        }
        if code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {code}. Run with request.debug = 1 for more info"
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut errs = Vec::new();
        let mut out = BidderResponse::with_bids_capacity(1);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match bid.mtype {
                    MarkupType::BANNER => out.bids.push(TypedBid::new(bid, BidType::Banner)),
                    MarkupType::VIDEO => out.bids.push(TypedBid::new(bid, BidType::Video)),
                    MarkupType::NATIVE => out.bids.push(TypedBid::new(bid, BidType::Native)),
                    _ => errs.push(BidderError::other(format!(
                        "Unable to fetch mediaType in multi-format: {}",
                        bid.impid
                    ))),
                }
            }
        }
        (Some(out), errs)
    }
}
