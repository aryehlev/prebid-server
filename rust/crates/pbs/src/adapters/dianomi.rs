//! Go `adapters/dianomi/dianomi.go`.

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse};
use crate::ortb::Ext;
use serde::{Deserialize, Deserializer};
use sonic_rs::JsonValueTrait;

/// Go `json.Number` field: accepts a JSON number or a quoted number, keeps the text.
fn json_number<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    struct V;
    impl serde::de::Visitor<'_> for V {
        type Value = String;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a number")
        }
        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_unit<E: serde::de::Error>(self) -> Result<String, E> {
            Ok(String::new())
        }
    }
    d.deserialize_any(V)
}

/// Go `openrtb_ext.ExtImpDianomi`.
#[derive(Debug, Default, Deserialize)]
struct ExtImpDianomi {
    #[serde(rename = "smartadId", default, deserialize_with = "json_number")]
    smartad_id: String,
    #[serde(rename = "priceType", default)]
    price_type: String,
}

#[derive(Debug, Default, Deserialize)]
struct ExtBidPrebid {
    #[serde(default)]
    r#type: String,
}

/// Go `openrtb_ext.ExtBid` (only `prebid.type` is read).
#[derive(Debug, Default, Deserialize)]
struct ExtBid {
    #[serde(default)]
    prebid: Option<ExtBidPrebid>,
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
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

/// Go re-marshals `request.ext` through `dianomiRequestExt{ExtRequest, PriceType `pt`}`, which
/// writes `prebid` (an empty object when absent), `schain` when present and `pt`. The typed
/// Go `ExtRequestPrebid` is not ported, so `prebid` is carried over untouched.
fn with_price_type(ext: Option<&Ext>, price_type: &str) -> Result<Ext, BidderError> {
    if let Some(ext) = ext {
        // Validates the shape the way `jsonutil.Unmarshal` does (object or null only).
        if !(ext.0.is_object() || ext.0.is_null()) {
            return Err(jsonutil::unmarshal::<ExtBid>(ext.to_json().as_bytes())
                .err()
                .unwrap_or_else(|| BidderError::FailedToUnmarshal("invalid request.ext".into())));
        }
    }
    let mut out = String::from("{\"prebid\":");
    let prebid = ext.and_then(|e| e.0.get("prebid")).filter(|v| !v.is_null());
    match prebid {
        Some(p) => out.push_str(&p.to_string()),
        None => out.push_str("{}"),
    }
    if let Some(s) = ext.and_then(|e| e.0.get("schain")).filter(|v| !v.is_null()) {
        out.push_str(",\"schain\":");
        out.push_str(&s.to_string());
    }
    out.push_str(",\"pt\":");
    out.push_str(&sonic_rs::to_string(price_type).map_err(|e| BidderError::other(e.to_string()))?);
    out.push('}');
    Ext::from_slice(out.as_bytes()).map_err(|e| BidderError::other(e.to_string()))
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors = Vec::new();
        let mut valid_imps = Vec::with_capacity(request.imp.len());
        let mut price_type = String::new();
        for imp in &request.imp {
            let bidder_ext: ExtImpBidder = match decode_ext(imp.ext.as_ref()) {
                Ok(v) => v,
                Err(e) => {
                    errors.push(BidderError::bad_input(e.to_string()));
                    continue;
                }
            };
            let dianomi_ext: ExtImpDianomi = match decode_ext(bidder_ext.bidder.as_ref()) {
                Ok(v) => v,
                Err(e) => {
                    errors.push(BidderError::bad_input(e.to_string()));
                    continue;
                }
            };
            let mut imp = imp.clone();
            imp.tagid = dianomi_ext.smartad_id;
            valid_imps.push(imp);
            // If imps specify priceType they should all be the same. If they differ, only the first one will be used
            if !dianomi_ext.price_type.is_empty() && price_type.is_empty() {
                price_type = dianomi_ext.price_type;
            }
        }
        let mut request = request.clone();
        if !price_type.is_empty() {
            match with_price_type(request.ext.as_ref(), &price_type) {
                Ok(e) => request.ext = Some(e),
                Err(e) => errors.push(e),
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
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
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
        if response_data.status_code == 204 {
            return (None, vec![]);
        }
        if response_data.status_code == 400 {
            return (None, vec![BidderError::bad_input("Unexpected status code: 400. Bad request from publisher.")]);
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
        bid_response.currency = response.cur.clone();
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
    if bid.ext.is_some() {
        if let Ok(ext) = decode_ext::<ExtBid>(bid.ext.as_ref()) {
            if let Some(prebid) = ext.prebid {
                return BidType::parse(&prebid.r#type).map_err(BidderError::other);
            }
        }
    }
    Err(BidderError::bad_server_response(format!("Failed to parse impression \"{}\" mediatype", bid.impid)))
}
