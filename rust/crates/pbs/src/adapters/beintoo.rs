//! Go `adapters/beintoo/beintoo.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;
use sonic_rs::JsonValueTrait;

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
struct ExtImpBeintoo {
    tagid: String,
    bidfloor: String,
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

/// Lower-cases the top-level keys of an object ext (all of this struct's JSON names are lower case).
fn lowercase_keys(ext: &Ext) -> Ext {
    if !ext.0.is_object() {
        return ext.clone();
    }
    let mut map = serde_json::Map::new();
    if let Ok(v) = ext.decode::<serde_json::Map<String, serde_json::Value>>() {
        for (k, val) in v {
            map.insert(k.to_lowercase(), val);
        }
    }
    Ext::from_slice(&serde_json::to_vec(&map).unwrap_or_default()).unwrap_or_else(|_| ext.clone())
}

fn unpack_imp_ext(imp: &Imp) -> Result<ExtImpBeintoo, BidderError> {
    let bidder_ext: ExtImpBidder =
        unmarshal_ext(imp.ext.as_ref()).map_err(|e| BidderError::bad_input(e.to_string()))?;
    // Go (json-iterator) matches keys case-insensitively: `tagId` fills `tagid`.
    let bidder = bidder_ext.bidder.as_ref().map(lowercase_keys);
    let beintoo_ext: ExtImpBeintoo = unmarshal_ext(bidder.as_ref())
        .map_err(|_| BidderError::bad_input(format!("ignoring imp id={}, invalid ImpExt", imp.id)))?;
    // Go `strconv.ParseInt(tagid, 10, 64)`.
    match beintoo_ext.tagid.parse::<i64>() {
        Ok(v) if v != 0 => Ok(beintoo_ext),
        _ => Err(BidderError::bad_input(format!(
            "ignoring imp id={}, invalid tagid must be a String of numbers",
            imp.id
        ))),
    }
}

fn build_imp_banner(imp: &mut Imp) -> Result<(), BidderError> {
    imp.ext = None;
    let Some(banner) = &imp.banner else {
        return Err(BidderError::bad_input("Request needs to include a Banner object"));
    };
    let mut banner = banner.clone();
    if banner.w.is_none() && banner.h.is_none() {
        if banner.format.is_empty() {
            return Err(BidderError::bad_input("Need at least one size to build request"));
        }
        let format = banner.format.remove(0);
        banner.w = Some(format.w);
        banner.h = Some(format.h);
        imp.banner = Some(banner);
    }
    Ok(())
}

fn add_imp_props(imp: &mut Imp, secure: i8, ext: &ExtImpBeintoo) {
    imp.tagid = ext.tagid.clone();
    imp.secure = Some(secure);
    if !ext.bidfloor.is_empty() {
        let bid_floor = ext.bidfloor.parse::<f64>().unwrap_or(0.0);
        if bid_floor > 0.0 {
            imp.bidfloor = bid_floor;
        }
    }
}

fn add_header_if_non_empty(headers: &mut Header, name: &str, value: &str) {
    if !value.is_empty() {
        headers.add(name, value);
    }
}

/// Go `preprocess`: errors collected so far (at most one, it returns on the first).
fn preprocess(request: &mut BidRequest) -> Vec<BidderError> {
    let mut secure = 0i8;
    if let Some(site) = &request.site {
        if !site.page.is_empty() {
            if let Ok(u) = url::Url::parse(&site.page) {
                if u.scheme() == "https" {
                    secure = 1;
                }
            }
        }
    }
    let mut res_imps = Vec::with_capacity(request.imp.len());
    for imp in &request.imp {
        let mut imp = imp.clone();
        let ext = match unpack_imp_ext(&imp) {
            Ok(e) => e,
            Err(e) => return vec![e],
        };
        add_imp_props(&mut imp, secure, &ext);
        if let Err(e) = build_imp_banner(&mut imp) {
            return vec![e];
        }
        res_imps.push(imp);
    }
    request.imp = res_imps;
    vec![]
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::bad_input("No Imps in Bid Request")]);
        }
        let mut req = request.clone();
        let mut errors = preprocess(&mut req);
        if !errors.is_empty() {
            // Go formats the error slice with `%s`: `[msg1 msg2]`.
            let joined = errors.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
            errors.push(BidderError::bad_input(format!("Error in preprocess of Imp, err: [{joined}]")));
            return (vec![], errors);
        }
        let data = match crate::go_json::to_vec(&req) {
            Ok(d) => d,
            Err(_) => return (vec![], vec![BidderError::bad_input("Error in packaging request to JSON")]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        if let Some(device) = &req.device {
            add_header_if_non_empty(&mut headers, "User-Agent", &device.ua);
            add_header_if_non_empty(&mut headers, "X-Forwarded-For", &device.ip);
            add_header_if_non_empty(&mut headers, "Accept-Language", &device.language);
            if let Some(dnt) = device.dnt {
                add_header_if_non_empty(&mut headers, "DNT", &dnt.to_string());
            }
        }
        if let Some(site) = &req.site {
            add_header_if_non_empty(&mut headers, "Referer", &site.page);
        }
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body: data,
                headers,
                imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        _internal_request: &BidRequest,
        _external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 204 {
            return (None, vec![]);
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Invalid Status Returned: {}. Run with request.debug = 1 for more info",
                    response.status_code
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "Unable to unpackage bid response. Error: {e}"
                    ))],
                )
            }
        };
        let mut out = BidderResponse::with_bids_capacity(1);
        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                bid.impid = bid.id.clone();
                out.bids.push(TypedBid::new(bid, BidType::Banner));
            }
        }
        (Some(out), vec![])
    }
}
