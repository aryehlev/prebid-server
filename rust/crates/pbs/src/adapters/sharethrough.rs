//! Go `adapters/sharethrough/sharethrough.go`.

use std::sync::Arc;

use serde::Deserialize;
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp, Source};
use crate::ortb::Ext;

const ADAPTER_VERSION: &str = "10.0";
/// Go `version.Ver`: empty unless set by `-ldflags` at build time.
const VER: &str = "";

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpSharethrough {
    pkey: String,
    badv: Vec<String>,
    bcat: Vec<String>,
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

/// Go `jsonutil.Unmarshal(raw, &v)` on a `json.RawMessage` that may be absent or not an object.
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

fn split_impressions_by_media_type(impression: &mut Imp) -> Result<Vec<Imp>, BidderError> {
    if impression.banner.is_none() && impression.video.is_none() && impression.native.is_none() {
        return Err(BidderError::bad_input(
            "Invalid MediaType. Sharethrough only supports Banner, Video and Native.",
        ));
    }
    impression.audio = None;
    let mut impressions = Vec::with_capacity(3);
    if impression.banner.is_some() {
        let mut c = impression.clone();
        c.video = None;
        c.native = None;
        impressions.push(c);
    }
    if impression.video.is_some() {
        let mut c = impression.clone();
        c.banner = None;
        c.native = None;
        impressions.push(c);
    }
    if impression.native.is_some() {
        impression.banner = None;
        impression.video = None;
        impressions.push(impression.clone());
    }
    Ok(impressions)
}

fn get_media_type_for_bid(bid: &Bid) -> Result<BidType, BidderError> {
    if let Some(ext) = &bid.ext {
        if let Ok(bid_ext) = ext.decode::<ExtBidIn>() {
            if let Some(prebid) = bid_ext.prebid {
                return BidType::parse(&prebid.r#type).map_err(BidderError::other);
            }
        }
    }
    Err(BidderError::bad_server_response(format!(
        "Failed to parse bid mediatype for impression \"{}\"",
        bid.impid
    )))
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errors = Vec::new();

        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");

        let mut modifiable_source: Source = request.source.clone().unwrap_or_default();
        // Go unmarshals the source ext into a map and adds `str` and `version`; any failure
        // (absent, not an object) starts a fresh map. A `serde_json` map keeps keys sorted, as
        // Go's map marshalling does.
        let mut source_ext: serde_json::Map<String, serde_json::Value> = match &modifiable_source.ext {
            Some(e) if e.0.is_object() => e.decode().unwrap_or_default(),
            _ => serde_json::Map::new(),
        };
        source_ext.insert("str".into(), ADAPTER_VERSION.into());
        source_ext.insert("version".into(), VER.into());
        match Ext::from_slice(&serde_json::to_vec(&source_ext).unwrap_or_default()) {
            Ok(e) => modifiable_source.ext = Some(e),
            Err(e) => errors.push(BidderError::other(e.to_string())),
        }

        // `requestCopy` in Go: shallow copy that accumulates bcat/badv across imps.
        let mut request_copy = request.clone();
        request_copy.source = Some(modifiable_source);
        let mut badv: Vec<String> = request_copy.badv.to_vec();

        for imp in &request.imp {
            let mut imp = imp.clone();
            let imp_ext: ExtImpBidder = match unmarshal_ext(imp.ext.as_ref()) {
                Ok(e) => e,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            let params: ExtImpSharethrough = match unmarshal_ext(imp_ext.bidder.as_ref()) {
                Ok(p) => p,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };

            // Convert floor into USD.
            if imp.bidfloor > 0.0
                && !imp.bidfloorcur.is_empty()
                && !imp.bidfloorcur.eq_ignore_ascii_case("USD")
            {
                match req_info.convert_currency(imp.bidfloor, &imp.bidfloorcur, "USD") {
                    Ok(v) => {
                        imp.bidfloorcur = "USD".into();
                        imp.bidfloor = v;
                    }
                    Err(e) => return (vec![], vec![e]),
                }
            }

            // Relocate custom params.
            imp.tagid = params.pkey;
            request_copy.bcat.extend(params.bcat);
            badv.extend(params.badv);
            request_copy.badv = Arc::from(badv.clone());

            let by_media_type = match split_impressions_by_media_type(&mut imp) {
                Ok(v) => v,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            for impression in by_media_type {
                request_copy.imp = vec![impression];
                let body = match crate::go_json::to_vec(&request_copy) {
                    Ok(b) => b,
                    Err(e) => {
                        errors.push(BidderError::other(e.to_string()));
                        continue;
                    }
                };
                requests.push(RequestData {
                    method: "POST".into(),
                    uri: self.endpoint.clone(),
                    body,
                    headers: headers.clone(),
                    imp_ids: request_copy.imp.iter().map(|i| i.id.clone()).collect(),
                });
            }
        }
        (requests, errors)
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 204 {
            return (None, vec![]);
        }
        if response.status_code == 400 {
            return (
                None,
                vec![BidderError::bad_input(format!(
                    "Unexpected status code: {}. Run with request.debug = 1 for more info",
                    response.status_code
                ))],
            );
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::other(format!(
                    "unexpected status code: {}. Run with request.debug = 1 for more info",
                    response.status_code
                ))],
            );
        }
        // Go parses the request body here but never reads it, other than for the error.
        if let Err(e) = jsonutil::unmarshal::<BidRequest>(&request_data.body) {
            return (None, vec![e]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut out = BidderResponse::new();
        out.currency = "USD".into();
        let mut errors = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_bid(&bid) {
                    Ok(t) => out.bids.push(TypedBid::new(bid, t)),
                    // Go records the error and still appends the bid, with an empty `BidType`.
                    Err(e) => {
                        errors.push(e);
                        out.bids.push(TypedBid::new(bid, BidType::Other));
                    }
                }
            }
        }
        (Some(out), errors)
    }
}
