//! Go `adapters/unicorn/unicorn.go`.

use serde::{Deserialize, Serialize};
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Publisher, Source};
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

/// Go `openrtb_ext.ExtImpUnicorn` (all `omitempty`).
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ExtImpUnicorn {
    #[serde(rename = "placementId", skip_serializing_if = "String::is_empty")]
    placement_id: String,
    #[serde(rename = "publisherId", skip_serializing_if = "String::is_empty")]
    publisher_id: String,
    #[serde(rename = "mediaId", skip_serializing_if = "String::is_empty")]
    media_id: String,
    #[serde(rename = "accountId", skip_serializing_if = "is_zero")]
    account_id: i64,
}

fn is_zero(v: &i64) -> bool {
    *v == 0
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct UnicornImpExt {
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<UnicornImpExtContext>,
    bidder: ExtImpUnicorn,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct UnicornImpExtContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

/// Go `unicornExt`: `prebid` is an `ExtImpPrebid` pointer; only the request-level fields the
/// request carries matter, so it is kept as raw JSON with its key order.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct UnicornExt {
    #[serde(skip_serializing_if = "Option::is_none")]
    prebid: Option<ExtImpPrebid>,
    #[serde(rename = "accountId", skip_serializing_if = "is_zero")]
    account_id: i64,
}

/// Go `openrtb_ext.ExtImpPrebid`: decoding into it drops every other key (`targeting`, `cache`, ...).
/// Nested values are kept as written (Go would also filter inside `storedrequest` etc.).
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ExtImpPrebid {
    #[serde(skip_serializing_if = "Option::is_none")]
    storedrequest: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    storedauctionresponse: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    storedbidresponse: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_rewarded_inventory: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bidder: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    adunitcode: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    passthrough: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    floors: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    imp: Option<serde_json::Value>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtRegs {
    gdpr: Option<i8>,
    us_privacy: String,
}

/// Go `jsonparser.GetString(ext, path...)`: `Err(msg)` carries jsonparser's message.
fn json_get<'a>(ext: Option<&'a Ext>, path: &[&str]) -> Result<&'a sonic_rs::Value, &'static str> {
    let Some(ext) = ext else { return Err("Key path not found") };
    let mut v = &ext.0;
    for key in path {
        v = v.get(*key).ok_or("Key path not found")?;
    }
    Ok(v)
}

fn json_get_string(ext: Option<&Ext>, path: &[&str]) -> Result<String, &'static str> {
    let v = json_get(ext, path)?;
    match v.as_str() {
        Some(s) => Ok(s.to_string()),
        None => Err("Value is not a string"),
    }
}

fn get_headers(request: &BidRequest) -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");
    headers.add("X-Openrtb-Version", "2.5");
    if let Some(device) = &request.device {
        if !device.ua.is_empty() {
            headers.add("User-Agent", device.ua.clone());
        }
        if !device.ipv6.is_empty() {
            headers.add("X-Forwarded-For", device.ipv6.clone());
        }
        if !device.ip.is_empty() {
            headers.add("X-Forwarded-For", device.ip.clone());
        }
    }
    headers
}

fn get_stored_request_imp_id(ext: Option<&Ext>) -> Result<String, BidderError> {
    json_get_string(ext, &["prebid", "storedrequest", "id"])
        .map_err(|e| BidderError::other(format!("stored request id not found: {e}")))
}

fn modify_imps(request: &mut BidRequest) -> Result<(), BidderError> {
    for (i, imp) in request.imp.iter_mut().enumerate() {
        let mut ext: UnicornImpExt = match &imp.ext {
            None => {
                return Err(BidderError::bad_input(format!(
                    "Error while decoding imp[{i}].ext: expect {{ or n, but found \u{0}"
                )))
            }
            Some(e) if e.0.is_null() => UnicornImpExt::default(),
            Some(e) if !e.0.is_object() => {
                let first = e.to_json().chars().next().unwrap_or('\0');
                return Err(BidderError::bad_input(format!(
                    "Error while decoding imp[{i}].ext: expect {{ or n, but found {first}"
                )));
            }
            Some(e) => {
                // accountId is an int in Go; a non-integer value gives jsoniter's type error.
                if let Some(acc) = e.0.get("bidder").and_then(|b| b.get("accountId")) {
                    if !acc.is_null() && !acc.is_i64() && !acc.is_u64() {
                        let text = acc.to_string();
                        let first = text.chars().next().unwrap_or('\0');
                        return Err(BidderError::bad_input(format!(
                            "Error while decoding imp[{i}].ext: cannot unmarshal openrtb_ext.ExtImpUnicorn.AccountID: unexpected character: {first}"
                        )));
                    }
                }
                e.decode().map_err(|err| {
                    BidderError::bad_input(format!("Error while decoding imp[{i}].ext: {err}"))
                })?
            }
        };
        if ext.bidder.placement_id.is_empty() {
            ext.bidder.placement_id = get_stored_request_imp_id(imp.ext.as_ref()).map_err(|e| {
                BidderError::bad_input(format!("Error get StoredRequestImpID from imp[{i}]: {e}"))
            })?;
        }
        imp.ext = Some(Ext::from_serialize(&ext).map_err(|e| {
            BidderError::bad_input(format!("Error while encoding imp[{i}].ext: {e}"))
        })?);
        imp.secure = Some(1);
        imp.tagid = ext.bidder.placement_id.clone();
    }
    Ok(())
}

fn modify_app(request: &mut BidRequest) -> Result<(), BidderError> {
    let Some(app) = &request.app else {
        return Err(BidderError::other("request app is required"));
    };
    let mut app = app.clone();
    // Go indexes `Imp[0]`; `modify_imps` has already failed on an empty `imp`'s absence only when
    // there are imps, so an empty list is reported here instead of panicking.
    let Some(first) = request.imp.first() else {
        return Err(BidderError::other("request has no imp"));
    };
    if let Ok(media_id) = json_get_string(first.ext.as_ref(), &["bidder", "mediaId"]) {
        app.id = media_id;
    }
    if let Ok(publisher_id) = json_get_string(first.ext.as_ref(), &["bidder", "publisherId"]) {
        let mut publisher: Publisher = app.publisher.clone().unwrap_or_default();
        publisher.id = publisher_id;
        app.publisher = Some(publisher);
    }
    request.app = Some(app);
    Ok(())
}

fn set_ext(request: &BidRequest) -> Result<Ext, BidderError> {
    let first = request
        .imp
        .first()
        .ok_or_else(|| BidderError::other("accountId field is required"))?;
    // `jsonparser.GetInt`: the value must be present and an integer.
    let account_id = json_get(first.ext.as_ref(), &["bidder", "accountId"])
        .ok()
        .and_then(|v| v.as_i64())
        .ok_or_else(|| BidderError::other("accountId field is required"))?;
    let mut decoded: UnicornExt = match &request.ext {
        Some(e) => e.decode().unwrap_or_default(),
        None => UnicornExt::default(),
    };
    decoded.account_id = account_id;
    Ext::from_serialize(&decoded)
        .map_err(|e| BidderError::bad_input(format!("Error while encoding ext, err: {e}")))
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if let Some(regs) = &request.regs {
            if regs.coppa == 1 {
                return (vec![], vec![BidderError::bad_input("COPPA is not supported")]);
            }
            if let Some(ext) = &regs.ext {
                if let Ok(ext_regs) = ext.decode::<ExtRegs>() {
                    if ext_regs.gdpr == Some(1) {
                        return (vec![], vec![BidderError::bad_input("GDPR is not supported")]);
                    }
                    if !ext_regs.us_privacy.is_empty() {
                        return (vec![], vec![BidderError::bad_input("CCPA is not supported")]);
                    }
                }
            }
        }
        let mut req = request.clone();
        if let Err(e) = modify_imps(&mut req) {
            return (vec![], vec![e]);
        }
        if let Err(e) = modify_app(&mut req) {
            return (vec![], vec![e]);
        }
        let mut source: Source = req.source.clone().unwrap_or_default();
        source.ext = Ext::from_slice(br#"{"stype": "prebid_server_uncn", "bidder": "unicorn"}"#).ok();
        req.source = Some(source);
        match set_ext(&req) {
            Ok(e) => req.ext = Some(e),
            Err(e) => return (vec![], vec![e]),
        }
        let body = match crate::go_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers: get_headers(&req),
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
            400 => return (None, vec![BidderError::bad_input("Unexpected http status code: 400")]),
            200 => {}
            code => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!("Unexpected http status code: {code}"))],
                )
            }
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut out = BidderResponse::with_bids_capacity(request.imp.len());
        out.currency = response.cur.clone();
        for sb in response.seatbid {
            for bid in sb.bid {
                // Go's zero `BidType` is "" unless the bid's imp has a banner (`BidType::Other`).
                let mut bid_type = BidType::Other;
                for imp in &request.imp {
                    if imp.id == bid.impid && imp.banner.is_some() {
                        bid_type = BidType::Banner;
                    }
                }
                out.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        (Some(out), vec![])
    }
}
