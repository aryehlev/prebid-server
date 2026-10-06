//! Go `adapters/bematterfull/bematterfull.go`.

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp, SeatBid};
use crate::ortb::Ext;
use serde::Deserialize;

/// Go `openrtb_ext.ExtBematterfull`.
#[derive(Debug, Default, Deserialize)]
struct ExtBematterfull {
    #[serde(default)]
    env: String,
    #[serde(default)]
    pid: String,
}

#[derive(Debug, Default, Deserialize)]
struct BidTypeExt {
    #[serde(rename = "type", default)]
    r#type: String,
}

#[derive(Debug, Default, Deserialize)]
struct BidExt {
    #[serde(default)]
    prebid: BidTypeExt,
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
            .map_err(|e| format!("unable to parse endpoint URL template: {e}"))?;
        Ok(Self { endpoint: tmpl })
    }

    fn build_endpoint_from_request(&self, imp: &Imp) -> Result<String, BidderError> {
        let imp_ext: ExtImpBidder = decode_ext(imp.ext.as_ref()).map_err(|e| {
            BidderError::bad_input(format!("Failed to deserialize bidder impression extension: {e}"))
        })?;
        let ext: ExtBematterfull = decode_ext(imp_ext.bidder.as_ref())
            .map_err(|e| BidderError::bad_input(format!("Failed to deserialize Bematterfull extension: {e}")))?;
        let params = EndpointTemplateParams { host: ext.env, source_id: ext.pid, ..Default::default() };
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
                vec![BidderError::bad_input("Bidder Bematterfull is unavailable. Please contact the bidder support.")],
            );
        }
        if let Some(err) = check_response_status_code_for_errors(response_data) {
            return (None, vec![err]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        if bid_resp.seatbid.is_empty() {
            return (None, vec![BidderError::bad_server_response("Array SeatBid cannot be empty")]);
        }
        prepare_bid_response(bid_resp.seatbid)
    }
}

fn prepare_bid_response(seats: Vec<SeatBid>) -> (Option<BidderResponse>, Vec<BidderError>) {
    let mut errs = Vec::new();
    let mut bid_response = BidderResponse::with_bids_capacity(seats.len());
    for seat_bid in seats {
        for (bid_id, bid) in seat_bid.bid.into_iter().enumerate() {
            let bid_ext: BidExt = match decode_ext(bid.ext.as_ref()) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::bad_server_response(format!("Failed to parse Bid[{bid_id}].Ext: {e}")));
                    continue;
                }
            };
            let bid_type = match BidType::parse(&bid_ext.prebid.r#type) {
                Ok(t) => t,
                Err(_) => {
                    errs.push(BidderError::bad_server_response(format!(
                        "Bid[{bid_id}].Ext.Prebid.Type expects one of the following values: 'banner', 'native', 'video', 'audio', got '{}'",
                        bid_ext.prebid.r#type
                    )));
                    continue;
                }
            };
            bid_response.bids.push(TypedBid::new(bid, bid_type));
        }
    }
    (Some(bid_response), errs)
}

#[allow(dead_code)]
fn _unused(_: Ext) {}
