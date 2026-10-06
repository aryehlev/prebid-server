//! Go `adapters/lm_kiviads`.

use serde::Deserialize;
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder,
    BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
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
    pub fn new(endpoint: &str) -> Result<Self, BidderError> {
        let template = EndpointTemplate::parse(endpoint).map_err(|e| {
            BidderError::other(format!("unable to parse endpoint URL template: {e}"))
        })?;
        // Go's `template.Parse` rejects an action that is not a field (`{{Malformed}}`).
        template.resolve(&EndpointTemplateParams::default()).map_err(|e| {
            BidderError::other(format!("unable to parse endpoint URL template: {e}"))
        })?;
        Ok(Self { endpoint: template })
    }

    fn build_endpoint_from_request(&self, imp: &Imp) -> Result<String, BidderError> {
        let imp_ext: ExtImpBidder = unmarshal_ext(imp.ext.as_ref()).map_err(|e| {
            BidderError::bad_input(format!("Failed to deserialize bidder impression extension: {e}"))
        })?;
        let ext: ExtLmKiviads = unmarshal_ext(imp_ext.bidder.as_ref()).map_err(|e| {
            BidderError::bad_input(format!("Failed to deserialize LmKiviads extension: {e}"))
        })?;
        let params = EndpointTemplateParams { host: ext.env, source_id: ext.pid, ..Default::default() };
        self.endpoint.resolve(&params).map_err(BidderError::other)
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtLmKiviads {
    env: String,
    pid: String,
}

/// Go `jsonutil.Unmarshal(raw, &v)` on a `json.RawMessage` that may be absent or not an object:
/// json-iterator reports `expect { or n, but found X` for anything but an object or `null`.
fn unmarshal_ext<T: serde::de::DeserializeOwned + Default>(ext: Option<&Ext>) -> Result<T, BidderError> {
    let Some(ext) = ext else {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into()));
    };
    if ext.0.is_null() {
        return Ok(T::default());
    }
    if !ext.0.is_object() {
        let text = ext.to_json();
        let first = text.chars().next().unwrap_or('\0');
        return Err(BidderError::FailedToUnmarshal(format!("expect {{ or n, but found {first}")));
    }
    ext.decode().map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidType_ {
    r#type: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidExt {
    prebid: BidType_,
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
        _request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response_data) {
            return (None, vec![]);
        }
        if response_data.status_code == 503 {
            return (
                None,
                vec![BidderError::bad_input("Bidder LmKiviads is unavailable. Please contact the bidder support.")],
            );
        }
        if let Some(err) = check_response_status_code_for_errors(response_data) {
            return (None, vec![err]);
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        if response.seatbid.is_empty() {
            return (None, vec![BidderError::bad_server_response("Array SeatBid cannot be empty")]);
        }
        let mut out = BidderResponse::with_bids_capacity(response.seatbid.len());
        let mut errs = Vec::new();
        for sb in response.seatbid {
            for (bid_id, bid) in sb.bid.into_iter().enumerate() {
                let bid_ext: BidExt = match unmarshal_ext(bid.ext.as_ref()) {
                    Ok(e) => e,
                    Err(e) => {
                        errs.push(BidderError::bad_server_response(format!(
                            "Failed to parse Bid[{bid_id}].Ext: {e}"
                        )));
                        continue;
                    }
                };
                match BidType::parse(&bid_ext.prebid.r#type) {
                    Ok(t) => out.bids.push(TypedBid::new(bid, t)),
                    Err(_) => errs.push(BidderError::bad_server_response(format!(
                        "Bid[{bid_id}].Ext.Prebid.Type expects one of the following values: \'banner\', \'native\', \'video\', \'audio\', got \'{}\'",
                        bid_ext.prebid.r#type
                    ))),
                }
            }
        }
        (Some(out), errs)
    }
}
