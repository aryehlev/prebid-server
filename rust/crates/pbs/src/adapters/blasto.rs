//! Go `adapters/blasto/blasto.go`.

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use serde::Deserialize;

/// Go `openrtb_ext.ExtBlasto`.
#[derive(Debug, Default, Deserialize)]
struct ExtBlasto {
    #[serde(rename = "accountId", default)]
    account_id: String,
    #[serde(rename = "sourceId", default)]
    source_id: String,
    #[serde(default)]
    #[allow(dead_code)]
    host: String,
    #[serde(rename = "placementId", default)]
    #[allow(dead_code)]
    placement_id: String,
}
/// Go `jsonutil.Unmarshal(ext, &target)` on a `json.RawMessage`; the failure text is the
/// json-iterator top-level one (`expect { or n, but found X`).
fn decode_ext<T: serde::de::DeserializeOwned>(ext: Option<&crate::ortb::Ext>) -> Result<T, BidderError> {
    let Some(ext) = ext else {
        // Go: Unmarshal of an empty RawMessage fails (never happens: PBS core validates imp.ext).
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into()));
    };
    use sonic_rs::JsonValueTrait;
    // json-iterator picks the object decoder from the first byte: anything but `{` / `null`
    // fails with its top-level message, while serde would accept an array for a struct.
    if ext.0.is_object() || ext.0.is_null() {
        if let Ok(v) = ext.decode::<T>() {
            return Ok(v);
        }
    }
    jsonutil::unmarshal::<T>(ext.to_json().as_bytes())
}
/// Go `adapters.ExtImpBidder` (only `bidder` is used).
#[derive(Debug, Default, serde::Deserialize)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<crate::ortb::Ext>,
}
pub struct Adapter {
    endpoint: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: &str) -> Result<Self, String> {
        let tmpl = EndpointTemplate::parse(endpoint)
            .map_err(|e| format!("unable to parse endpoint url template: {e}"))?;
        Ok(Self { endpoint: tmpl })
    }
}

fn get_headers(request: &BidRequest) -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");
    headers.add("X-Openrtb-Version", "2.5");
    if let Some(device) = request.device.as_ref() {
        if !device.ua.is_empty() {
            headers.add("User-Agent", device.ua.clone());
        }
        if !device.ipv6.is_empty() {
            headers.add("X-Forwarded-For", device.ipv6.clone());
        }
        if !device.ip.is_empty() {
            headers.add("X-Forwarded-For", device.ip.clone());
        }
    }
    headers
}

fn get_impression_ext(imp: &Imp) -> Result<ExtBlasto, BidderError> {
    let bidder_ext: ExtImpBidder =
        decode_ext(imp.ext.as_ref()).map_err(|_| BidderError::bad_input("ext.bidder not provided"))?;
    decode_ext(bidder_ext.bidder.as_ref()).map_err(|_| BidderError::bad_input("ext.bidder not provided"))
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        // Go indexes `Imp[0]` and panics on an empty imp list; report an error instead.
        let Some(first) = request.imp.first() else {
            return (vec![], vec![BidderError::bad_input("ext.bidder not provided")]);
        };
        let blasto_ext = match get_impression_ext(first) {
            Ok(e) => e,
            Err(e) => return (vec![], vec![e]),
        };
        let mut request = request.clone();
        for imp in request.imp.iter_mut() {
            imp.ext = None;
        }
        let params = EndpointTemplateParams {
            account_id: blasto_ext.account_id,
            source_id: blasto_ext.source_id,
            ..Default::default()
        };
        let url = match self.endpoint.resolve(&params) {
            Ok(u) => u,
            Err(e) => return (vec![], vec![BidderError::other(e)]),
        };
        let body = match crate::go_json::to_vec(&request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        (
            vec![RequestData {
                method: "POST".into(),
                uri: url,
                body,
                headers: get_headers(&request),
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
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
        let code = response.status_code;
        if code == 204 {
            return (None, vec![]);
        }
        if code == 400 {
            return (None, vec![BidderError::bad_input(format!("Unexpected status code: [ {code} ]"))]);
        }
        if code == 503 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Something went wrong, please contact your Account Manager. Status Code: [ {code} ] "
                ))],
            );
        }
        if code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: [ {code} ]. Run with request.debug = 1 for more info"
                ))],
            );
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
