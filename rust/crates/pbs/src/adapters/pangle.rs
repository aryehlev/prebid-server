//! Go `adapters/pangle/pangle.go`.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

pub struct Adapter {
    endpoint: String,
}

/// Go `adapters.ExtImpBidder` (prebid kept as raw JSON, re-written as is).
#[derive(Deserialize, Default)]
#[serde(default)]
struct WrappedExtImpBidder {
    prebid: Option<Value>,
    bidder: Option<Ext>,
    ae: i64,
}

/// Go `openrtb_ext.ImpExtPangle`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct ImpExtPangle {
    token: String,
    appid: String,
    placementid: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct PangleBidExt {
    pangle: Option<BidExt>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidExt {
    adtype: Option<i64>,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Result<Self, BidderError> {
        Ok(Self { endpoint: endpoint.into() })
    }
}

fn get_ad_type(imp: &Imp, ext: &WrappedExtImpBidder) -> i32 {
    if imp.video.is_some() {
        let rewarded = ext
            .prebid
            .as_ref()
            .and_then(|p| p.get("is_rewarded_inventory"))
            .and_then(|v| v.as_i64())
            == Some(1);
        if rewarded {
            return 7;
        }
        if imp.instl == 1 {
            return 8;
        }
    }
    if imp.banner.is_some() {
        return if imp.instl == 1 { 2 } else { 1 };
    }
    if let Some(n) = &imp.native {
        if !n.request.is_empty() {
            return 5;
        }
    }
    -1
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        let mut request_copy = request.clone();
        for imp in &request.imp {
            let imp_ext: WrappedExtImpBidder = match unmarshal_ext(imp.ext.as_ref()) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::other(format!("failed unmarshalling imp ext (err){e}")));
                    continue;
                }
            };
            let bidder_imp_ext: ImpExtPangle = match unmarshal_ext(imp_ext.bidder.as_ref()) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::other(format!("failed unmarshalling bidder imp ext (err){e}")));
                    continue;
                }
            };
            let ad_type = get_ad_type(imp, &imp_ext);
            if ad_type == -1 {
                errs.push(BidderError::bad_input("not a supported adtype"));
                continue;
            }
            let mut network_ids = None;
            if !bidder_imp_ext.appid.is_empty() && !bidder_imp_ext.placementid.is_empty() {
                network_ids = Some(json!({"appid": bidder_imp_ext.appid, "placementid": bidder_imp_ext.placementid}));
            } else if !bidder_imp_ext.appid.is_empty() || !bidder_imp_ext.placementid.is_empty() {
                errs.push(BidderError::bad_input("only one of appid or placementid is provided"));
                continue;
            }

            // Go marshals the wrapper struct: embedded ExtImpBidder fields (prebid, bidder, ae),
            // then adtype, is_prebid, networkids.
            let bidder_value: Value = match &imp_ext.bidder {
                Some(b) => serde_json::from_str(&b.to_json()).unwrap_or(Value::Null),
                None => Value::Null,
            };
            let mut m = serde_json::Map::new();
            m.insert("prebid".into(), imp_ext.prebid.clone().unwrap_or(Value::Null));
            m.insert("bidder".into(), bidder_value);
            if imp_ext.ae != 0 {
                m.insert("ae".into(), json!(imp_ext.ae));
            }
            m.insert("adtype".into(), json!(ad_type));
            m.insert("is_prebid".into(), json!(true));
            if let Some(n) = network_ids {
                m.insert("networkids".into(), n);
            }
            let new_ext = match crate::go_json::to_vec(&m).ok().and_then(|b| Ext::from_slice(&b).ok()) {
                Some(e) => e,
                None => {
                    errs.push(BidderError::other("failed re-marshalling imp ext"));
                    continue;
                }
            };
            let mut imp = imp.clone();
            imp.ext = Some(new_ext);
            request_copy.imp = vec![imp];
            let body = match crate::go_json::to_vec(&request_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::other(e.to_string()));
                    continue;
                }
            };
            // Go builds the header map literally (`"TOKEN"` is not canonicalised).
            let headers: Header = serde_json::from_value(json!({
                "TOKEN": [bidder_imp_ext.token],
                "Content-Type": ["application/json"],
            }))
            .unwrap_or_default();
            requests.push(RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: request_copy.imp.iter().map(|i| i.id.clone()).collect(),
            });
        }
        (requests, errs)
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response_data.status_code == 204 {
            return (None, vec![]);
        }
        if response_data.status_code == 400 {
            return (
                None,
                vec![BidderError::bad_input(
                    "Unexpected status code: 400. Bad request from publisher. Run with request.debug = 1 for more info.",
                )],
            );
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
        let mut errs = Vec::new();
        let mut bid_response = BidderResponse::with_bids_capacity(1);
        bid_response.currency = response.cur.clone();
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                match get_media_type_for_bid(&bid) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        (Some(bid_response), errs)
    }
}

fn get_media_type_for_bid(bid: &Bid) -> Result<BidType, BidderError> {
    let bid_ext: PangleBidExt = unmarshal_ext(bid.ext.as_ref()).map_err(|_| BidderError::other("invalid bid ext"))?;
    let Some(ad_type) = bid_ext.pangle.and_then(|p| p.adtype) else {
        return Err(BidderError::other("missing pangleExt/adtype in bid ext"));
    };
    match ad_type {
        1 | 2 => Ok(BidType::Banner),
        5 => Ok(BidType::Native),
        7 | 8 => Ok(BidType::Video),
        _ => Err(BidderError::other("unrecognized adtype in response")),
    }
}

// ---- local helpers (shared foundation untouched) ----

/// Go `jsonutil.Unmarshal(ext, &T)`: json-iterator matches keys case-insensitively; a non-object
/// (other than null) reports `expect { or n, but found X`; absent ext is empty input.
fn unmarshal_ext<T: serde::de::DeserializeOwned + Default>(ext: Option<&Ext>) -> Result<T, BidderError> {
    use sonic_rs::JsonValueTrait;
    let Some(ext) = ext else {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into()));
    };
    if ext.0.is_null() {
        return Ok(T::default());
    }
    let text = ext.to_json();
    if !ext.0.is_object() {
        let c = text.chars().next().unwrap_or('\u{0}');
        return Err(BidderError::FailedToUnmarshal(format!("expect {{ or n, but found {c}")));
    }
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text)
        .map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))?;
    let lowered: serde_json::Map<String, serde_json::Value> =
        map.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect();
    serde_json::from_value(serde_json::Value::Object(lowered))
        .map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}
