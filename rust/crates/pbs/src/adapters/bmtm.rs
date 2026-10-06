//! Go `adapters/bmtm/brightmountainmedia.go`.

use serde::Deserialize;
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{
    is_response_status_code_no_content, Bidder, BidderResponse, ExtraRequestInfo, RequestData,
    ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

/// Go `openrtb_ext.ImpExtBmtm`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ImpExtBmtm {
    #[serde(rename = "placement_id", deserialize_with = "crate::ortb::de::int")]
    placement_id: i64,
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
        if imp.banner.is_none() && imp.video.is_none() {
            return Err(BidderError::bad_input(format!(
                "For Imp ID {} Banner or Video is undefined",
                imp.id
            )));
        }
        let ext: ImpExtBmtm = imp_bidder_params(imp.ext.as_ref()).map_err(|m| {
            BidderError::bad_input(format!("Error unmarshalling ExtImpBidder: {m}"))
        })?;

        let mut imp = imp.clone();
        imp.tagid = ext.placement_id.to_string();
        imp.ext = None;
        let mut req = request.clone();
        req.imp = vec![imp];

        let body = crate::go_json::to_vec(&req).map_err(|e| BidderError::other(e.to_string()))?;
        Ok(RequestData {
            method: "POST".into(),
            uri: self.endpoint.clone(),
            body,
            headers: set_headers(&req),
            imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
        })
    }
}

/// Go: `jsonutil.Unmarshal(imp.Ext, &ExtImpBidder)` then `jsonutil.Unmarshal(bidderExt.Bidder, &T)`.
/// Returns the Go error message text of whichever step fails.
fn imp_bidder_params<T: serde::de::DeserializeOwned>(ext: Option<&Ext>) -> Result<T, String> {
    fn not_obj(v: &sonic_rs::Value) -> Option<String> {
        if v.is_object() || v.is_null() {
            return None;
        }
        let first = sonic_rs::to_string(v).ok()?.chars().next().unwrap_or('\u{0}');
        Some(format!("expect {{ or n, but found {first}"))
    }
    let Some(ext) = ext else {
        return Err("expect { or n, but found \u{0}".to_string());
    };
    if let Some(m) = not_obj(&ext.0) {
        return Err(m);
    }
    match ext.0.get("bidder") {
        None => Err("expect { or n, but found \u{0}".to_string()),
        Some(b) => {
            if let Some(m) = not_obj(b) {
                return Err(m);
            }
            sonic_rs::from_value::<T>(b).map_err(|e| e.to_string())
        }
    }
}

fn set_headers(request: &BidRequest) -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");
    if let Some(device) = &request.device {
        if !device.ua.is_empty() {
            headers.add("User-Agent", device.ua.clone());
        }
        if !device.ip.is_empty() {
            headers.add("X-Forwarded-For", device.ip.clone());
        } else if !device.ipv6.is_empty() {
            headers.add("X-Forwarded-For", device.ipv6.clone());
        }
    }
    if let Some(site) = &request.site {
        if !site.page.is_empty() {
            headers.add("Referer", site.page.clone());
        }
    }
    headers
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut reqs = Vec::new();
        let mut errs = Vec::new();
        for imp in &request.imp {
            match self.make_request(request, imp) {
                Ok(r) => reqs.push(r),
                Err(e) => errs.push(e),
            }
        }
        (reqs, errs)
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response) {
            return (None, vec![]);
        }
        if response.status_code == 400 {
            return (None, vec![BidderError::bad_input(format!("Unknown status code: {}.", response.status_code))]);
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!("Unknown status code: {}.", response.status_code))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut out = BidderResponse::with_bids_capacity(1);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let t = get_media_type_for_bid(&bid.impid, &request.imp);
                out.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(out), vec![])
    }
}

fn get_media_type_for_bid(imp_id: &str, imps: &[Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return BidType::Banner;
            } else if imp.video.is_some() {
                return BidType::Video;
            }
        }
    }
    BidType::Banner
}
