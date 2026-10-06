//! Go `adapters/kidoz/kidoz.go`.

use serde::Deserialize;
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
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
struct ExtImpKidoz {
    access_token: String,
    publisher_id: String,
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

/// Go `GetMediaTypeForImp`; `None` is `UndefinedMediaType`.
fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> Option<BidType> {
    let mut bid_type = None;
    for imp in imps {
        if imp.id != imp_id {
            continue;
        }
        if imp.banner.is_some() {
            bid_type = Some(BidType::Banner);
        } else if imp.video.is_some() {
            bid_type = Some(BidType::Video);
        } else if imp.native.is_some() {
            bid_type = Some(BidType::Native);
        } else if imp.audio.is_some() {
            bid_type = Some(BidType::Audio);
        }
        break;
    }
    bid_type
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        headers.add("x-openrtb-version", "2.5");

        let mut result = Vec::with_capacity(request.imp.len());
        let mut errs = Vec::new();
        for impression in &request.imp {
            if impression.banner.is_none() && impression.video.is_none() {
                errs.push(BidderError::bad_input("Kidoz only supports banner or video ads"));
                continue;
            }
            if let Some(banner) = &impression.banner {
                // Go tells a nil `format` ("banner format required") from an empty one.
                if banner.format.is_nil() {
                    errs.push(BidderError::bad_input("banner format required"));
                    continue;
                }
                if banner.format.is_empty() {
                    errs.push(BidderError::bad_input("banner format array is empty"));
                    continue;
                }
            }
            let Some(imp_ext) = &impression.ext else {
                errs.push(BidderError::other("impression extensions required"));
                continue;
            };
            let bidder_ext: ExtImpBidder = match unmarshal_ext(Some(imp_ext)) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            let Some(bidder) = bidder_ext.bidder else {
                errs.push(BidderError::other("bidder required"));
                continue;
            };
            let impression_ext: ExtImpKidoz = match unmarshal_ext(Some(&bidder)) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            if impression_ext.access_token.is_empty() {
                errs.push(BidderError::other("Kidoz access_token required"));
                continue;
            }
            if impression_ext.publisher_id.is_empty() {
                errs.push(BidderError::other("Kidoz publisher_id required"));
                continue;
            }
            let mut copy = request.clone();
            copy.imp = vec![impression.clone()];
            let body = match crate::go_json::to_vec(&copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::other(e.to_string()));
                    continue;
                }
            };
            result.push(RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers: headers.clone(),
                imp_ids: vec![impression.id.clone()],
            });
        }
        (result, errs)
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        let body_text = || String::from_utf8_lossy(&response_data.body).into_owned();
        match response_data.status_code {
            204 | 503 => return (None, vec![]),
            400 | 401 | 403 => {
                return (
                    None,
                    vec![BidderError::bad_input(format!(
                        "unexpected status code: {} {}",
                        response_data.status_code,
                        body_text()
                    ))],
                )
            }
            200 => {}
            code => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "unexpected status code: {code} {}",
                        body_text()
                    ))],
                )
            }
        }
        let bid_response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![BidderError::bad_server_response(e.to_string())]),
        };
        let mut out = BidderResponse::with_bids_capacity(request.imp.len());
        let mut errs = Vec::new();
        for sb in bid_response.seatbid {
            for bid in sb.bid {
                match get_media_type_for_imp(&bid.impid, &request.imp) {
                    None => errs.push(BidderError::bad_server_response(format!(
                        "ignoring bid id={}, request doesn't contain any valid impression with id={}",
                        bid.id, bid.impid
                    ))),
                    Some(t) => out.bids.push(TypedBid::new(bid, t)),
                }
            }
        }
        (Some(out), errs)
    }
}
