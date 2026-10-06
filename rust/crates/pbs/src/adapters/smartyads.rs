//! Go `adapters/smartyads/smartyads.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

pub struct Adapter {
    endpoint: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: &str) -> Result<Self, String> {
        let endpoint = EndpointTemplate::parse(endpoint)
            .map_err(|e| format!("unable to parse endpoint url template: {e}"))?;
        Ok(Self { endpoint })
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtSmartyAds {
    accountid: String,
    sourceid: String,
    host: String,
}

fn ext_text(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

fn get_impression_ext(imp: &Imp) -> Result<ExtSmartyAds, BidderError> {
    let not_provided = || BidderError::bad_input("ext.bidder not provided");
    let bidder_ext: ExtImpBidder = jsonutil::unmarshal(&ext_text(&imp.ext)).map_err(|_| not_provided())?;
    jsonutil::unmarshal(&ext_text(&bidder_ext.bidder)).map_err(|_| not_provided())
}

/// Go `GetHeaders`.
pub fn get_headers(request: &BidRequest) -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");
    headers.add("X-Openrtb-Version", "2.5");
    if let Some(device) = &request.device {
        if !device.ua.is_empty() {
            headers.add("User-Agent", device.ua.clone());
        }
        if !device.ipv6.is_empty() {
            headers.add("X-Forwarded-For", device.ipv6.clone());
        }
        if !device.ip.is_empty() {
            headers.add("X-Forwarded-For", device.ip.clone());
        }
        if !device.language.is_empty() {
            headers.add("Accept-Language", device.language.clone());
        }
        if let Some(dnt) = device.dnt {
            headers.add("Dnt", dnt.to_string());
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
        let mut req = request.clone();
        let mut smartyads_ext: Option<ExtSmartyAds> = None;
        for imp in req.imp.iter_mut() {
            match get_impression_ext(imp) {
                Ok(e) => smartyads_ext = Some(e),
                Err(e) => return (vec![], vec![e]),
            }
            imp.ext = None;
        }
        // Go dereferences a nil ext when there are no imps and panics; return an error instead.
        let Some(ext) = smartyads_ext else {
            return (vec![], vec![BidderError::other("no impressions in the request")]);
        };
        let params = EndpointTemplateParams {
            host: ext.host,
            source_id: ext.sourceid,
            account_id: ext.accountid,
            ..Default::default()
        };
        let url = match self.endpoint.resolve(&params) {
            Ok(u) => u,
            Err(e) => return (vec![], vec![BidderError::other(e)]),
        };
        let body = match crate::go_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        (
            vec![RequestData {
                method: "POST".into(),
                uri: url,
                body,
                headers: get_headers(&req),
                imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if let Some(e) = check_response_status_codes(response) {
            return (None, vec![e]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(_) => return (None, vec![BidderError::bad_server_response("Bad Server Response")]),
        };
        let Some(sb) = bid_resp.seatbid.into_iter().next() else {
            return (None, vec![BidderError::bad_server_response("Empty SeatBid array")]);
        };
        let mut bid_response = BidderResponse::with_bids_capacity(sb.bid.len());
        for bid in sb.bid {
            let t = get_media_type_for_imp(&bid.impid, &request.imp);
            bid_response.bids.push(TypedBid::new(bid, t));
        }
        (Some(bid_response), vec![])
    }
}

fn check_response_status_codes(response: &ResponseData) -> Option<BidderError> {
    let code = response.status_code;
    if code == 204 {
        return Some(BidderError::bad_input("No bid response"));
    }
    if code == 400 {
        return Some(BidderError::bad_input(format!("Unexpected status code: [ {code} ]")));
    }
    if code == 503 || code < 200 || code >= 300 {
        return Some(BidderError::bad_input(format!(
            "Something went wrong, please contact your Account Manager. Status Code: [ {code} ] "
        )));
    }
    None
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> BidType {
    let mut media_type = BidType::Banner;
    for imp in imps {
        if imp.id == imp_id {
            if imp.video.is_some() {
                media_type = BidType::Video;
            } else if imp.native.is_some() {
                media_type = BidType::Native;
            }
            return media_type;
        }
    }
    media_type
}
