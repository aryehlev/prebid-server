//! Go `adapters/kobler/kobler.go`.

use serde::Deserialize;
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{
    is_response_status_code_no_content, Bidder, BidderResponse, ExtraRequestInfo, RequestData,
    ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

const DEV_BIDDER_ENDPOINT: &str = "https://bid-service.dev.essrtb.com/bid/prebid_server_rtb_call";
const SUPPORTED_CURRENCY: &str = "USD";

/// Go `openrtb_ext.ExtImpKobler`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtImpKobler {
    test: bool,
}

pub struct Adapter {
    endpoint: String,
    dev_endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into(), dev_endpoint: DEV_BIDDER_ENDPOINT.to_string() }
    }
}

/// Go: `jsonutil.Unmarshal(imp.Ext, &ExtImpBidder)` then `jsonutil.Unmarshal(bidderExt.Bidder, &T)`.
/// Returns the Go error message text of whichever step fails.
fn imp_bidder_params<T: serde::de::DeserializeOwned>(ext: Option<&Ext>) -> Result<T, String> {
    fn not_obj(v: &sonic_rs::Value) -> Option<String> {
        if v.is_object() || v.is_null() {
            return None;
        }
        let first = sonic_rs::to_string(v).ok()?.chars().next().unwrap_or('\u{0}');
        Some(format!("expect {{ or n, but found {first}"))
    }
    let Some(ext) = ext else {
        return Err("expect { or n, but found \u{0}".to_string());
    };
    if let Some(m) = not_obj(&ext.0) {
        return Err(m);
    }
    match ext.0.get("bidder") {
        None => Err("expect { or n, but found \u{0}".to_string()),
        Some(b) => {
            if let Some(m) = not_obj(b) {
                return Err(m);
            }
            sonic_rs::from_value::<T>(b).map_err(|e| e.to_string())
        }
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors = Vec::new();
        let mut test_mode = false;

        let mut sanitized = sanitize_request(request);
        if !sanitized.cur.iter().any(|c| c == SUPPORTED_CURRENCY) {
            sanitized.cur.push(SUPPORTED_CURRENCY.to_string());
        }

        for i in 0..sanitized.imp.len() {
            if let Err(e) = convert_imp_currency(&mut sanitized.imp[i], req_info) {
                return (vec![], vec![e]);
            }

            if i == 0 && sanitized.imp[i].ext.is_some() {
                match imp_bidder_params::<ExtImpKobler>(sanitized.imp[i].ext.as_ref()) {
                    Ok(e) => test_mode = e.test,
                    Err(_) => {
                        // Go cannot tell the two unmarshal steps apart in the message either way
                        // for a non-object ext; keep its two texts.
                        let step1_failed = sanitized.imp[i]
                            .ext
                            .as_ref()
                            .is_some_and(|e| !e.0.is_object() && !e.0.is_null());
                        errors.push(BidderError::bad_input(if step1_failed {
                            "Error parsing bidderExt object"
                        } else {
                            "Error parsing impExt object"
                        }));
                        continue;
                    }
                }
            }
        }

        let body = match crate::go_json::to_vec(&sanitized) {
            Ok(b) => b,
            Err(e) => {
                errors.push(BidderError::FailedToMarshal(e.to_string()));
                return (vec![], errors);
            }
        };

        let endpoint = if test_mode { self.dev_endpoint.clone() } else { self.endpoint.clone() };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");

        (
            vec![RequestData {
                method: "POST".into(),
                uri: endpoint,
                body,
                headers,
                imp_ids: sanitized.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            // Go returns a nil error slice here even when imps failed to parse above.
            vec![],
        )
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        // Go checks `responseData.Body == nil`; an absent body is an empty one here.
        if is_response_status_code_no_content(response_data) || response_data.body.is_empty() {
            return (None, vec![]);
        }
        if response_data.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {}. Run with request.debug = 1 for more info.",
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
                let t = get_media_type_for_bid(&bid);
                bid_response.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type_for_bid(bid: &Bid) -> BidType {
    if let Some(ext) = &bid.ext {
        if let Some(t) = ext.0.get("prebid").and_then(|p| p.get("type")).and_then(|t| t.as_str()) {
            if let Ok(mt) = BidType::parse(t) {
                return mt;
            }
        }
    }
    BidType::Banner
}

fn convert_imp_currency(imp: &mut Imp, req_info: &ExtraRequestInfo) -> Result<(), BidderError> {
    if imp.bidfloor > 0.0
        && !imp.bidfloorcur.is_empty()
        && imp.bidfloorcur.to_uppercase() != SUPPORTED_CURRENCY
    {
        let converted = req_info.convert_currency(imp.bidfloor, &imp.bidfloorcur, SUPPORTED_CURRENCY)?;
        imp.bidfloor = converted;
        imp.bidfloorcur = SUPPORTED_CURRENCY.to_string();
    }
    Ok(())
}

fn sanitize_request(request: &BidRequest) -> BidRequest {
    let mut r = request.clone();
    if let Some(d) = r.device.as_mut() {
        d.ip = String::new();
        d.ipv6 = String::new();
    }
    r.user = None;
    r
}
