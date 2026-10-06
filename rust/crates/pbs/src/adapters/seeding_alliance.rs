//! Go `adapters/seedingAlliance/seedingAlliance.go`.

use serde::Deserialize;
use sonic_rs::{JsonContainerTrait, JsonValueTrait};

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

pub struct Adapter {
    endpoint: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: &str) -> Result<Self, BidderError> {
        let template = EndpointTemplate::parse(endpoint)
            .map_err(|e| BidderError::other(format!("unable to parse endpoint url template: {e}")))?;
        // Go's `template.Parse` rejects an action that is not a field (`{{Malformed}}`).
        template
            .resolve(&EndpointTemplateParams::default())
            .map_err(|e| BidderError::other(format!("unable to parse endpoint url template: {e}")))?;
        Ok(Self { endpoint: template })
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

#[derive(Default)]
struct ImpExtSeedingAlliance {
    ad_unit_id: String,
    seat_id: String,
    account_id: String,
}

#[derive(Deserialize, Default)]
struct BidExtPrebid {
    #[serde(rename = "type", default)]
    r#type: String,
}

#[derive(Deserialize, Default)]
struct ExtBidIn {
    #[serde(default)]
    prebid: Option<BidExtPrebid>,
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

/// jsoniter's string-field type error: `cannot unmarshal {path}: expects " or n, but found X`.
fn string_field(obj: &Ext, key: &str, go_path: &str) -> Result<String, BidderError> {
    // Go matches keys case-insensitively when no exact key matches.
    let value = obj.0.get(key).or_else(|| {
        obj.0.as_object().and_then(|o| {
            o.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v)
        })
    });
    match value {
        None => Ok(String::new()),
        Some(v) if v.is_null() => Ok(String::new()),
        Some(v) => match v.as_str() {
            Some(s) => Ok(s.to_string()),
            None => {
                let text = v.to_string();
                let first = text.chars().next().unwrap_or('\0');
                Err(BidderError::FailedToUnmarshal(format!(
                    "cannot unmarshal {go_path}: expects \" or n, but found {first}"
                )))
            }
        },
    }
}

fn get_ext_info(imp: &mut Imp) -> Result<String, BidderError> {
    let ext: ExtImpBidder = unmarshal_ext(imp.ext.as_ref())
        .map_err(|e| BidderError::other(format!("could not unmarshal adapters.ExtImpBidder: {e}")))?;
    let wrap = |e: BidderError| {
        BidderError::other(format!("could not unmarshal openrtb_ext.ImpExtSeedingAlliance: {e}"))
    };
    let mut ext_sa = ImpExtSeedingAlliance::default();
    match &ext.bidder {
        None => return Err(wrap(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into()))),
        Some(b) if b.0.is_null() => {}
        Some(b) if !b.0.is_object() => {
            let text = b.to_json();
            let first = text.chars().next().unwrap_or('\0');
            return Err(wrap(BidderError::FailedToUnmarshal(format!("expect {{ or n, but found {first}"))));
        }
        Some(b) => {
            const P: &str = "openrtb_ext.ImpExtSeedingAlliance";
            ext_sa.ad_unit_id = string_field(b, "adUnitId", &format!("{P}.AdUnitID")).map_err(wrap)?;
            ext_sa.seat_id = string_field(b, "seatId", &format!("{P}.SeatID")).map_err(wrap)?;
            ext_sa.account_id = string_field(b, "accountId", &format!("{P}.AccountID")).map_err(wrap)?;
        }
    }
    
    let mut account_id = "pbs".to_string();
    imp.tagid = ext_sa.ad_unit_id;
    if !ext_sa.seat_id.is_empty() {
        account_id = ext_sa.seat_id;
    }
    if !ext_sa.account_id.is_empty() {
        account_id = ext_sa.account_id;
    }
    Ok(account_id)
}

fn get_media_type_for_bid(ext: Option<&Ext>) -> Result<BidType, BidderError> {
    let bid_ext: ExtBidIn = unmarshal_ext(ext)
        .map_err(|e| BidderError::other(format!("could not unmarshal openrtb_ext.ExtBid: {e}")))?;
    let Some(prebid) = bid_ext.prebid else {
        return Err(BidderError::other("bid.Ext.Prebid is empty"));
    };
    BidType::parse(&prebid.r#type).map_err(BidderError::other)
}

fn resolve_price_macro(bid: &mut Bid) {
    // Go `strconv.FormatFloat(price, 'f', -1, 64)`: Rust's `Display` for f64 is the same text.
    let price = format!("{}", bid.price);
    bid.adm = bid.adm.replace("${AUCTION_PRICE}", &price);
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req = request.clone();
        let mut account_id = String::new();
        for imp in &mut req.imp {
            match get_ext_info(imp) {
                Ok(a) => account_id = a,
                Err(e) => return (vec![], vec![e]),
            }
        }
        if !req.cur.iter().any(|c| c == "EUR") {
            req.cur.push("EUR".into());
        }
        let body = match crate::go_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        let url = match self
            .endpoint
            .resolve(&EndpointTemplateParams { account_id, ..Default::default() })
        {
            Ok(u) => u,
            Err(e) => return (vec![], vec![BidderError::other(e)]),
        };
        (
            vec![RequestData {
                method: "POST".into(),
                uri: url,
                body,
                headers: Header::new(),
                imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        match response_data.status_code {
            204 => return (None, vec![]),
            400 => {
                return (
                    None,
                    vec![BidderError::bad_input(
                        "Unexpected status code: 400. Bad request from publisher. Run with request.debug = 1 for more info.",
                    )],
                )
            }
            200 => {}
            code => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "Unexpected status code: {code}. Run with request.debug = 1 for more info."
                    ))],
                )
            }
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut out = BidderResponse::with_bids_capacity(request.imp.len());
        out.currency = response.cur.clone();
        let mut errs = Vec::new();
        for sb in response.seatbid {
            for mut bid in sb.bid {
                resolve_price_macro(&mut bid);
                match get_media_type_for_bid(bid.ext.as_ref()) {
                    Ok(t) => out.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        (Some(out), errs)
    }
}
