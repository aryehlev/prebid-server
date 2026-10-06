//! Go `adapters/improvedigital/improvedigital.go`.

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

use std::collections::BTreeMap;

use serde_json::value::RawValue;

const IS_REWARDED_INVENTORY: &str = "is_rewarded_inventory";
const STATE_REWARDED_INVENTORY_ENABLE: &str = "1";
const PUBLISHER_ENDPOINT_PARAM: &str = "{PublisherId}";

/// Go `jsonutil.Unmarshal` of an absent `json.RawMessage`: jsoniter reports a NUL byte.
fn unmarshal_raw<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T, BidderError> {
    if data.iter().all(|b| b" \t\r\n".contains(b)) {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()));
    }
    jsonutil::unmarshal(data)
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidExt {
    #[serde(alias = "improvedigital", rename = "Improvedigital")]
    improvedigital: BidExtImprovedigital,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidExtImprovedigital {
    line_item_id: i64,
    buying_type: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ImpExtBidderInner {
    #[serde(rename = "publisherId")]
    publisher_id: i64,
}

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }

    fn make_request(&self, request: &BidRequest, imp: &Imp) -> Result<RequestData, BidderError> {
        let mut imp = imp.clone();
        // Handle Rewarded Inventory
        if let Some(ext) = get_imp_ext_with_rewarded_inventory(&imp)? {
            imp.ext = Some(ext);
        }
        let mut req = request.clone();
        req.imp = vec![imp];
        let body = crate::go_json::to_vec(&req).map_err(|e| BidderError::other(e.to_string()))?;
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        Ok(RequestData {
            method: "POST".into(),
            uri: self.build_endpoint_url(&req.imp[0]),
            body,
            headers,
            imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
        })
    }

    fn build_endpoint_url(&self, imp: &Imp) -> String {
        let mut publisher_endpoint = String::new();
        // Go decodes with case-insensitive keys (`Bidder` matches `bidder`).
        if let Ok(outer) = unmarshal_raw::<ExtImpBidderCi>(&ext_bytes(&imp.ext)) {
            if let Some(b) = outer.bidder {
                if b.publisher_id != 0 {
                    publisher_endpoint = format!("{}/", b.publisher_id);
                }
            }
        }
        self.endpoint.replace(PUBLISHER_ENDPOINT_PARAM, &publisher_endpoint)
    }
}

#[derive(Deserialize, Default)]
struct ExtImpBidderCi {
    #[serde(default, alias = "Bidder")]
    bidder: Option<ImpExtBidderInner>,
}

fn get_imp_ext_with_rewarded_inventory(imp: &Imp) -> Result<Option<Ext>, BidderError> {
    let mut ext: BTreeMap<String, Box<RawValue>> = unmarshal_raw(&ext_bytes(&imp.ext))?;
    let Some(prebid_value) = ext.get("prebid") else {
        return Ok(None);
    };
    let prebid_map: BTreeMap<String, Box<RawValue>> = unmarshal_raw(prebid_value.get().as_bytes())?;
    if let Some(rewarded) = prebid_map.get(IS_REWARDED_INVENTORY) {
        if rewarded.get() == STATE_REWARDED_INVENTORY_ENABLE {
            ext.insert(
                IS_REWARDED_INVENTORY.to_string(),
                RawValue::from_string("true".to_string()).map_err(|e| BidderError::other(e.to_string()))?,
            );
            let bytes = crate::go_json::to_vec(&ext).map_err(|e| BidderError::other(e.to_string()))?;
            return Ext::from_slice(&bytes).map(Some).map_err(|e| BidderError::other(e.to_string()));
        }
    }
    Ok(None)
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors = Vec::new();
        let mut reqs = Vec::with_capacity(request.imp.len());
        // Split multi-imp request into multiple ad server requests. SRA is currently not recommended.
        for imp in &request.imp {
            match self.make_request(request, imp) {
                Ok(r) => reqs.push(r),
                Err(e) => errors.push(e),
            }
        }
        (reqs, errors)
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        _external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        let code = response.status_code;
        if code == 204 {
            return (None, vec![]);
        }
        if code == 400 {
            return (
                None,
                vec![BidderError::bad_input(format!(
                    "Unexpected status code: {code}. Run with request.debug = 1 for more info"
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
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        if bid_resp.seatbid.is_empty() {
            return (None, vec![]);
        }
        if bid_resp.seatbid.len() > 1 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected SeatBid! Must be only one but have: {}",
                    bid_resp.seatbid.len()
                ))],
            );
        }
        let cur = bid_resp.cur.clone();
        let seat_bid = bid_resp.seatbid.into_iter().next().unwrap();
        if seat_bid.bid.is_empty() {
            return (None, vec![]);
        }
        let mut out = BidderResponse::with_bids_capacity(seat_bid.bid.len());
        out.currency = cur;

        // Later imps with a duplicate id overwrite earlier ones, as in Go's map.
        let mut imp_map: std::collections::HashMap<&str, &Imp> = std::collections::HashMap::new();
        for imp in &internal_request.imp {
            imp_map.insert(imp.id.as_str(), imp);
        }

        for mut bid in seat_bid.bid {
            let bid_type = match get_bid_type(&bid, &imp_map) {
                Ok(t) => t,
                Err(e) => return (None, vec![e]),
            };
            if bid.ext.is_some() {
                let bid_ext: BidExt = match unmarshal_raw(&ext_bytes(&bid.ext)) {
                    Ok(e) => e,
                    Err(e) => return (None, vec![e]),
                };
                let ext = bid_ext.improvedigital;
                if ext.line_item_id != 0 && ext.buying_type.contains("classic")
                    || ext.line_item_id != 0 && ext.buying_type.contains("deal")
                {
                    bid.dealid = ext.line_item_id.to_string();
                }
            }
            out.bids.push(TypedBid::new(bid, bid_type));
        }
        (Some(out), vec![])
    }
}

fn get_bid_type(
    bid: &crate::ortb::openrtb2::Bid,
    imp_map: &std::collections::HashMap<&str, &Imp>,
) -> Result<BidType, BidderError> {
    // there must be a matching imp against bid.ImpID
    let Some(imp) = imp_map.get(bid.impid.as_str()) else {
        return Err(BidderError::bad_server_response(format!(
            "Failed to find impression for ID: \"{}\"",
            bid.impid
        )));
    };

    let mut mtype = bid.mtype;
    // if MType is not set in server response, try to determine it
    if mtype.0 == 0 {
        if !is_multi_format_imp(imp) {
            // Not a bid for multi format impression. So, determine MType from impression
            if imp.banner.is_some() {
                mtype = MarkupType::BANNER;
            } else if imp.video.is_some() {
                mtype = MarkupType::VIDEO;
            } else if imp.audio.is_some() {
                mtype = MarkupType::AUDIO;
            } else if imp.native.is_some() {
                mtype = MarkupType::NATIVE;
            } else {
                // This should not happen. Let's handle it just in case by returning an error.
                return Err(BidderError::bad_server_response(format!(
                    "Could not determine MType from impression with ID: \"{}\"",
                    bid.impid
                )));
            }
        } else {
            return Err(BidderError::bad_server_response(format!(
                "Bid must have non-zero MType for multi format impression with ID: \"{}\"",
                bid.impid
            )));
        }
    }

    match mtype {
        MarkupType::BANNER => Ok(BidType::Banner),
        MarkupType::VIDEO => Ok(BidType::Video),
        MarkupType::AUDIO => Ok(BidType::Audio),
        MarkupType::NATIVE => Ok(BidType::Native),
        other => Err(BidderError::bad_server_response(format!(
            "Unsupported MType {} for impression with ID: \"{}\"",
            other.0, bid.impid
        ))),
    }
}

fn is_multi_format_imp(imp: &Imp) -> bool {
    let count = imp.banner.is_some() as u8 + imp.video.is_some() as u8 + imp.audio.is_some() as u8 + imp.native.is_some() as u8;
    count > 1
}
