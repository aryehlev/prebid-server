//! Go `adapters/brave/brave.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{
    Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

use crate::macros::{EndpointTemplate, EndpointTemplateParams};

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtImpBrave {
    #[serde(rename = "placementId")]
    placement_id: String,
}

pub struct Adapter {
    endpoint: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`: fails on an endpoint that is not a valid template.
    pub fn new(endpoint: impl AsRef<str>) -> Result<Self, BidderError> {
        Ok(Self { endpoint: build_template(endpoint.as_ref())? })
    }
}

// ---- local helpers (shared foundation files are off limits) ----

/// Go `jsonutil.Unmarshal(ext, &v)` on an optional raw message: a missing message is empty
/// input (`expect { or n, but found` + NUL), anything but an object or null is rejected with
/// json-iterator's top-level wording.
#[allow(dead_code)]
fn decode_ext<T: serde::de::DeserializeOwned>(ext: Option<&Ext>) -> Result<T, BidderError> {
    let Some(ext) = ext else {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into()));
    };
    {
        use sonic_rs::JsonValueTrait;
        if !ext.0.is_object() && !ext.0.is_null() {
            let found = ext.to_json().chars().next().unwrap_or('\u{0}');
            return Err(BidderError::FailedToUnmarshal(format!("expect {{ or n, but found {found}")));
        }
    }
    ext.decode::<T>().map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))
}

/// Go `adapters.ExtImpBidder` (only the part adapters read).
#[allow(dead_code)]
#[derive(Debug, Default, serde::Deserialize)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<Ext>,
}

/// `imp.ext` -> `ext.bidder` -> `T`, the usual two-step decode.
#[allow(dead_code)]
fn decode_bidder<T: serde::de::DeserializeOwned>(imp: &Imp) -> Result<T, BidderError> {
    let bidder_ext: ExtImpBidder = decode_ext(imp.ext.as_ref())?;
    decode_ext(bidder_ext.bidder.as_ref())
}

#[allow(dead_code)]
fn ext_from<T: serde::Serialize>(v: &T) -> Result<Ext, BidderError> {
    let bytes = crate::go_json::to_vec(v).map_err(|e| BidderError::other(e.to_string()))?;
    Ext::from_slice(&bytes).map_err(|e| BidderError::other(e.to_string()))
}

#[allow(dead_code)]
fn marshal<T: serde::Serialize>(v: &T) -> Result<Vec<u8>, BidderError> {
    crate::go_json::to_vec(v).map_err(|e| BidderError::other(e.to_string()))
}

#[allow(dead_code)]
fn imp_ids(imps: &[Imp]) -> Vec<String> {
    imps.iter().map(|i| i.id.clone()).collect()
}

/// Go `json.Number`: accepts a JSON number or string, keeps the text.
#[allow(dead_code)]
#[derive(Debug, Default, Clone, PartialEq)]
struct JsonNumber(String);

impl<'de> serde::Deserialize<'de> for JsonNumber {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = JsonNumber;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a number or string")
            }
            fn visit_i64<E>(self, v: i64) -> Result<JsonNumber, E> {
                Ok(JsonNumber(v.to_string()))
            }
            fn visit_u64<E>(self, v: u64) -> Result<JsonNumber, E> {
                Ok(JsonNumber(v.to_string()))
            }
            fn visit_f64<E>(self, v: f64) -> Result<JsonNumber, E> {
                Ok(JsonNumber(v.to_string()))
            }
            fn visit_str<E>(self, v: &str) -> Result<JsonNumber, E> {
                Ok(JsonNumber(v.to_string()))
            }
            fn visit_string<E>(self, v: String) -> Result<JsonNumber, E> {
                Ok(JsonNumber(v))
            }
            fn visit_unit<E>(self) -> Result<JsonNumber, E> {
                Ok(JsonNumber(String::new()))
            }
            fn visit_none<E>(self) -> Result<JsonNumber, E> {
                Ok(JsonNumber(String::new()))
            }
        }
        d.deserialize_any(V)
    }
}

#[allow(dead_code)]
impl JsonNumber {
    /// Go `Number.String`.
    fn as_str(&self) -> &str {
        &self.0
    }
    /// Go `Number.Int64` (`strconv.ParseInt(s, 10, 64)`).
    fn int64(&self) -> Result<i64, String> {
        self.0.parse::<i64>().map_err(|e| {
            use std::num::IntErrorKind::*;
            let why = match e.kind() {
                PosOverflow | NegOverflow => "value out of range",
                _ => "invalid syntax",
            };
            format!("strconv.ParseInt: parsing {:?}: {why}", self.0)
        })
    }
    /// Go `Number.Float64`.
    fn float64(&self) -> Result<f64, String> {
        self.0
            .parse::<f64>()
            .map_err(|_| format!("strconv.ParseFloat: parsing {:?}: invalid syntax", self.0))
    }
}

/// Go `template.New("endpointTemplate").Parse(endpoint)`. Go's parser rejects a call to an
/// undefined function (`{{Malformed}}`) at parse time, so a bare identifier fails here too.
#[allow(dead_code)]
fn build_template(endpoint: &str) -> Result<crate::macros::EndpointTemplate, BidderError> {
    let fail = |e: String| BidderError::other(format!("unable to parse endpoint url template: {e}"));
    let t = crate::macros::EndpointTemplate::parse(endpoint).map_err(fail)?;
    if let Err(m) = t.resolve(&crate::macros::EndpointTemplateParams::default()) {
        if !m.contains("function \".") {
            return Err(fail(m));
        }
    }
    Ok(t)
}

#[allow(dead_code)]
fn status_err(code: u16, suffix: &str) -> String {
    format!("Unexpected status code: {code}.{suffix}")
}


fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            return if imp.video.is_some() {
                BidType::Video
            } else if imp.native.is_some() {
                BidType::Native
            } else {
                BidType::Banner
            };
        }
    }
    BidType::Banner
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        // Go indexes `request.Imp[0]` and panics on an empty list; return an error instead.
        let Some(first) = request.imp.first() else {
            return (vec![], vec![BidderError::bad_input("ext.bidder not provided")]);
        };
        let not_provided = || BidderError::bad_input("ext.bidder not provided");
        let bidder_ext: ExtImpBidder = match decode_ext(first.ext.as_ref()) {
            Ok(b) => b,
            Err(_) => return (vec![], vec![not_provided()]),
        };
        let brave_ext: ExtImpBrave = match decode_ext(bidder_ext.bidder.as_ref()) {
            Ok(b) => b,
            Err(_) => return (vec![], vec![not_provided()]),
        };

        let mut request = request.clone();
        request.imp[0].ext = None;
        let body = match marshal(&request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![e]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        let uri = match self
            .endpoint
            .resolve(&EndpointTemplateParams { publisher_id: brave_ext.placement_id, ..Default::default() })
        {
            Ok(u) => u,
            Err(e) => return (vec![], vec![BidderError::other(e)]),
        };
        (
            vec![RequestData { method: "POST".into(), uri, body, headers, imp_ids: imp_ids(&request.imp) }],
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
        let bid_response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(_) => return (None, vec![BidderError::bad_server_response("Bad Server Response")]),
        };
        let Some(sb) = bid_response.seatbid.into_iter().next() else {
            return (None, vec![BidderError::bad_server_response("Empty SeatBid array")]);
        };
        let mut out = BidderResponse::with_bids_capacity(sb.bid.len());
        for bid in sb.bid {
            let t = get_media_type_for_imp(&bid.impid, &request.imp);
            out.bids.push(TypedBid::new(bid, t));
        }
        (Some(out), vec![])
    }
}
