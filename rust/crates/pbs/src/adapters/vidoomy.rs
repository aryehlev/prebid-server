//! Go `adapters/vidoomy/vidoomy.go`.

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

fn get_headers(request: &BidRequest) -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");
    headers.add("x-openrtb-version", "2.5");
    let Some(device) = &request.device else {
        return headers;
    };
    if !device.ua.is_empty() {
        headers.set("User-Agent", device.ua.clone());
    }
    if !device.ipv6.is_empty() {
        headers.add("X-Forwarded-For", device.ipv6.clone());
    }
    if !device.ip.is_empty() {
        headers.add("X-Forwarded-For", device.ip.clone());
    }
    headers
}

fn change_request_for_bid_service(request: &mut BidRequest) -> Result<(), BidderError> {
    // Go indexes `request.Imp[0]`, which always exists here (one imp per split request).
    let Some(banner) = request.imp.first_mut().and_then(|i| i.banner.as_mut()) else {
        return Ok(());
    };
    if let (Some(w), Some(h)) = (banner.w, banner.h) {
        if w == 0 || h == 0 {
            return Err(BidderError::other(format!("invalid sizes provided for Banner {w} x {h}")));
        }
        return Ok(());
    }
    let Some(f) = banner.format.first() else {
        return Err(BidderError::other("no sizes provided for Banner []"));
    };
    banner.w = Some(f.w);
    banner.h = Some(f.h);
    Ok(())
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors = vec![];
        let mut reqs = Vec::with_capacity(request.imp.len());
        let header = get_headers(request);
        for imp in &request.imp {
            // Split multi-impression requests so each is associated with a single impression.
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];
            if let Err(e) = change_request_for_bid_service(&mut req_copy) {
                errors.push(e);
                continue;
            }
            let body = match crate::go_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(BidderError::other(e.to_string()));
                    continue;
                }
            };
            reqs.push(RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers: header.clone(),
                imp_ids: vec![imp.id.clone()],
            });
        }
        (reqs, errors)
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 204 {
            return (None, vec![]);
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {}.",
                    response.status_code
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => {
                // Go formats the error pointer with `%d`, which prints the struct fields.
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "Bad server response: &{{%!d(string={})}}.",
                        e.message()
                    ))],
                );
            }
        };
        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let Some(media_type) = get_imp_info(&bid.impid, &request.imp) else {
                    return (
                        None,
                        vec![BidderError::bad_server_response(format!(
                            "Unknown ad unit code '{}'",
                            bid.impid
                        ))],
                    );
                };
                // Only banner and video are supported, anything else is ignored.
                let Some(media_type) = media_type else { continue };
                bid_response.bids.push(TypedBid::new(bid, media_type));
            }
        }
        (Some(bid_response), vec![])
    }
}

/// `None`: imp not found; `Some(None)`: found, neither video nor banner.
fn get_imp_info(imp_id: &str, imps: &[Imp]) -> Option<Option<BidType>> {
    for imp in imps {
        if imp.id == imp_id {
            return Some(if imp.video.is_some() {
                Some(BidType::Video)
            } else if imp.banner.is_some() {
                Some(BidType::Banner)
            } else {
                None
            });
        }
    }
    None
}
