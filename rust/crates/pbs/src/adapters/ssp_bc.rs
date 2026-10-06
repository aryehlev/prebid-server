//! Go `adapters/sspBC/sspbc.go`.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::bid_types::BidType;
use crate::bidder::{
    is_response_status_code_no_content, Bidder, BidderResponse, ExtraRequestInfo, RequestData,
    ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, MarkupType};

const ADAPTER_VERSION: &str = "6.0";

#[derive(Serialize)]
struct RequestInfo<'a> {
    #[serde(rename = "PbsEntryPoint")]
    pbs_entry_point: &'a str,
}

/// Go `requestData`.
#[derive(Serialize)]
struct SspBcRequest<'a> {
    #[serde(rename = "bidRequest")]
    request: &'a BidRequest,
    #[serde(rename = "requestInfo")]
    request_info: RequestInfo<'a>,
}

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: &str) -> Result<Self, BidderError> {
        let endpoint = build_adapter_endpoint(endpoint, ADAPTER_VERSION)
            .map_err(|e| BidderError::other(format!("unable to build sspbc adapter endpoint: {e}")))?;
        Ok(Self { endpoint })
    }
}

/// Go `url.QueryEscape`.
fn query_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Go `buildAdapterEndpoint`: add `bdver` to the query, which `Values.Encode` writes sorted by key.
fn build_adapter_endpoint(endpoint: &str, adapter_version: &str) -> Result<String, String> {
    let parsed = url::Url::parse(endpoint).map_err(|e| format!("unable to parse endpoint URL: {e}"))?;
    let mut params: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (k, v) in parsed.query_pairs() {
        params.entry(k.into_owned()).or_default().push(v.into_owned());
    }
    params.entry("bdver".to_string()).or_default().push(adapter_version.to_string());
    let query = params
        .iter()
        .flat_map(|(k, vs)| vs.iter().map(move |v| format!("{}={}", query_escape(k), query_escape(v))))
        .collect::<Vec<_>>()
        .join("&");

    let (rest, fragment) = match endpoint.split_once('#') {
        Some((r, f)) => (r, Some(f)),
        None => (endpoint, None),
    };
    let base = rest.split_once('?').map_or(rest, |(b, _)| b);
    let mut out = format!("{base}?{query}");
    if let Some(f) = fragment {
        out.push('#');
        out.push_str(f);
    }
    Ok(out)
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        extra: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let ssp_request = SspBcRequest {
            request,
            request_info: RequestInfo { pbs_entry_point: &extra.pbs_entry_point },
        };
        let body = match crate::go_json::to_vec(&ssp_request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers: Header::new(),
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        _external_request: &RequestData,
        external_response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(external_response) {
            return (None, vec![]);
        }
        if external_response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "unexpected status code: {}.",
                    external_response.status_code
                ))],
            );
        }
        let response: BidResponse = match jsonutil::unmarshal(&external_response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(internal_request.imp.len());
        bid_response.currency = response.cur;
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                match get_bid_type(&bid) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => return (None, vec![e]),
                }
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_bid_type(bid: &Bid) -> Result<BidType, BidderError> {
    match bid.mtype {
        MarkupType::BANNER => Ok(BidType::Banner),
        MarkupType::VIDEO => Ok(BidType::Video),
        MarkupType::AUDIO => Ok(BidType::Audio),
        MarkupType::NATIVE => Ok(BidType::Native),
        m => Err(BidderError::bad_server_response(format!("unsupported MType: {}.", m.0))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Go `TestInvalidEndpointURL`.
    #[test]
    fn invalid_endpoint_url_is_rejected() {
        assert!(Adapter::new("http://ssp.wp.test   /bidder/").is_err());
    }
}
