//! Go `adapters/conversant/conversant.go`.

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::adcom1::{ApiFramework, MediaCreativeSubtype, PlacementPosition};
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use serde::Deserialize;

/// Go `openrtb_ext.ExtImpConversant`.
#[derive(Debug, Default, Deserialize)]
struct ExtImpConversant {
    #[serde(default)]
    site_id: String,
    #[serde(default)]
    secure: Option<i8>,
    #[serde(default)]
    tag_id: String,
    #[serde(default)]
    position: Option<i8>,
    #[serde(default)]
    bidfloor: f64,
    #[serde(default)]
    mimes: Option<Vec<String>>,
    #[serde(default)]
    api: Option<Vec<i8>>,
    #[serde(default)]
    protocols: Option<Vec<i8>>,
    #[serde(default)]
    maxduration: Option<i64>,
}
/// Go `jsonutil.Unmarshal(ext, &target)` on a `json.RawMessage`; the failure text is the
/// json-iterator top-level one (`expect { or n, but found X`).
fn decode_ext<T: serde::de::DeserializeOwned>(ext: Option<&crate::ortb::Ext>) -> Result<T, BidderError> {
    let Some(ext) = ext else {
        // Go: Unmarshal of an empty RawMessage fails (never happens: PBS core validates imp.ext).
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into()));
    };
    use sonic_rs::JsonValueTrait;
    // json-iterator picks the object decoder from the first byte: anything but `{` / `null`
    // fails with its top-level message, while serde would accept an array for a struct.
    if ext.0.is_object() || ext.0.is_null() {
        if let Ok(v) = ext.decode::<T>() {
            return Ok(v);
        }
    }
    jsonutil::unmarshal::<T>(ext.to_json().as_bytes())
}
/// Go `adapters.ExtImpBidder` (only `bidder` is used).
#[derive(Debug, Default, serde::Deserialize)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<crate::ortb::Ext>,
}
pub struct Adapter {
    uri: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { uri: endpoint.into() }
    }
}

fn parse_cnvr_params(
    imp: &mut Imp,
    cnvr_ext: &ExtImpConversant,
    req_info: &ExtraRequestInfo,
) -> Result<(), Vec<BidderError>> {
    imp.displaymanager = "prebid-s2s".into();
    imp.displaymanagerver = "2.0.0".into();

    if imp.bidfloor <= 0.0 && cnvr_ext.bidfloor > 0.0 {
        imp.bidfloor = cnvr_ext.bidfloor;
    }
    if !cnvr_ext.tag_id.is_empty() {
        imp.tagid = cnvr_ext.tag_id.clone();
    }
    // Take care not to override the global secure flag
    if (imp.secure.is_none() || imp.secure == Some(0)) && cnvr_ext.secure.is_some() {
        imp.secure = cnvr_ext.secure;
    }
    let position = cnvr_ext.position.map(PlacementPosition);
    if let Some(banner) = imp.banner.as_mut() {
        banner.pos = position;
    } else if let Some(video) = imp.video.as_mut() {
        video.pos = position;
        if let Some(api) = cnvr_ext.api.as_ref().filter(|a| !a.is_empty()) {
            video.api = api.iter().map(|a| ApiFramework(i64::from(*a))).collect();
        }
        // Include protocols, mimes, and max duration if specified; they override the ad unit's.
        if let Some(p) = cnvr_ext.protocols.as_ref().filter(|p| !p.is_empty()) {
            video.protocols = p.iter().map(|x| MediaCreativeSubtype(*x)).collect();
        }
        if let Some(m) = cnvr_ext.mimes.as_ref().filter(|m| !m.is_empty()) {
            video.mimes = Some(m.clone());
        }
        if let Some(d) = cnvr_ext.maxduration {
            video.maxduration = d;
        }
    }
    if imp.bidfloor > 0.0 && !imp.bidfloorcur.is_empty() && imp.bidfloorcur.to_uppercase() != "USD" {
        match req_info.convert_currency(imp.bidfloor, &imp.bidfloorcur, "USD") {
            Ok(floor) => {
                imp.bidfloorcur = "USD".into();
                imp.bidfloor = floor;
            }
            Err(_) => {
                return Err(vec![BidderError::bad_input(format!(
                    "Unable to convert provided bid floor currency from {} to USD",
                    imp.bidfloorcur
                ))]);
            }
        }
    }
    Ok(())
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut request = request.clone();
        // Backend needs USD or it will reject the request
        if !request.cur.is_empty() && request.cur[0] != "USD" {
            request.cur = vec!["USD".into()];
        }
        for i in 0..request.imp.len() {
            let bidder_ext: ExtImpBidder = match decode_ext(request.imp[i].ext.as_ref()) {
                Ok(v) => v,
                Err(_) => {
                    return (vec![], vec![BidderError::bad_input(format!("Impression[{i}] missing ext object"))]);
                }
            };
            let cnvr_ext: ExtImpConversant = match decode_ext(bidder_ext.bidder.as_ref()) {
                Ok(v) => v,
                Err(_) => {
                    return (vec![], vec![BidderError::bad_input(format!("Impression[{i}] missing ext.bidder object"))]);
                }
            };
            if cnvr_ext.site_id.is_empty() {
                return (vec![], vec![BidderError::bad_input(format!("Impression[{i}] requires ext.bidder.site_id"))]);
            }
            if i == 0 {
                if let Some(site) = request.site.as_mut() {
                    site.id = cnvr_ext.site_id.clone();
                } else if let Some(app) = request.app.as_mut() {
                    app.id = cnvr_ext.site_id.clone();
                }
            }
            if let Err(errs) = parse_cnvr_params(&mut request.imp[i], &cnvr_ext, req_info) {
                return (vec![], errs);
            }
        }
        let data = match crate::go_json::to_vec(&request) {
            Ok(d) => d,
            Err(_) => return (vec![], vec![BidderError::bad_input("Error in packaging request to JSON")]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.uri.clone(),
                body: data,
                headers,
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        _external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 204 {
            return (None, vec![]);
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!("Unexpected status code: {}", response.status_code))],
            );
        }
        let resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => {
                // Go formats the error with `%d`, which prints the struct as `&{%!d(string=msg)}`.
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "bad server response: &{{%!d(string={})}}. ",
                        e.message()
                    ))],
                );
            }
        };
        let Some(first) = resp.seatbid.into_iter().next() else {
            return (None, vec![BidderError::bad_server_response("Empty bid request")]);
        };
        let mut bid_response = BidderResponse::with_bids_capacity(first.bid.len());
        for bid in first.bid {
            let t = get_bid_type(&bid.impid, &internal_request.imp);
            bid_response.bids.push(TypedBid::new(bid, t));
        }
        (Some(bid_response), vec![])
    }
}

fn get_bid_type(imp_id: &str, imps: &[Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.native.is_some() {
                return BidType::Native;
            } else if imp.audio.is_some() {
                return BidType::Audio;
            } else if imp.video.is_some() {
                return BidType::Video;
            }
            break;
        }
    }
    BidType::Banner
}
