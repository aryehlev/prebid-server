//! Go `adapters/displayio/displayio.go`.

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse};
use crate::ortb::Ext;
use serde::{Deserialize, Serialize};
use sonic_rs::JsonValueTrait;

/// Go `openrtb_ext.ExtImpDisplayio`.
#[derive(Debug, Default, Deserialize)]
struct ExtImpDisplayio {
    #[serde(rename = "publisherId", default)]
    publisher_id: String,
    #[serde(rename = "inventoryId", default)]
    inventory_id: String,
    #[serde(rename = "placementId", default)]
    placement_id: String,
}

#[derive(Serialize)]
struct ReqDioExt {
    #[serde(rename = "userSession", skip_serializing_if = "String::is_empty")]
    user_session: String,
    #[serde(rename = "placementId")]
    placement_id: String,
    #[serde(rename = "inventoryId")]
    inventory_id: String,
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

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        request_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        headers.add("x-openrtb-version", "2.5");

        let mut result = Vec::with_capacity(request.imp.len());
        let mut errs = Vec::new();
        for impression in &request.imp {
            let mut impression = impression.clone();
            if impression.bidfloorcur.is_empty() || impression.bidfloor == 0.0 {
                impression.bidfloorcur = "USD".into();
            } else if impression.bidfloorcur != "USD" {
                match request_info.convert_currency(impression.bidfloor, &impression.bidfloorcur, "USD") {
                    Ok(v) => {
                        impression.bidfloor = v;
                        impression.bidfloorcur = "USD".into();
                    }
                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                }
            }
            if impression.ext.is_none() {
                errs.push(BidderError::other("impression extensions required"));
                continue;
            }
            let bidder_ext: ExtImpBidder = match decode_ext(impression.ext.as_ref()) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            let impression_ext: ExtImpDisplayio = match decode_ext(bidder_ext.bidder.as_ref()) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            let dio_ext = ReqDioExt {
                user_session: String::new(),
                placement_id: impression_ext.placement_id.clone(),
                inventory_id: impression_ext.inventory_id.clone(),
            };
            // Go unmarshals request.ext into a map (a failure leaves an empty map), sets
            // `displayio` and marshals it back (keys sorted).
            let mut request_ext: serde_json::Map<String, serde_json::Value> = match request.ext.as_ref() {
                Some(e) if e.0.is_object() => {
                    serde_json::from_str(&e.to_json()).unwrap_or_default()
                }
                _ => serde_json::Map::new(),
            };
            match serde_json::to_value(&dio_ext) {
                Ok(v) => {
                    request_ext.insert("displayio".into(), v);
                }
                Err(e) => {
                    errs.push(BidderError::other(e.to_string()));
                    continue;
                }
            }
            let mut request_copy = request.clone();
            request_copy.ext = match Ext::from_serialize(&request_ext) {
                Ok(e) => Some(e),
                Err(e) => {
                    errs.push(BidderError::other(e.to_string()));
                    continue;
                }
            };
            request_copy.imp = vec![impression];
            let body = match crate::go_json::to_vec(&request_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::other(e.to_string()));
                    continue;
                }
            };
            let params = EndpointTemplateParams { publisher_id: impression_ext.publisher_id, ..Default::default() };
            let url = match self.endpoint.resolve(&params) {
                Ok(u) => u,
                Err(e) => return (vec![], vec![BidderError::other(e)]),
            };
            result.push(RequestData {
                method: "POST".into(),
                uri: url,
                body,
                headers: headers.clone(),
                imp_ids: request_copy.imp.iter().map(|i| i.id.clone()).collect(),
            });
        }
        if result.is_empty() {
            return (vec![], errs);
        }
        (result, errs)
    }

    fn make_bids(
        &self,
        _internal_request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response_data) {
            return (None, vec![]);
        }
        if let Some(err) = check_response_status_code_for_errors(response_data) {
            return (None, vec![err]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => {
                // Go formats the error with `%d`: `&{%!d(string=msg)}`.
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "Bad server response: &{{%!d(string={})}}",
                        e.message()
                    ))],
                );
            }
        };
        if bid_resp.seatbid.len() != 1 {
            return (
                None,
                vec![BidderError::bad_server_response(format!("Invalid SeatBids count: {}", bid_resp.seatbid.len()))],
            );
        }
        let mut errs = Vec::new();
        let mut bid_response = BidderResponse::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_bid_media_type_from_mtype(&bid) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        (Some(bid_response), errs)
    }
}

fn get_bid_media_type_from_mtype(bid: &Bid) -> Result<BidType, BidderError> {
    use crate::ortb::openrtb2::MarkupType;
    match bid.mtype {
        MarkupType::BANNER => Ok(BidType::Banner),
        MarkupType::VIDEO => Ok(BidType::Video),
        _ => Err(BidderError::other(format!("unexpected media type for bid: {}", bid.impid))),
    }
}
