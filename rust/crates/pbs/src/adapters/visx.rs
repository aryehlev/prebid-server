//! Go `adapters/visx/visx.go`.

use serde::Deserialize;
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{
    Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, Imp};
use crate::ortb::Ext;

/// Go `visxBid`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct VisxBid {
    impid: String,
    price: f64,
    crid: String,
    adm: String,
    adomain: Vec<String>,
    dealid: String,
    w: u64,
    h: u64,
    ext: Option<Ext>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct VisxSeatBid {
    bid: Vec<VisxBid>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct VisxResponse {
    seatbid: Vec<VisxSeatBid>,
    cur: String,
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

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut request_copy = request.clone();
        if request_copy.cur.is_empty() {
            request_copy.cur = vec!["USD".to_string()];
        }
        let body = match crate::go_json::to_vec(&request_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };

        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        if let Some(device) = &request.device {
            if !device.ip.is_empty() {
                headers.add("X-Forwarded-For", device.ip.clone());
            }
            if !device.ipv6.is_empty() {
                headers.add("X-Forwarded-For", device.ipv6.clone());
            }
        }
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: request_copy.imp.iter().map(|i| i.id.clone()).collect(),
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
        if response.status_code == 204 {
            return (None, vec![]);
        }
        let msg = || {
            format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            )
        };
        if response.status_code == 400 {
            return (None, vec![BidderError::bad_input(msg())]);
        }
        if response.status_code != 200 {
            return (None, vec![BidderError::bad_server_response(msg())]);
        }
        let bid_resp: VisxResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };

        let mut out = BidderResponse::with_bids_capacity(1);
        for sb in bid_resp.seatbid {
            for vb in sb.bid {
                let bid_type = match get_media_type_for_imp(&vb.impid, &internal_request.imp, &vb) {
                    Ok(t) => t,
                    Err(e) => return (None, vec![e]),
                };
                let bid = Bid {
                    id: internal_request.id.clone(),
                    crid: vb.crid,
                    impid: vb.impid,
                    price: vb.price,
                    adm: vb.adm,
                    w: vb.w as i64,
                    h: vb.h as i64,
                    adomain: vb.adomain,
                    dealid: vb.dealid,
                    ext: vb.ext,
                    ..Default::default()
                };
                out.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        if !bid_resp.cur.is_empty() {
            out.currency = bid_resp.cur;
        }
        (Some(out), vec![])
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp], bid: &VisxBid) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            // Go unmarshals into `visxBidExt`; a failure just skips the meta lookup.
            if let Some(ext) = &bid.ext {
                if let Some(mt) = ext
                    .0
                    .get("prebid")
                    .and_then(|p| p.get("meta"))
                    .and_then(|m| m.get("mediaType"))
                    .and_then(|t| t.as_str())
                {
                    if mt == "banner" {
                        return Ok(BidType::Banner);
                    }
                    if mt == "video" {
                        return Ok(BidType::Video);
                    }
                }
            }
            if imp.banner.is_some() {
                return Ok(BidType::Banner);
            }
            if imp.video.is_some() {
                return Ok(BidType::Video);
            }
            return Err(BidderError::bad_server_response(format!("Unknown impression type for ID: \"{imp_id}\"")));
        }
    }
    Err(BidderError::bad_server_response(format!("Failed to find impression for ID: \"{imp_id}\"")))
}
