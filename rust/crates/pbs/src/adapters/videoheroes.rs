//! Go `adapters/videoheroes/videoheroes.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{
    Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
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

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpVideoHeroes {
    #[serde(rename = "placementId")]
    placement_id: String,
}

pub struct Adapter {
    endpoint: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl AsRef<str>) -> Result<Self, BidderError> {
        let endpoint = EndpointTemplate::parse(endpoint.as_ref())
            .map_err(|e| BidderError::other(format!("unable to parse endpoint url template: {e}")))?;
        Ok(Self { endpoint })
    }
}

fn get_impression_ext(imp: &Imp) -> Result<ExtImpVideoHeroes, BidderError> {
    parse_bidder_ext(imp).map_err(|_| BidderError::bad_input("ext.bidder not provided"))
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        // Go indexes `request.Imp[0]` and panics on an empty imp list; return an error instead.
        let Some(first) = request.imp.first() else {
            return (vec![], vec![BidderError::bad_input("no impressions in the request")]);
        };
        let ext = match get_impression_ext(first) {
            Ok(e) => e,
            Err(e) => return (vec![], vec![e]),
        };
        let mut req = request.clone();
        req.imp[0].ext = None;
        let body = match crate::go_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        let params = EndpointTemplateParams { publisher_id: ext.placement_id, ..Default::default() };
        let uri = match self.endpoint.resolve(&params) {
            Ok(u) => u,
            Err(e) => return (vec![], vec![BidderError::other(e)]),
        };
        (
            vec![RequestData {
                method: "POST".into(),
                uri,
                body,
                headers: json_headers(),
                imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
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
        let code = response_data.status_code;
        if code == 204 {
            return (None, vec![BidderError::bad_input("No bid")]);
        }
        if code == 400 {
            return (
                None,
                vec![BidderError::bad_input(format!(
                    "Unexpected status code: {code}. Run with request.debug = 1 for more info"
                ))],
            );
        }
        if code == 503 {
            return (
                None,
                vec![BidderError::bad_input(format!("Service Unavailable. Status Code: [ {code} ] "))],
            );
        }
        if code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Something went wrong, please contact your Account Manager. Status Code: [ {code} ] "
                ))],
            );
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(_) => return (None, vec![BidderError::bad_server_response("Bad Server Response")]),
        };
        let Some(first) = response.seatbid.into_iter().next() else {
            return (None, vec![BidderError::bad_server_response("Empty SeatBid array")]);
        };
        let mut out = BidderResponse::with_bids_capacity(first.bid.len());
        for bid in first.bid {
            let t = get_media_type_for_imp(&bid.impid, &request.imp);
            out.bids.push(TypedBid::new(bid, t));
        }
        (Some(out), vec![])
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> BidType {
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
