//! Go `adapters/impactify/impactify.go`.

use serde::{Deserialize, Serialize};

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
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
struct ExtImpImpactify {
    #[serde(rename = "appId")]
    app_id: String,
    format: String,
    style: String,
}

#[derive(Serialize, Default)]
struct ImpactifyExtBidder {
    impactify: ExtImpImpactify,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct DefaultExtBidder {
    bidder: ExtImpImpactify,
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return Ok(BidType::Banner);
            } else if imp.video.is_some() {
                return Ok(BidType::Video);
            }
        }
    }
    Err(BidderError::bad_input(format!(
        "Failed to find a supported media type impression \"{imp_id}\""
    )))
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        bid_request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        // Go mutates the request in place; mutate a clone instead.
        let mut req = bid_request.clone();
        for i in 0..req.imp.len() {
            let imp = &mut req.imp[i];
            // Imp with a bid floor in a foreign currency: convert to USD.
            if imp.bidfloor > 0.0 && !imp.bidfloorcur.is_empty() && imp.bidfloorcur.to_uppercase() != "USD" {
                match req_info.convert_currency(imp.bidfloor, &imp.bidfloorcur, "USD") {
                    Ok(v) => {
                        imp.bidfloorcur = "USD".into();
                        imp.bidfloor = v;
                    }
                    Err(e) => return (vec![], vec![e]),
                }
            }
            // Set the CUR of the bid to USD after converting all floors.
            req.cur = vec!["USD".into()];
            let imp = &mut req.imp[i];
            let decode_err = || {
                BidderError::bad_input(format!("Unable to decode the imp ext : \"{}\"", imp.id))
            };
            let default_ext: DefaultExtBidder = match &imp.ext {
                // Go: `jsonutil.Unmarshal(nil, ..)` fails on empty input.
                None => return (vec![], vec![decode_err()]),
                Some(e) => match e.decode() {
                    Ok(v) => v,
                    Err(_) => return (vec![], vec![decode_err()]),
                },
            };
            let out = ImpactifyExtBidder { impactify: default_ext.bidder };
            match Ext::from_serialize(&out) {
                Ok(e) => imp.ext = Some(e),
                Err(_) => return (vec![], vec![decode_err()]),
            }
        }
        if req.imp.is_empty() {
            return (vec![], vec![BidderError::bad_input("No valid impressions in the bid request")]);
        }
        let body = match crate::go_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        headers.add("x-openrtb-version", "2.5");
        if let Some(device) = &req.device {
            if !device.ua.is_empty() {
                headers.add("User-Agent", device.ua.clone());
            }
            // Add IPv4 or IPv6 if available.
            if !device.ip.is_empty() {
                headers.add("X-Forwarded-For", device.ip.clone());
            } else if !device.ipv6.is_empty() {
                headers.add("X-Forwarded-For", device.ipv6.clone());
            }
        }
        if let Some(site) = &req.site {
            headers.add("Referer", site.page.clone());
        }
        // Set the user's cookie.
        if let Some(user) = &req.user {
            if !user.buyeruid.is_empty() {
                headers.add("Cookie", format!("uids={}", user.buyeruid));
            }
        }
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
        internal_request: &BidRequest,
        _external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        match response.status_code {
            204 => return (None, vec![]),
            400 => return (None, vec![BidderError::bad_input("Invalid request.")]),
            200 => {}
            code => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!("Unexpected HTTP status {code}."))],
                )
            }
        }
        let rtb: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(_) => return (None, vec![BidderError::bad_server_response("Bad server body response")]),
        };
        let Some(sb) = rtb.seatbid.into_iter().next() else {
            return (None, vec![]);
        };
        let mut out = BidderResponse::with_bids_capacity(sb.bid.len());
        out.currency = rtb.cur;
        for bid in sb.bid {
            if !(bid.price > 0.0) {
                continue;
            }
            match get_media_type_for_imp(&bid.impid, &internal_request.imp) {
                Ok(t) => out.bids.push(TypedBid::new(bid, t)),
                Err(e) => return (None, vec![e]),
            }
        }
        (Some(out), vec![])
    }
}
