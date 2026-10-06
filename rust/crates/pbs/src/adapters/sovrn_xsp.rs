//! Go `adapters/sovrnXsp/sovrnXsp.go`.

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, MarkupType, Publisher};
use serde::Deserialize;

/// Go `openrtb_ext.ExtImpSovrnXsp`.
#[derive(Debug, Default, Deserialize)]
struct ExtImpSovrnXsp {
    #[serde(default)]
    pub_id: String,
    #[serde(default)]
    med_id: String,
    #[serde(default)]
    zone_id: String,
    #[serde(default)]
    #[allow(dead_code)]
    force_bid: bool,
}

/// Go `bidExt`: the XSP bid extension.
#[derive(Debug, Default, Deserialize)]
struct BidExt {
    #[serde(default)]
    creative_type: i64,
}

const CREATIVE_TYPE_BANNER: i64 = 0;
const CREATIVE_TYPE_VIDEO: i64 = 1;
const CREATIVE_TYPE_NATIVE: i64 = 2;
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
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        // Go copies request.App, mutating the copy; it dereferences a nil App (panic), which is
        // reported as an error here.
        let Some(app) = request.app.as_ref() else {
            return (vec![], vec![BidderError::bad_input("no app in the request")]);
        };
        let mut request = request.clone();
        let mut app = app.clone();
        if app.publisher.is_none() {
            app.publisher = Some(Publisher::default());
        }
        let mut errors = Vec::new();
        let mut imps = Vec::new();
        for (idx, imp) in std::mem::take(&mut request.imp).into_iter().enumerate() {
            let mut imp = imp;
            if imp.banner.is_none() && imp.video.is_none() && imp.native.is_none() {
                continue;
            }
            let bidder_ext: ExtImpBidder = match decode_ext(imp.ext.as_ref()) {
                Ok(v) => v,
                Err(_) => {
                    errors.push(BidderError::bad_input(format!("imp #{idx}: ext.bidder not provided")));
                    continue;
                }
            };
            let xsp_ext: ExtImpSovrnXsp = match decode_ext(bidder_ext.bidder.as_ref()) {
                Ok(v) => v,
                Err(e) => {
                    errors.push(BidderError::bad_input(format!("imp #{idx}: {e}")));
                    continue;
                }
            };
            if let Some(p) = app.publisher.as_mut() {
                p.id = xsp_ext.pub_id;
            }
            if !xsp_ext.med_id.is_empty() {
                app.id = xsp_ext.med_id;
            }
            if !xsp_ext.zone_id.is_empty() {
                imp.tagid = xsp_ext.zone_id;
            }
            imps.push(imp);
        }
        if imps.is_empty() {
            errors.push(BidderError::bad_input("no matching impression with ad format"));
            return (vec![], errors);
        }
        request.app = Some(app);
        request.imp = imps;
        let body = match crate::go_json::to_vec(&request) {
            Ok(b) => b,
            Err(e) => {
                errors.push(BidderError::other(e.to_string()));
                return (vec![], errors);
            }
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        headers.add("x-openrtb-version", "2.5");
        let data = RequestData {
            method: "POST".into(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
        };
        (vec![data], errors)
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
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut errors = Vec::new();
        let mut result = BidderResponse::with_bids_capacity(request.imp.len());
        for seat_bid in response.seatbid {
            for mut bid in seat_bid.bid {
                let ext: BidExt = match decode_ext(bid.ext.as_ref()) {
                    Ok(v) => v,
                    Err(e) => {
                        errors.push(e);
                        continue;
                    }
                };
                let (bid_type, mkup) = match ext.creative_type {
                    CREATIVE_TYPE_BANNER => (BidType::Banner, MarkupType::BANNER),
                    CREATIVE_TYPE_VIDEO => (BidType::Video, MarkupType::VIDEO),
                    CREATIVE_TYPE_NATIVE => (BidType::Native, MarkupType::NATIVE),
                    other => {
                        errors.push(BidderError::bad_server_response(format!("Unsupported creative type: {other}")));
                        continue;
                    }
                };
                if bid.mtype == MarkupType(0) {
                    bid.mtype = mkup;
                }
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        if result.bids.is_empty() {
            // it's possible an empty seat array was sent as a response
            return (None, errors);
        }
        (Some(result), errors)
    }
}
