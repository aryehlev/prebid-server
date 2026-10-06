//! Go `adapters/mobilefuse/mobilefuse.go`.

use serde::Deserialize;
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{
    is_response_status_code_no_content, Bidder, BidderResponse, ExtraRequestInfo, RequestData,
    ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

/// Go `openrtb_ext.ExtImpMobileFuse`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtImpMobileFuse {
    #[serde(deserialize_with = "crate::ortb::de::int")]
    placement_id: i64,
}

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

/// Go: `jsonutil.Unmarshal(imp.Ext, &ExtImpBidder)` then `jsonutil.Unmarshal(bidderExt.Bidder, &T)`.
/// Returns the Go error message text of whichever step fails.
fn imp_bidder_params<T: serde::de::DeserializeOwned>(ext: Option<&Ext>) -> Result<T, String> {
    fn not_obj(v: &sonic_rs::Value) -> Option<String> {
        if v.is_object() || v.is_null() {
            return None;
        }
        let first = sonic_rs::to_string(v).ok()?.chars().next().unwrap_or('\u{0}');
        Some(format!("expect {{ or n, but found {first}"))
    }
    let Some(ext) = ext else {
        return Err("expect { or n, but found \u{0}".to_string());
    };
    if let Some(m) = not_obj(&ext.0) {
        return Err(m);
    }
    match ext.0.get("bidder") {
        None => Err("expect { or n, but found \u{0}".to_string()),
        Some(b) => {
            if let Some(m) = not_obj(b) {
                return Err(m);
            }
            sonic_rs::from_value::<T>(b).map_err(|e| e.to_string())
        }
    }
}

/// Go `jsonutil.Unmarshal(imp.Ext, &ExtImpBidder)` + `Unmarshal(bidder, &ext)`, merged into `ext`
/// (Go unmarshals on top of the existing value, so an absent `placement_id` keeps the old one).
fn merge_ext_for_imp(imp: &Imp, ext: &mut ExtImpMobileFuse) -> Result<(), BidderError> {
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Probe {
        #[serde(deserialize_with = "crate::ortb::de::opt_int")]
        placement_id: Option<i64>,
    }
    let p: Probe = imp_bidder_params(imp.ext.as_ref()).map_err(BidderError::FailedToUnmarshal)?;
    if let Some(id) = p.placement_id {
        ext.placement_id = id;
    }
    Ok(())
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        match self.make_request(request) {
            Ok(r) => (vec![r], vec![]),
            Err(errs) => (vec![], errs),
        }
    }

    fn make_bids(
        &self,
        _incoming: &BidRequest,
        _outgoing: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response) {
            return (None, vec![]);
        }
        if response.status_code == 400 {
            return (
                None,
                vec![BidderError::bad_input(format!("Unexpected status code: {}.", response.status_code))],
            );
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {}.",
                    response.status_code
                ))],
            );
        }
        let incoming: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut out = BidderResponse::with_bids_capacity(1);
        for seatbid in incoming.seatbid {
            for mut bid in seatbid.bid {
                let t = get_bid_type(&bid);
                bid.ext = None;
                out.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(out), vec![])
    }
}

impl Adapter {
    fn make_request(&self, bid_request: &BidRequest) -> Result<RequestData, Vec<BidderError>> {
        let (mobile_fuse_ext, errs) = get_first_mobilefuse_extension(bid_request);
        if !errs.is_empty() {
            return Err(errs);
        }
        let valid_imps = get_valid_imps(bid_request, mobile_fuse_ext).map_err(|e| vec![e])?;

        let mut req = bid_request.clone();
        req.imp = valid_imps;
        let body = crate::go_json::to_vec(&req).map_err(|e| vec![BidderError::other(e.to_string())])?;

        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        Ok(RequestData {
            method: "POST".into(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
        })
    }
}

/// Go `getFirstMobileFuseExtension`: stops at the first imp that parses; Go returns the errors of
/// the imps before it, and `makeRequest` treats any error as fatal.
fn get_first_mobilefuse_extension(request: &BidRequest) -> (ExtImpMobileFuse, Vec<BidderError>) {
    let mut ext = ExtImpMobileFuse::default();
    let mut errs = Vec::new();
    for imp in &request.imp {
        match merge_ext_for_imp(imp, &mut ext) {
            Ok(()) => break,
            Err(e) => errs.push(e),
        }
    }
    (ext, errs)
}

fn get_valid_imps(bid_request: &BidRequest, mut ext: ExtImpMobileFuse) -> Result<Vec<Imp>, BidderError> {
    let mut valid = Vec::new();
    for imp in &bid_request.imp {
        if imp.banner.is_some() || imp.video.is_some() || imp.native.is_some() {
            let mut imp = imp.clone();
            merge_ext_for_imp(&imp, &mut ext)?;
            imp.tagid = ext.placement_id.to_string();

            // Go: `Unmarshal(imp.Ext, &ExtSkadn)`; `skadn` is kept when present (non-nil RawMessage).
            let skadn = match imp.ext.as_ref() {
                None => return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into())),
                Some(e) if e.0.is_object() => e.0.get("skadn").map(|v| v.to_string()),
                Some(e) if e.0.is_null() => None,
                Some(e) => {
                    return Err(BidderError::FailedToUnmarshal(format!(
                        "expect {{ or n, but found {}",
                        e.to_json().chars().next().unwrap_or('\u{0}')
                    )))
                }
            };
            imp.ext = match skadn {
                Some(s) => Some(
                    Ext::from_slice(format!("{{\"skadn\":{s}}}").as_bytes())
                        .map_err(|e| BidderError::other(e.to_string()))?,
                ),
                None => None,
            };
            valid.push(imp);
        }
    }
    if valid.is_empty() {
        return Err(BidderError::other("No valid imps"));
    }
    Ok(valid)
}

fn get_bid_type(bid: &Bid) -> BidType {
    if let Some(ext) = &bid.ext {
        if let Some(mt) = ext.0.get("mf").and_then(|m| m.get("media_type")).and_then(|t| t.as_str()) {
            if mt == "video" {
                return BidType::Video;
            } else if mt == "native" {
                return BidType::Native;
            }
        }
    }
    BidType::Banner
}
