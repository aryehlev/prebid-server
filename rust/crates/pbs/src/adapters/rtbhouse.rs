//! Go `adapters/rtbhouse/rtbhouse.go`.
#![allow(dead_code, unused_imports)]

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

use crate::ortb::openrtb2::{MarkupType, Publisher, Site};

const BIDDER_CURRENCY: &str = "USD";

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

/// Go `openrtb_ext.ExtImpRTBHouse`.
#[derive(serde::Deserialize, Default)]
struct ExtImpRtbHouse {
    #[serde(rename = "publisherId", default)]
    publisher_id: String,
    #[serde(default)]
    bidfloor: f64,
}

/// Go `publisherExt` / `publisherExtPrebid`.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct PublisherExt {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prebid: Option<PublisherExtPrebid>,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct PublisherExtPrebid {
    #[serde(rename = "publisherId", default, skip_serializing_if = "String::is_empty")]
    publisher_id: String,
}

fn get_impression_ext(imp: &Imp) -> Result<ExtImpRtbHouse, BidderError> {
    parse_imp_ext(
        imp,
        |_| BidderError::bad_input("Bidder extension not provided or can't be unmarshalled"),
        |_| BidderError::bad_input("Error while unmarshaling bidder extension"),
    )
}

/// Go `clearAuctionEnvironment`: drops `ae`, `igs` and `paapi` from `imp.ext` (the result is a
/// re-marshalled map, so keys come out sorted).
fn clear_auction_environment(imp: &Imp) -> Result<Option<Ext>, BidderError> {
    let Some(ext) = imp.ext.as_ref() else {
        return Err(BidderError::other("unexpected end of JSON input"));
    };
    let mut map: std::collections::BTreeMap<String, serde_json::Value> =
        serde_json::from_str(&ext.to_json()).map_err(|e| BidderError::other(e.to_string()))?;
    for key in ["ae", "igs", "paapi"] {
        map.remove(key);
    }
    let bytes = crate::go_json::to_vec(&map).map_err(|e| BidderError::other(e.to_string()))?;
    Ext::from_slice(&bytes).map(Some).map_err(|e| BidderError::other(e.to_string()))
}

/// Go `setPublisherID`.
fn set_publisher_id(request: &mut BidRequest, publisher_id: &str) -> Result<(), BidderError> {
    let publisher: Option<Publisher>;
    if let Some(site) = request.site.as_ref() {
        publisher = site.publisher.clone();
    } else if let Some(app) = request.app.as_ref() {
        publisher = app.publisher.clone();
    } else {
        // If neither site nor app exists, create a site object
        request.site = Some(Site::default());
        publisher = None;
    }
    let mut publisher = publisher.unwrap_or_default();

    let mut pub_ext = PublisherExt::default();
    if let Some(ext) = publisher.ext.as_ref() {
        pub_ext = jsonutil::unmarshal(ext.to_json().as_bytes())?;
    }
    pub_ext.prebid.get_or_insert_with(PublisherExtPrebid::default).publisher_id = publisher_id.to_string();
    let bytes = crate::go_json::to_vec(&pub_ext).map_err(|e| BidderError::other(e.to_string()))?;
    publisher.ext = Ext::from_slice(&bytes).ok();

    if let Some(site) = request.site.as_mut() {
        site.publisher = Some(publisher);
    } else if let Some(app) = request.app.as_mut() {
        app.publisher = Some(publisher);
    }
    Ok(())
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req_copy = request.clone();
        req_copy.imp = vec![];
        let mut publisher_id = String::new();

        for src in &request.imp {
            let mut imp = src.clone();
            let ext = match get_impression_ext(&imp) {
                Ok(e) => e,
                Err(e) => return (vec![], vec![e]),
            };
            // Extract publisherId from the first impression that has one
            if publisher_id.is_empty() && !ext.publisher_id.is_empty() {
                publisher_id = ext.publisher_id.clone();
            }

            let mut bid_floor_cur = imp.bidfloorcur.clone();
            let mut bid_floor = imp.bidfloor;
            if bid_floor_cur.is_empty() && bid_floor == 0.0 && ext.bidfloor > 0.0 {
                bid_floor = ext.bidfloor;
                bid_floor_cur = BIDDER_CURRENCY.to_string();
                if let Some(first) = req_copy.cur.first() {
                    bid_floor_cur = first.clone();
                }
            }

            // Check if imp comes with bid floor amount defined in a foreign currency
            if bid_floor > 0.0 && !bid_floor_cur.is_empty() && bid_floor_cur.to_uppercase() != BIDDER_CURRENCY {
                match req_info.convert_currency(bid_floor, &bid_floor_cur, BIDDER_CURRENCY) {
                    Ok(v) => {
                        bid_floor_cur = BIDDER_CURRENCY.to_string();
                        bid_floor = v;
                    }
                    Err(e) => return (vec![], vec![e]),
                }
            }

            if bid_floor > 0.0 && bid_floor_cur == BIDDER_CURRENCY {
                imp.bidfloorcur = bid_floor_cur;
                imp.bidfloor = bid_floor;
            }

            // remove PAAPI signals from imp.Ext
            match clear_auction_environment(&imp) {
                Ok(e) => imp.ext = e,
                Err(e) => return (vec![], vec![e]),
            }

            // Remove PMP from impression
            imp.pmp = None;

            // Set the CUR of bid to BIDDER_CURRENCY after converting all floors
            req_copy.cur = vec![BIDDER_CURRENCY.to_string()];
            req_copy.imp.push(imp);
        }

        // Set publisher ID in site/app.publisher.ext.prebid.publisherId if we found one
        if !publisher_id.is_empty() {
            if let Err(e) = set_publisher_id(&mut req_copy, &publisher_id) {
                return (vec![], vec![e]);
            }
        }

        let body = match crate::go_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: req_copy.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        match response.status_code {
            200 => {}
            204 => return (None, vec![]),
            400 => {
                return (
                    None,
                    vec![BidderError::bad_input(format!(
                        "Unexpected status code: {}. Run with request.debug = 1 for more info",
                        response.status_code
                    ))],
                )
            }
            _ => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "Unexpected status code: {}. Run with request.debug = 1 for more info",
                        response.status_code
                    ))],
                )
            }
        }
        let resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        // Go indexes `SeatBid[0]` and panics on an empty seatbid; report it instead.
        let Some(first) = resp.seatbid.first() else {
            return (None, vec![BidderError::bad_server_response("no seatbid in the response")]);
        };
        let mut bidder_response = BidderResponse::with_bids_capacity(first.bid.len());
        let mut errs = vec![];
        for seat_bid in resp.seatbid {
            for mut bid in seat_bid.bid {
                let bid_type = match bid.mtype {
                    MarkupType::BANNER => Ok(BidType::Banner),
                    MarkupType::NATIVE => Ok(BidType::Native),
                    _ => Err(BidderError::other(format!(
                        "unrecognized bid type in response from rtbhouse for bid {}",
                        bid.impid
                    ))),
                };
                resolve_macros(&mut bid);
                let bid_type = match bid_type {
                    Ok(t) => t,
                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                };
                // for native bid responses fix Adm field
                if bid_type == BidType::Native {
                    match get_native_adm(&bid.adm) {
                        Ok(adm) => bid.adm = adm,
                        Err((adm, e)) => {
                            bid.adm = adm;
                            errs.push(e);
                            continue;
                        }
                    }
                }
                bidder_response.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        bidder_response.currency = BIDDER_CURRENCY.to_string();
        (Some(bidder_response), errs)
    }
}

/// Go `getNativeAdm`: moves `adm.native` to `adm`.
fn get_native_adm(adm: &str) -> Result<String, (String, BidderError)> {
    let fail = |msg: &str| (adm.to_string(), BidderError::other(msg));
    let trimmed = adm.trim();
    // json null decodes into a nil map without error
    if trimmed == "null" {
        return Ok(adm.to_string());
    }
    let map: std::collections::HashMap<String, Box<serde_json::value::RawValue>> =
        serde_json::from_str(adm).map_err(|_| fail("unable to unmarshal native adm"))?;
    if let Some(value) = map.get("native") {
        let text = value.get();
        if !text.trim_start().starts_with('{') {
            return Err(fail("unable to get native adm"));
        }
        return Ok(text.to_string());
    }
    Ok(adm.to_string())
}

fn resolve_macros(bid: &mut Bid) {
    let price = format_price(bid.price);
    bid.nurl = bid.nurl.replace("${AUCTION_PRICE}", &price);
    bid.adm = bid.adm.replace("${AUCTION_PRICE}", &price);
}

// ── local helpers (Go `adapters.ExtImpBidder`, `openrtb_ext.ExtBid`) ─────────────────────────

/// Go `adapters.ExtImpBidder` (only `bidder` is used here).
#[derive(serde::Deserialize, Default)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<Ext>,
}

/// Go `jsonutil.Unmarshal(raw, &v)` on a `json.RawMessage` field. An absent message is empty
/// input, which json-iterator rejects with the same `expect { or n, but found` text.
fn unmarshal_ext<T: serde::de::DeserializeOwned>(ext: Option<&Ext>) -> Result<T, BidderError> {
    match ext {
        Some(e) => jsonutil::unmarshal(e.to_json().as_bytes()),
        None => Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string())),
    }
}

/// Go: unmarshal `imp.ext` into `ExtImpBidder`, then `.Bidder` into `T`; the two errors are
/// returned separately so adapters can wrap them differently.
fn parse_imp_ext<T: serde::de::DeserializeOwned>(
    imp: &Imp,
    wrap_bidder_ext: impl Fn(BidderError) -> BidderError,
    wrap_params: impl Fn(BidderError) -> BidderError,
) -> Result<T, BidderError> {
    let bidder_ext: ExtImpBidder = unmarshal_ext(imp.ext.as_ref()).map_err(wrap_bidder_ext)?;
    unmarshal_ext(bidder_ext.bidder.as_ref()).map_err(wrap_params)
}

/// Go `openrtb_ext.ExtBid` (`bid.ext.prebid.type`).
#[derive(serde::Deserialize, Default)]
struct ExtBid {
    #[serde(default)]
    prebid: Option<ExtBidPrebid>,
}

#[derive(serde::Deserialize, Default)]
struct ExtBidPrebid {
    #[serde(rename = "type", default)]
    bid_type: String,
}

/// Go `jsonutil.Unmarshal(bid.Ext, &ExtBid)`: `Err` on a malformed ext.
fn parse_bid_ext(bid: &Bid) -> Option<Result<ExtBid, BidderError>> {
    bid.ext.as_ref().map(|e| jsonutil::unmarshal(e.to_json().as_bytes()))
}

/// Go `strconv.FormatFloat(price, 'f', -1, 64)`.
fn format_price(price: f64) -> String {
    format!("{price}")
}

/// Go `template.New("endpointTemplate").Parse(endpoint)`. `{{Malformed}}` is a Go parse error
/// (unknown function), which `EndpointTemplate::parse` only reports at resolve time, so a dry
/// run with empty params catches it here.
fn parse_template(endpoint: &str) -> Result<EndpointTemplate, String> {
    let t = EndpointTemplate::parse(endpoint)?;
    t.resolve(&EndpointTemplateParams::default())?;
    Ok(t)
}

/// Go `url.QueryEscape`.
fn query_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Go `url.PathEscape`.
fn path_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'$' | b'&' | b'+'
            | b'=' | b':' | b'@' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
