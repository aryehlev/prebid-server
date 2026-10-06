//! Go `adapters/admatic/admatic.go`.

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{Imp, BidRequest, BidResponse};
use serde::Deserialize;

/// Go `openrtb_ext.ImpExtAdmatic`.
#[derive(Debug, Default, Deserialize)]
struct ImpExtAdmatic {
    #[serde(default)]
    host: String,
    #[serde(rename = "networkId", default)]
    #[allow(dead_code)]
    network_id: i64,
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
/// `decode_ext` plus the json-iterator wording for a non-string `host` (the shared
/// `jsonutil::unmarshal` only reproduces top-level messages; see /tmp/pbs-needs-b01.md).
fn decode_admatic_ext(ext: Option<&crate::ortb::Ext>) -> Result<ImpExtAdmatic, BidderError> {
    use sonic_rs::JsonValueTrait;
    if let Some(host) = ext.filter(|e| e.0.is_object()).and_then(|e| e.0.get("host")) {
        if !host.is_str() && !host.is_null() {
            let found = host.to_string().chars().next().unwrap_or(' ');
            return Err(BidderError::FailedToUnmarshal(format!(
                "cannot unmarshal openrtb_ext.ImpExtAdmatic.Host: expects \" or n, but found {found}"
            )));
        }
    }
    decode_ext(ext)
}

pub struct Adapter {
    endpoint: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: &str) -> Result<Self, String> {
        let tmpl = EndpointTemplate::parse(endpoint)
            .map_err(|e| format!("unable to parse endpoint template: {e}"))?;
        Ok(Self { endpoint: tmpl })
    }

    fn build_endpoint_from_request(&self, imp: &Imp) -> Result<String, BidderError> {
        let imp_ext: ExtImpBidder = decode_ext(imp.ext.as_ref()).map_err(|e| {
            BidderError::bad_input(format!("Failed to deserialize bidder impression extension: {e}"))
        })?;
        let ext: ImpExtAdmatic = decode_admatic_ext(imp_ext.bidder.as_ref())
            .map_err(|e| BidderError::bad_input(format!("Failed to deserialize AdMatic extension: {e}")))?;
        let params = EndpointTemplateParams { host: ext.host, ..Default::default() };
        self.endpoint.resolve(&params).map_err(BidderError::other)
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
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
        for imp in &request.imp {
            let endpoint = match self.build_endpoint_from_request(imp) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            let mut copy = request.clone();
            copy.imp = vec![imp.clone()];
            let body = match crate::go_json::to_vec(&copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::other(e.to_string()));
                    continue;
                }
            };
            requests.push(RequestData {
                method: "POST".into(),
                uri: endpoint,
                body,
                headers: headers.clone(),
                imp_ids: vec![imp.id.clone()],
            });
        }
        (requests, errs)
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response_data) {
            return (None, vec![]);
        }
        if let Some(err) = check_response_status_code_for_errors(response_data) {
            return (None, vec![err]);
        }
        // json-iterator reports an empty body as `expect { or n, but found \0`; the shared
        // `jsonutil::unmarshal` falls through to serde's EOF text.
        if response_data.body.iter().all(|b| b" \t\r\n".contains(b)) {
            return (None, vec![BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into())]);
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        if !response.cur.is_empty() {
            bid_response.currency = response.cur.clone();
        }
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                let t = match get_media_type_for_bid(&bid.impid, &request.imp) {
                    Ok(t) => t,
                    Err(e) => return (None, vec![e]),
                };
                bid_response.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type_for_bid(imp_id: &str, imps: &[Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return Ok(BidType::Banner);
            } else if imp.video.is_some() {
                return Ok(BidType::Video);
            } else if imp.native.is_some() {
                return Ok(BidType::Native);
            }
        }
    }
    Err(BidderError::bad_server_response(format!("The impression with ID {imp_id} is not present into the request")))
}
