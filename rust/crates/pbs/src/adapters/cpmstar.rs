//! Go `adapters/cpmstar/cpmstar.go`.

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

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

fn validate_imp(imp: &Imp) -> Result<(), BidderError> {
    if imp.banner.is_none() && imp.video.is_none() {
        return Err(BidderError::bad_input("Only Banner and Video bid-types are supported at this time"));
    }
    Ok(())
}

/// Go `preprocess`: flattens `imp.ext.bidder` into `imp.ext` (on a copy of the imps).
fn preprocess(imps: &mut [Imp]) -> Result<(), BidderError> {
    if imps.is_empty() {
        return Err(BidderError::bad_input("No Imps in Bid Request"));
    }
    for imp in imps.iter_mut() {
        let original: std::collections::BTreeMap<String, serde_json::Value> =
            unmarshal_raw(&ext_bytes(&imp.ext)).map_err(|e| BidderError::bad_input(e.to_string()))?;
        let Some(bidder_raw) = original.get("bidder") else {
            return Err(BidderError::bad_input("bidder field not found in impression extension"));
        };
        validate_imp(imp)?;
        // Go decodes into a `map[string]json.RawMessage` / `map[string]interface{}` and re-marshals
        // them, so keys come out sorted.
        let mut new_ext: std::collections::BTreeMap<String, serde_json::Value> =
            original.iter().filter(|(k, _)| k.as_str() != "bidder").map(|(k, v)| (k.clone(), v.clone())).collect();
        let bidder_bytes = serde_json::to_vec(bidder_raw).map_err(|e| BidderError::bad_input(e.to_string()))?;
        let bidder_config: std::collections::BTreeMap<String, serde_json::Value> =
            jsonutil::unmarshal(&bidder_bytes).map_err(|e| BidderError::bad_input(e.to_string()))?;
        for (k, v) in bidder_config {
            new_ext.insert(k, v);
        }
        let bytes = crate::go_json::to_vec(&new_ext).map_err(|e| BidderError::bad_input(e.to_string()))?;
        imp.ext = Some(Ext::from_slice(&bytes).map_err(|e| BidderError::bad_input(e.to_string()))?);
    }
    Ok(())
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req = request.clone();
        if let Err(e) = preprocess(&mut req.imp) {
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
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response_data.status_code == 204 {
            return (None, vec![]);
        }
        if response_data.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected HTTP status code: {}. Run with request.debug = 1 for more info",
                    response_data.status_code
                ))],
            );
        }
        let bid_response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![BidderError::bad_server_response(e.to_string())]),
        };
        if bid_response.seatbid.is_empty() {
            return (None, vec![]);
        }
        let mut rv = BidderResponse::with_bids_capacity(bid_response.seatbid[0].bid.len());
        let mut errors = Vec::new();
        for seatbid in bid_response.seatbid {
            for bid in seatbid.bid {
                let mut found = false;
                let mut bid_type = BidType::Banner;
                for imp in &request.imp {
                    if imp.id == bid.impid {
                        found = true;
                        if imp.banner.is_some() {
                            bid_type = BidType::Banner;
                        } else if imp.video.is_some() {
                            bid_type = BidType::Video;
                        }
                        break;
                    }
                }
                if found {
                    rv.bids.push(TypedBid::new(bid, bid_type));
                } else {
                    errors.push(BidderError::bad_server_response(format!(
                        "bid id='{}' could not find valid impid='{}'",
                        bid.id, bid.impid
                    )));
                }
            }
        }
        (Some(rv), errors)
    }
}
