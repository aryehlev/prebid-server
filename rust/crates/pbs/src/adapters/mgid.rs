//! Go `adapters/mgid/mgid.go`.

use serde::Deserialize;
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse};
use crate::ortb::Ext;

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
struct ExtImpMgid {
    #[serde(rename = "accountId")]
    account_id: String,
    #[serde(rename = "placementId")]
    placement_id: String,
    cur: String,
    currency: String,
    #[serde(rename = "bidfloor")]
    bid_floor: f64,
    #[serde(rename = "bidFloor")]
    bid_floor2: f64,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RespBidExt {
    crtype: String,
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

/// Go `preprocess`: mutates the request (`request` here is the caller's clone) and returns the
/// URL path (the account id).
fn preprocess(request: &mut BidRequest) -> Result<String, BidderError> {
    let mut path = String::new();
    if request.tmax == 0 {
        request.tmax = 200;
    }
    for imp in &mut request.imp {
        let bidder_ext: ExtImpBidder =
            unmarshal_ext(imp.ext.as_ref()).map_err(|e| BidderError::bad_input(e.to_string()))?;
        let mgid_ext: ExtImpMgid = unmarshal_ext(bidder_ext.bidder.as_ref())
            .map_err(|e| BidderError::bad_input(e.to_string()))?;
        if path.is_empty() {
            path = mgid_ext.account_id.clone();
        }
        if mgid_ext.placement_id.is_empty() {
            imp.tagid = imp.id.clone();
        } else {
            imp.tagid = format!("{}/{}", mgid_ext.placement_id, imp.id);
        }
        let mut cur = "";
        if !mgid_ext.currency.is_empty() && mgid_ext.currency != "USD" {
            cur = &mgid_ext.currency;
        }
        if cur.is_empty() && !mgid_ext.cur.is_empty() && mgid_ext.cur != "USD" {
            cur = &mgid_ext.cur;
        }
        let mut bidfloor = mgid_ext.bid_floor;
        if bidfloor <= 0.0 {
            bidfloor = mgid_ext.bid_floor2;
        }
        if bidfloor > 0.0 {
            imp.bidfloor = bidfloor;
        }
        if !cur.is_empty() {
            imp.bidfloorcur = cur.to_string();
        }
    }
    if path.is_empty() {
        return Err(BidderError::bad_input("accountId is not set"));
    }
    Ok(path)
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req = request.clone();
        let path = match preprocess(&mut req) {
            Ok(p) => p,
            Err(e) => return (vec![], vec![e]),
        };
        let body = match crate::go_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: format!("{}{}", self.endpoint, path),
                body,
                headers,
                imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        _unused: &RequestData,
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
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {}. Run with request.debug = 1 for more info",
                    response.status_code
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut out = BidderResponse::with_bids_capacity(1);
        out.currency = bid_resp.cur.clone();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mut bid_type = BidType::Banner;
                if let Some(ext) = &bid.ext {
                    if ext.to_json().contains("crtype") {
                        // Go takes any non-empty `crtype` string as the BidType; a value that is
                        // not one of the four known types cannot be represented, so it stays banner.
                        if let Ok(e) = ext.decode::<RespBidExt>() {
                            if let Ok(t) = BidType::parse(&e.crtype) {
                                bid_type = t;
                            }
                        }
                    }
                }
                out.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        (Some(out), vec![])
    }
}
