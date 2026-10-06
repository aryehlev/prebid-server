//! Go `adapters/telaria/telaria.go`.

use serde::{Deserialize, Serialize};

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Publisher};
use crate::ortb::Ext;

/// Go `telaria.Endpoint`, the default when the config has none.
pub const ENDPOINT: &str = "https://ads.tremorhub.com/ad/rtb/prebid";

pub struct Adapter {
    uri: String,
}

impl Adapter {
    /// Go `Builder`: an empty endpoint falls back to [`ENDPOINT`].
    pub fn new(endpoint: impl Into<String>) -> Self {
        let endpoint = endpoint.into();
        Self { uri: if endpoint.is_empty() { ENDPOINT.to_string() } else { endpoint } }
    }
}

#[derive(Serialize)]
struct ImpressionExtOut<'a> {
    #[serde(rename = "originalTagid")]
    original_tag_id: &'a str,
    #[serde(rename = "originalPublisherid")]
    original_publisher_id: &'a str,
}

#[derive(Serialize)]
struct TelariaBidExt<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    extra: Option<&'a Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpTelaria {
    #[serde(rename = "adCode")]
    ad_code: String,
    #[serde(rename = "seatCode")]
    seat_code: String,
    extra: Option<Ext>,
}

fn ext_text(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

fn get_headers(request: &BidRequest) -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");
    headers.add("X-Openrtb-Version", "2.5");
    if let Some(device) = &request.device {
        if !device.ua.is_empty() {
            headers.add("User-Agent", device.ua.clone());
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

fn fetch_original_publisher_id(request: &BidRequest) -> String {
    if let Some(p) = request.site.as_ref().and_then(|s| s.publisher.as_ref()) {
        return p.id.clone();
    }
    if request.site.is_none() {
        if let Some(p) = request.app.as_ref().and_then(|a| a.publisher.as_ref()) {
            return p.id.clone();
        }
    }
    String::new()
}

/// Deep copy of the publisher with the seat code as its id (Go copies only these fields).
fn make_publisher_object(seat_code: &str, publisher: Option<&Publisher>) -> Publisher {
    let mut pub_ = Publisher { id: seat_code.to_string(), ..Default::default() };
    if let Some(p) = publisher {
        pub_.domain = p.domain.clone();
        pub_.name = p.name.clone();
        pub_.cat = p.cat.clone();
        pub_.ext = p.ext.clone();
    }
    pub_
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request_in: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut request = request_in.clone();

        // CheckHasImps
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::bad_input("Telaria: Missing Imp Object")]);
        }
        // CheckHasVideoObject
        let mut has_video = false;
        for imp in &request.imp {
            if imp.banner.is_some() {
                return (vec![], vec![BidderError::bad_input("Telaria: Banner not supported")]);
            }
            has_video = has_video || imp.video.is_some();
        }
        if !has_video {
            return (vec![], vec![BidderError::bad_input("Telaria: Only Supports Video")]);
        }

        let original_publisher_id = fetch_original_publisher_id(&request);
        let mut imp = request.imp[0].clone();

        // FetchTelariaExtImpParams
        let bidder_ext: ExtImpBidder = match jsonutil::unmarshal(&ext_text(&imp.ext)) {
            Ok(e) => e,
            Err(_) => return (vec![], vec![BidderError::bad_input("Telaria: ext.bidder not provided")]),
        };
        let telaria_ext: ExtImpTelaria = match jsonutil::unmarshal(&ext_text(&bidder_ext.bidder)) {
            Ok(e) => e,
            Err(e) => return (vec![], vec![e]),
        };
        if telaria_ext.seat_code.is_empty() {
            return (vec![], vec![BidderError::bad_input("Telaria: Seat Code required")]);
        }
        let seat_code = telaria_ext.seat_code.clone();

        // Move the original tagId and publisher.id into imp.ext.
        imp.ext = match Ext::from_serialize(&ImpressionExtOut {
            original_tag_id: &imp.tagid,
            original_publisher_id: &original_publisher_id,
        }) {
            Ok(e) => Some(e),
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        // Swap the tagID with adCode.
        imp.tagid = telaria_ext.ad_code.clone();
        // Add the Extra from imp to the top-level ext.
        if telaria_ext.extra.is_some() {
            match Ext::from_serialize(&TelariaBidExt { extra: telaria_ext.extra.as_ref() }) {
                Ok(e) => request.ext = Some(e),
                Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
            }
        }
        request.imp = vec![imp];

        // PopulatePublisherId
        if let Some(site) = request.site.as_mut() {
            site.publisher = Some(make_publisher_object(&seat_code, site.publisher.as_ref()));
            request.app = None;
        } else if let Some(app) = request.app.as_mut() {
            app.publisher = Some(make_publisher_object(&seat_code, app.publisher.as_ref()));
        }

        let body = match crate::go_json::to_vec(&request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.uri.clone(),
                body,
                headers: get_headers(&request),
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if let Some(e) = check_response_status_codes(response) {
            return (None, vec![e]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(_) => return (None, vec![BidderError::bad_server_response("Telaria: Bad Server Response")]),
        };
        // Go indexes `SeatBid[0]` and panics on an empty seatbid; report an error instead.
        let Some(sb) = bid_resp.seatbid.into_iter().next() else {
            return (None, vec![BidderError::bad_server_response("Telaria: Bad Server Response")]);
        };
        let mut bid_response = BidderResponse::with_bids_capacity(sb.bid.len());
        for (i, mut bid) in sb.bid.into_iter().enumerate() {
            let Some(imp) = internal_request.imp.get(i) else { break };
            bid.impid = imp.id.clone();
            bid_response.bids.push(TypedBid::new(bid, BidType::Video));
        }
        (Some(bid_response), vec![])
    }
}

fn check_response_status_codes(response: &ResponseData) -> Option<BidderError> {
    let code = response.status_code;
    if code == 204 {
        return Some(BidderError::bad_input("Telaria: Invalid Bid Request received by the server"));
    }
    if code == 400 {
        return Some(BidderError::bad_input(format!("Telaria: Unexpected status code: [ {code} ] ")));
    }
    if code == 503 || code < 200 || code >= 300 {
        return Some(BidderError::bad_input(format!(
            "Telaria: Something went wrong, please contact your Account Manager. Status Code: [ {code} ] "
        )));
    }
    None
}
