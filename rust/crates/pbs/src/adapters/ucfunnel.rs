//! Go `adapters/ucfunnel/ucfunnel.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse};

pub struct Adapter {
    uri: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { uri: endpoint.into() }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpUcfunnel {
    adunitid: String,
    partnerid: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtBidderUcfunnel {
    bidder: ExtImpUcfunnel,
}

/// Go `url.PathEscape`.
fn path_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b'$' | b'&' | b'+' | b'=' | b':' | b'@' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn get_partner_id(request: &BidRequest) -> Result<String, Vec<BidderError>> {
    // Go indexes `Imp[0]`; the empty case is rejected by the caller first.
    let ext: ExtBidderUcfunnel = match &request.imp[0].ext {
        Some(e) => e.decode().map_err(|e| vec![BidderError::FailedToUnmarshal(e.to_string())])?,
        None => return Err(vec![BidderError::FailedToUnmarshal("unexpected end of JSON input".into())]),
    };
    if ext.bidder.partnerid.is_empty() || ext.bidder.adunitid.is_empty() {
        return Err(vec![BidderError::other("No PartnerId or AdUnitId in the bid request\n")]);
    }
    Ok(ext.bidder.partnerid)
}

fn get_bid_type(bid_req: &BidRequest, impid: &str) -> BidType {
    for imp in &bid_req.imp {
        if imp.id == impid {
            if imp.banner.is_some() {
                return BidType::Banner;
            } else if imp.video.is_some() {
                return BidType::Video;
            } else if imp.audio.is_some() {
                return BidType::Audio;
            } else if imp.native.is_some() {
                return BidType::Native;
            }
        }
    }
    BidType::Native
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::bad_input("No impression in the bid request\n")]);
        }
        let partner_id = match get_partner_id(request) {
            Ok(p) => p,
            Err(errs) => return (vec![], errs),
        };
        let body = match crate::go_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: format!("{}/{}/request", self.uri, path_escape(&partner_id)),
                body,
                headers,
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        let msg = format!(
            "Unexpected status code: {}. Run with request.debug = 1 for more info",
            response.status_code
        );
        match response.status_code {
            204 => return (None, vec![]),
            400 => return (None, vec![BidderError::bad_input(msg)]),
            200 => {}
            _ => return (None, vec![BidderError::bad_server_response(msg)]),
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let bid_req: BidRequest = match jsonutil::unmarshal(&external_request.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        // Go indexes `SeatBid[0]` and panics on an empty seatbid; report an error instead.
        let Some(first) = bid_resp.seatbid.first() else {
            return (None, vec![BidderError::bad_server_response("no seatbid in the response")]);
        };
        let mut out = BidderResponse::with_bids_capacity(first.bid.len());
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_bid_type(&bid_req, &bid.impid);
                if bid_type == BidType::Banner || bid_type == BidType::Video {
                    out.bids.push(TypedBid::new(bid, bid_type));
                }
            }
        }
        (Some(out), vec![])
    }
}
