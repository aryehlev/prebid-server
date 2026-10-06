//! Go `adapters/yahooAds/yahooAds.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Banner, BidRequest, BidResponse, Imp};
use crate::ortb::Ext;
use sonic_rs::JsonValueTrait as _;

pub struct Adapter {
    uri: String,
}

/// Go `openrtb_ext.ExtImpYahooAds`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpYahooAds {
    dcn: String,
    pos: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Result<Self, BidderError> {
        Ok(Self { uri: endpoint.into() })
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors = Vec::new();
        let mut reqs = Vec::with_capacity(request.imp.len());
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        headers.add("x-openrtb-version", "2.5");
        if let Some(device) = &request.device {
            if !device.ua.is_empty() {
                headers.set("User-Agent", device.ua.clone());
            }
        }

        for (idx, imp) in request.imp.iter().enumerate() {
            let bidder_ext: ExtImpBidder = match unmarshal_ext(imp.ext.as_ref()) {
                Ok(v) => v,
                Err(_) => {
                    errors.push(BidderError::bad_input(format!("imp #{idx}: ext.bidder not provided")));
                    continue;
                }
            };
            let yahoo: ExtImpYahooAds = match unmarshal_ext(bidder_ext.bidder.as_ref()) {
                Ok(v) => v,
                Err(e) => {
                    errors.push(BidderError::bad_input(format!("imp #{idx}: {e}")));
                    continue;
                }
            };

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];
            if let Err(e) = change_request_for_bid_service(&mut req_copy, &yahoo) {
                errors.push(e);
                continue;
            }
            match crate::go_json::to_vec(&req_copy) {
                Ok(body) => reqs.push(RequestData {
                    method: "POST".into(),
                    uri: self.uri.clone(),
                    body,
                    headers: headers.clone(),
                    imp_ids: req_copy.imp.iter().map(|i| i.id.clone()).collect(),
                }),
                Err(e) => errors.push(BidderError::other(e.to_string())),
            }
        }
        (reqs, errors)
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 204 {
            return (None, vec![]);
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!("Unexpected status code: {}.", response.status_code))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            // Go formats the error pointer with `%d`: `&{%!d(string=MSG)}`.
            Err(e) => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "Bad server response: &{{%!d(string={})}}.",
                        e.message()
                    ))],
                )
            }
        };
        let mut bid_response = BidderResponse::with_bids_capacity(internal_request.imp.len());
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let (exists, media_type) = get_imp_info(&bid.impid, &internal_request.imp);
                if !exists {
                    return (
                        None,
                        vec![BidderError::bad_server_response(format!("Unknown ad unit code '{}'", bid.impid))],
                    );
                }
                match media_type {
                    Some(t @ (BidType::Banner | BidType::Video)) => bid_response.bids.push(TypedBid::new(bid, t)),
                    _ => continue,
                }
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_imp_info(imp_id: &str, imps: &[Imp]) -> (bool, Option<BidType>) {
    for imp in imps {
        if imp.id == imp_id {
            let t = if imp.banner.is_some() {
                Some(BidType::Banner)
            } else if imp.video.is_some() {
                Some(BidType::Video)
            } else {
                None
            };
            return (true, t);
        }
    }
    (false, None)
}

fn change_request_for_bid_service(request: &mut BidRequest, extension: &ExtImpYahooAds) -> Result<(), BidderError> {
    request.imp[0].tagid = extension.pos.clone();
    if let Some(site) = request.site.as_mut() {
        site.id = extension.dcn.clone();
    } else if let Some(app) = request.app.as_mut() {
        app.id = extension.dcn.clone();
    }

    if let Some(banner) = request.imp[0].banner.as_mut() {
        validate_banner(banner)?;
    }

    if let Some(regs) = request.regs.as_mut() {
        if !regs.gpp.is_empty() {
            let mut regs_ext: serde_json::Map<String, serde_json::Value> = match regs.ext.as_ref() {
                None => serde_json::Map::new(),
                Some(e) => {
                    if e.0.is_null() {
                        serde_json::Map::new()
                    } else {
                        serde_json::from_str(&e.to_json()).map_err(|_| unmarshal_ext::<serde_json::Map<String, serde_json::Value>>(Some(e)).unwrap_err())?
                    }
                }
            };
            regs_ext.insert("gpp".into(), serde_json::Value::String(regs.gpp.clone()));
            if !regs.gpp_sid.is_empty() {
                regs_ext.insert("gpp_sid".into(), serde_json::json!(regs.gpp_sid));
            }
            let bytes = crate::go_json::to_vec(&regs_ext).map_err(|e| BidderError::other(e.to_string()))?;
            regs.ext = Some(Ext::from_slice(&bytes).map_err(|e| BidderError::other(e.to_string()))?);
            regs.gpp = String::new();
            regs.gpp_sid = Vec::new();
        }
    }
    Ok(())
}

fn validate_banner(banner: &mut Banner) -> Result<(), BidderError> {
    if let (Some(w), Some(h)) = (banner.w, banner.h) {
        if w == 0 || h == 0 {
            return Err(BidderError::other(format!("Invalid sizes provided for Banner {w}x{h}")));
        }
        return Ok(());
    }
    if banner.format.is_empty() {
        return Err(BidderError::other("No sizes provided for Banner []"));
    }
    banner.w = Some(banner.format[0].w);
    banner.h = Some(banner.format[0].h);
    Ok(())
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
