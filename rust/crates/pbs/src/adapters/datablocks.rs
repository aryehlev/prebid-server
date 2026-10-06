//! Go `adapters/datablocks/datablocks.go`.

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

/// `jsonutil.Unmarshal` of an absent `json.RawMessage`: jsoniter reads a NUL byte and reports
/// `expect { or n, but found \u{0}` where serde would say EOF.
fn unmarshal_raw<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T, BidderError> {
    if data.iter().all(|b| b" \t\r\n".contains(b)) {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()));
    }
    jsonutil::unmarshal(data)
}

/// `Unmarshal(imp.Ext, &bidderExt)` then `Unmarshal(bidderExt.Bidder, &params)`.
fn parse_bidder_ext<T: serde::de::DeserializeOwned>(imp: &Imp) -> Result<T, BidderError> {
    let outer: ExtImpBidder = unmarshal_raw(&ext_bytes(&imp.ext))?;
    unmarshal_raw(&ext_bytes(&outer.bidder))
}

fn json_headers() -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");
    headers
}

#[derive(Deserialize, Default, Clone, PartialEq, Eq)]
#[serde(default)]
struct ExtImpDatablocks {
    #[serde(rename = "sourceId")]
    source_id: i64,
}

pub struct Adapter {
    endpoint_template: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl AsRef<str>) -> Result<Self, BidderError> {
        let endpoint_template = EndpointTemplate::parse(endpoint.as_ref())
            .map_err(|e| BidderError::other(format!("unable to parse endpoint url template: {e}")))?;
        Ok(Self { endpoint_template })
    }
}

fn get_bidder_params(imp: &Imp) -> Result<ExtImpDatablocks, BidderError> {
    let outer: ExtImpBidder = unmarshal_raw(&ext_bytes(&imp.ext))
        .map_err(|e| BidderError::bad_input(format!("Missing bidder ext: {e}")))?;
    let ext: ExtImpDatablocks = unmarshal_raw(&ext_bytes(&outer.bidder))
        .map_err(|e| BidderError::bad_input(format!("Cannot Resolve sourceId: {e}")))?;
    if ext.source_id < 1 {
        return Err(BidderError::bad_input("Invalid/Missing SourceId"));
    }
    Ok(ext)
}

/// Go `splitImpressions`. Go iterates a map (random order); groups keep first-seen order here.
fn split_impressions(imps: &[Imp]) -> Result<Vec<(ExtImpDatablocks, Vec<Imp>)>, BidderError> {
    let mut groups: Vec<(ExtImpDatablocks, Vec<Imp>)> = Vec::new();
    for imp in imps {
        let params = get_bidder_params(imp)?;
        match groups.iter_mut().find(|(k, _)| *k == params) {
            Some((_, v)) => v.push(imp.clone()),
            None => groups.push((params, vec![imp.clone()])),
        }
    }
    Ok(groups)
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json");
        headers.add("Accept", "application/json");

        let groups = match split_impressions(&request.imp) {
            Ok(g) => g,
            Err(e) => {
                errs.push(e);
                Vec::new()
            }
        };

        let mut requests = Vec::new();
        let mut req = request.clone();
        for (ext, imps) in groups {
            req.imp = imps;
            let body = match crate::go_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::other(e.to_string()));
                    continue;
                }
            };
            let params = EndpointTemplateParams { source_id: ext.source_id.to_string(), ..Default::default() };
            let uri = match self.endpoint_template.resolve(&params) {
                Ok(u) => u,
                Err(e) => {
                    errs.push(BidderError::other(e));
                    continue;
                }
            };
            requests.push(RequestData {
                method: "POST".into(),
                uri,
                body,
                headers: headers.clone(),
                imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
            });
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
        if response_data.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "ERR, response with status {}",
                    response_data.status_code
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut out = BidderResponse::new();
        out.currency = bid_resp.cur.clone();
        for seat_bid in bid_resp.seatbid {
            for bid in seat_bid.bid {
                let t = get_media_type(&bid.impid, &request.imp);
                out.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(out), vec![])
    }
}

fn get_media_type(imp_id: &str, imps: &[Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.video.is_some() {
                return BidType::Video;
            } else if imp.native.is_some() {
                return BidType::Native;
            }
            return BidType::Banner;
        }
    }
    BidType::Banner
}
