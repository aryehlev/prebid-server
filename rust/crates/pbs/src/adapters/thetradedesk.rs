//! Go `adapters/thetradedesk/thetradedesk.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

use crate::macros::{EndpointTemplate, EndpointTemplateParams};

/// Go `openrtb_ext.ExtImpTheTradeDesk`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtImpTheTradeDesk {
    #[serde(rename = "publisherId")]
    publisher_id: String,
    #[serde(rename = "supplySourceId")]
    supply_source_id: String,
}

pub struct Adapter {
    default_endpoint: String,
    template_endpoint: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder` (`config.Adapter.Endpoint` and `ExtraAdapterInfo`).
    pub fn new(endpoint: impl AsRef<str>, extra_adapter_info: impl AsRef<str>) -> Result<Self, BidderError> {
        let extra = extra_adapter_info.as_ref();
        if !extra.is_empty() {
            // Go `regexp.Match("([a-z]+)$", ...)`: unanchored at the start, so the string only has
            // to end in a lower-case letter.
            let valid = extra.as_bytes().last().is_some_and(|b| b.is_ascii_lowercase());
            if !valid {
                return Err(BidderError::other("ExtraAdapterInfo must be a simple string provided by TheTradeDesk"));
            }
        }
        let template = build_template(endpoint.as_ref())?;
        let default_endpoint = template
            .resolve(&EndpointTemplateParams { supply_id: extra.to_string(), ..Default::default() })
            .map_err(|e| BidderError::other(format!("unable to resolve endpoint macros: {e}")))?;
        Ok(Self { default_endpoint, template_endpoint: template })
    }

    fn build_endpoint_url(&self, supply_source_id: &str) -> Result<String, BidderError> {
        if supply_source_id.is_empty() {
            if self.default_endpoint.is_empty() {
                return Err(BidderError::other("Either supplySourceId or a default endpoint must be provided"));
            }
            return Ok(self.default_endpoint.clone());
        }
        self.template_endpoint
            .resolve(&EndpointTemplateParams { supply_id: supply_source_id.to_string(), ..Default::default() })
            .map_err(|e| BidderError::other(format!("unable to resolve endpoint macros: {e}")))
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

/// jsoniter's wording for a JSON string field that holds another JSON type, as
/// `jsonutil.Unmarshal` reports it (`cannot unmarshal {struct}.{Field}: expects " or n, but found X`).
/// serde's message carries neither the struct path nor the offending byte, so the string fields
/// are checked up front; the first mismatch wins.
#[allow(dead_code)]
fn check_string_fields(
    ext: Option<&Ext>,
    go_struct: &str,
    fields: &[(&str, &str)],
) -> Result<(), BidderError> {
    use sonic_rs::JsonValueTrait;
    let Some(ext) = ext else { return Ok(()) };
    if !ext.0.is_object() {
        return Ok(());
    }
    for (key, go_field) in fields {
        if let Some(v) = ext.0.get(*key) {
            if !v.is_str() && !v.is_null() {
                let found = v.to_string().chars().next().unwrap_or('\u{0}');
                return Err(BidderError::FailedToUnmarshal(format!(
                    "cannot unmarshal {go_struct}.{go_field}: expects \" or n, but found {found}"
                )));
            }
        }
    }
    Ok(())
}


fn get_impression_ext(imp: &Imp) -> Result<ExtImpTheTradeDesk, BidderError> {
    let bidder_ext: ExtImpBidder = decode_ext(imp.ext.as_ref())?;
    check_string_fields(
        bidder_ext.bidder.as_ref(),
        "openrtb_ext.ExtImpTheTradeDesk",
        &[("publisherId", "PublisherId"), ("supplySourceId", "SupplySourceId")],
    )?;
    decode_ext(bidder_ext.bidder.as_ref())
}

fn get_extension_info(imps: &[Imp]) -> Result<(String, String), BidderError> {
    let mut publisher_id = String::new();
    let mut supply_source_id = String::new();
    for imp in imps {
        let ttd = get_impression_ext(imp)?;
        if !ttd.publisher_id.is_empty() && publisher_id.is_empty() {
            publisher_id = ttd.publisher_id;
        }
        if !ttd.supply_source_id.is_empty() && supply_source_id.is_empty() {
            supply_source_id = ttd.supply_source_id;
        }
        if !publisher_id.is_empty() && !supply_source_id.is_empty() {
            break;
        }
    }
    Ok((publisher_id, supply_source_id))
}

fn get_bid_type(mtype: i8) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        4 => Ok(BidType::Native),
        other => Err(BidderError::other(format!("unsupported mtype: {other}"))),
    }
}

/// Go `strconv.FormatFloat(price, 'f', -1, 64)`.
fn format_price(price: f64) -> String {
    format!("{price}")
}

fn resolve_auction_price_macros(bid: &mut Bid) {
    let price = format_price(bid.price);
    bid.nurl = bid.nurl.replace("${AUCTION_PRICE}", &price);
    bid.adm = bid.adm.replace("${AUCTION_PRICE}", &price);
    bid.burl = bid.burl.replace("${AUCTION_PRICE}", &price);
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let (pub_id, supply_source_id) = match get_extension_info(&request.imp) {
            Ok(v) => v,
            Err(e) => return (vec![], vec![e]),
        };

        let mut request = request.clone();
        for imp in request.imp.iter_mut() {
            if let Some(banner) = &imp.banner {
                if let Some(first) = banner.format.first() {
                    let mut banner_copy = banner.clone();
                    banner_copy.h = Some(first.h);
                    banner_copy.w = Some(first.w);
                    imp.banner = Some(banner_copy);
                }
            }
        }

        if let Some(site) = request.site.as_mut() {
            match site.publisher.as_mut() {
                Some(p) => {
                    if !pub_id.is_empty() {
                        p.id = pub_id.clone();
                    }
                }
                None => site.publisher = Some(crate::ortb::openrtb2::Publisher { id: pub_id.clone(), ..Default::default() }),
            }
        } else if let Some(app) = request.app.as_mut() {
            match app.publisher.as_mut() {
                Some(p) => {
                    if !pub_id.is_empty() {
                        p.id = pub_id.clone();
                    }
                }
                None => app.publisher = Some(crate::ortb::openrtb2::Publisher { id: pub_id.clone(), ..Default::default() }),
            }
        }

        let body = match marshal(&request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![e]),
        };
        let uri = match self.build_endpoint_url(&supply_source_id) {
            Ok(u) => u,
            Err(e) => return (vec![], vec![e]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        (
            vec![RequestData { method: "POST".into(), uri, body, headers, imp_ids: imp_ids(&request.imp) }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response_data) {
            return (Some(BidderResponse::new()), vec![]);
        }
        if let Some(err) = check_response_status_code_for_errors(response_data) {
            return (None, vec![err]);
        }
        // jsonutil::unmarshal gives serde's EOF text for an empty body; json-iterator reports
        // `expect { or n, but found ` plus the NUL byte it reads at end of input.
        if response_data.body.iter().all(|b| b" \t\r\n".contains(b)) {
            return (None, vec![BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into())]);
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut out = BidderResponse::new();
        out.currency = response.cur.clone();
        for seat_bid in response.seatbid {
            for mut bid in seat_bid.bid {
                resolve_auction_price_macros(&mut bid);
                match get_bid_type(bid.mtype.0) {
                    Ok(t) => out.bids.push(TypedBid::new(bid, t)),
                    Err(e) => return (None, vec![e]),
                }
            }
        }
        (Some(out), vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_errors() {
        assert!(Adapter::new("{{Malformed}}", "").is_err());
        assert!(Adapter::new("http://it.doesnt.matter/bid", "12365217635").is_err());
        assert!(Adapter::new("http://it.doesnt.matter/bid", "abcde").is_ok());
        assert!(Adapter::new("https://direct.adsrvr.org/bid/bidder/{{.SupplyId}}", "ttd").is_ok());
    }

    #[test]
    fn build_endpoint() {
        let a = Adapter::new("https://direct.adsrvr.org/bid/bidder/{{.SupplyId}}", "ttd").unwrap();
        assert_eq!(a.build_endpoint_url("pub_abc").unwrap(), "https://direct.adsrvr.org/bid/bidder/pub_abc");
        assert_eq!(a.build_endpoint_url("").unwrap(), "https://direct.adsrvr.org/bid/bidder/ttd");
        let b = Adapter::new("https://adsrvr.org/bid/bidder/{{.SupplyId}}", "").unwrap();
        assert!(b.build_endpoint_url("").is_ok());
    }

    #[test]
    fn price_macros() {
        let mut bid = Bid { price: 1.23, nurl: "http://n?p=${AUCTION_PRICE}".into(), ..Default::default() };
        resolve_auction_price_macros(&mut bid);
        assert_eq!(bid.nurl, "http://n?p=1.23");
    }
}
