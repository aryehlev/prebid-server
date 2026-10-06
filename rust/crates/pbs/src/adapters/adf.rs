//! Go `adapters/adf/adf.go`.

use serde::{Deserialize, Deserializer};
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{
    is_response_status_code_no_content, Bidder, BidderResponse, ExtraRequestInfo, RequestData,
    ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

/// Go `json.Number`: kept as its text, accepting a number or a string.
fn json_number<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum V {
        S(String),
        N(serde_json::Number),
        Null(()),
    }
    Ok(match V::deserialize(d)? {
        V::S(s) => s,
        V::N(n) => n.to_string(),
        V::Null(()) => String::new(),
    })
}

/// Go `openrtb_ext.ExtImpAdf`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtImpAdf {
    #[serde(rename = "mid", deserialize_with = "json_number")]
    master_tag_id: String,
    #[serde(rename = "priceType")]
    price_type: String,
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
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors: Vec<BidderError> = Vec::new();
        let mut valid_imps: Vec<Imp> = Vec::with_capacity(request.imp.len());
        let mut price_type = String::new();

        for imp in &request.imp {
            let adf_ext: ExtImpAdf = match imp_bidder_params(imp.ext.as_ref()) {
                Ok(e) => e,
                Err(m) => {
                    errors.push(BidderError::bad_input(m));
                    continue;
                }
            };
            let mut imp = imp.clone();
            imp.tagid = adf_ext.master_tag_id;
            valid_imps.push(imp);
            if !adf_ext.price_type.is_empty() && price_type.is_empty() {
                price_type = adf_ext.price_type;
            }
        }

        // Go mutates the request in place; work on a clone.
        let mut request = request.clone();

        if !price_type.is_empty() {
            // Go decodes `request.Ext` into `adfRequestExt{ExtRequest, pt}` and re-marshals it, which
            // keeps only `prebid` (always written) and `schain`, then `pt`.
            let mut ok = true;
            let mut prebid: Option<String> = None;
            let mut schain: Option<String> = None;
            if let Some(ext) = request.ext.as_ref() {
                if !ext.0.is_object() && !ext.0.is_null() {
                    ok = false;
                    errors.push(BidderError::FailedToUnmarshal(format!(
                        "expect {{ or n, but found {}",
                        ext.to_json().chars().next().unwrap_or('\u{0}')
                    )));
                } else {
                    prebid = ext.0.get("prebid").filter(|v| !v.is_null()).map(|v| v.to_string());
                    schain = ext.0.get("schain").filter(|v| !v.is_null()).map(|v| v.to_string());
                }
            }
            if ok {
                let mut out = format!("{{\"prebid\":{}", prebid.unwrap_or_else(|| "{}".to_string()));
                if let Some(s) = schain {
                    out.push_str(&format!(",\"schain\":{s}"));
                }
                out.push_str(&format!(",\"pt\":{}}}", sonic_rs::to_string(&price_type).unwrap_or_default()));
                match Ext::from_slice(out.as_bytes()) {
                    Ok(e) => request.ext = Some(e),
                    Err(e) => errors.push(BidderError::other(e.to_string())),
                }
            }
        }

        request.imp = valid_imps;

        let body = match crate::go_json::to_vec(&request) {
            Ok(b) => b,
            Err(e) => {
                errors.push(BidderError::other(e.to_string()));
                return (vec![], errors);
            }
        };

        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers: Default::default(),
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            errors,
        )
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
        if response_data.status_code == 400 {
            return (
                None,
                vec![BidderError::bad_input("Unexpected status code: 400. Bad request from publisher.")],
            );
        }
        if response_data.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {}.",
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
        let mut errors = Vec::new();
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                match get_media_type_for_bid(&bid) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errors.push(e),
                }
            }
        }
        (Some(bid_response), errors)
    }
}

fn get_media_type_for_bid(bid: &Bid) -> Result<BidType, BidderError> {
    if let Some(ext) = &bid.ext {
        // Go unmarshals into `ExtBid`; `Prebid` is a pointer and `Type` a string.
        if let Some(prebid) = ext.0.get("prebid").filter(|p| p.is_object()) {
            let t = prebid.get("type").and_then(|t| t.as_str()).unwrap_or("");
            return BidType::parse(t).map_err(BidderError::other);
        }
    }
    Err(BidderError::bad_server_response(format!("Failed to parse impression \"{}\" mediatype", bid.impid)))
}
