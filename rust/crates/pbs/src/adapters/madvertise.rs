//! Go `adapters/madvertise/madvertise.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::adcom1::CreativeAttribute;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

pub struct Adapter {
    endpoint_template: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`. Like Go, a template such as `{{Malformed}}` builds and only fails when it
    /// is resolved in `make_requests`.
    pub fn new(endpoint: &str) -> Result<Self, String> {
        let endpoint_template = EndpointTemplate::parse(endpoint)
            .map_err(|e| format!("unable to parse endpoint url template: {e}"))?;
        Ok(Self { endpoint_template })
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpMadvertise {
    #[serde(rename = "zoneId")]
    zone_id: String,
}

/// `jsonutil::unmarshal` with json-iterator's wording for empty input (nil ext).
fn unmarshal_obj<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T, BidderError> {
    if data.iter().all(|b| b" \t\r\n".contains(b)) {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()));
    }
    jsonutil::unmarshal(data)
}

fn ext_text(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

fn get_headers(request: &BidRequest) -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");
    headers.add("X-Openrtb-Version", "2.5");
    if let Some(d) = &request.device {
        if !d.ua.is_empty() {
            headers.add("User-Agent", d.ua.clone());
        }
        if !d.ip.is_empty() {
            headers.add("X-Forwarded-For", d.ip.clone());
        }
    }
    headers
}

fn get_impression_ext(imp: &Imp) -> Result<ExtImpMadvertise, BidderError> {
    let bidder_ext: ExtImpBidder = unmarshal_obj(&ext_text(&imp.ext))
        .map_err(|e| BidderError::bad_input(format!("{}; ImpID={}", e, imp.id)))?;
    let ext: ExtImpMadvertise = unmarshal_obj(&ext_text(&bidder_ext.bidder))
        .map_err(|e| BidderError::bad_input(format!("{}; ImpID={}", e, imp.id)))?;
    if ext.zone_id.is_empty() {
        return Err(BidderError::bad_input(format!("ext.bidder.zoneId not provided; ImpID={}", imp.id)));
    }
    Ok(ext)
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut zone_id = String::new();
        for imp in &request.imp {
            let ext = match get_impression_ext(imp) {
                Ok(e) => e,
                Err(e) => return (vec![], vec![e]),
            };
            // Go `len()` counts bytes.
            if ext.zone_id.len() < 7 {
                return (
                    vec![],
                    vec![BidderError::bad_input(format!(
                        "The minLength of zone ID is 7; ImpID={}",
                        imp.id
                    ))],
                );
            }
            if zone_id.is_empty() {
                zone_id = ext.zone_id;
            } else if zone_id != ext.zone_id {
                return (vec![], vec![BidderError::bad_input("There must be only one zone ID")]);
            }
        }
        let params = EndpointTemplateParams { zone_id, ..Default::default() };
        let url = match self.endpoint_template.resolve(&params) {
            Ok(u) => u,
            Err(e) => return (vec![], vec![BidderError::other(e)]),
        };
        let body = match crate::go_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        (
            vec![RequestData {
                method: "POST".into(),
                uri: url,
                body,
                headers: get_headers(request),
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response_data.status_code == 204 {
            return (None, vec![]);
        }
        if response_data.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {}. Run with request.debug = 1 for more info",
                    response_data.status_code
                ))],
            );
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        bid_response.currency = response.cur;
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                let t = get_media_type_for_bid(&bid.attr);
                bid_response.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type_for_bid(attr: &[CreativeAttribute]) -> BidType {
    for a in attr {
        if *a == CreativeAttribute::HAS_SKIP_BUTTON
            || *a == CreativeAttribute::VIDEO_AUTO
            || *a == CreativeAttribute::VIDEO_USER
        {
            return BidType::Video;
        }
    }
    BidType::Banner
}
