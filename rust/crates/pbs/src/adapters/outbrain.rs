//! Go `adapters/outbrain/outbrain.go`.

use std::sync::Arc;

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
use crate::ortb::native1::response::Response as NativeResponse;
use crate::ortb::native1::{EventTrackingMethod, EventType};
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp, Publisher};
use crate::ortb::Ext;

/// Go `openrtb_ext.ExtImpOutbrainPublisher`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtImpOutbrainPublisher {
    id: String,
    name: String,
    domain: String,
}

/// Go `openrtb_ext.ExtImpOutbrain`. `bcat` / `badv` stay `Option` to tell nil from empty.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtImpOutbrain {
    publisher: ExtImpOutbrainPublisher,
    tagid: String,
    bcat: Option<Vec<String>>,
    badv: Option<Vec<String>>,
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

/// Go: `Unmarshal(imp.Ext, &ExtImpBidder)` then `Unmarshal(bidderExt.Bidder, &T)`.
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

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req_copy = request.clone();
        let mut errs: Vec<BidderError> = Vec::new();
        // Go decodes every imp into the same value, so fields absent from later imps keep the
        // earlier value. Decoding on top of the previous value is emulated with the merge below.
        let mut outbrain_ext = ExtImpOutbrain::default();
        for i in 0..req_copy.imp.len() {
            let patch: ExtImpOutbrainPatch = match imp_bidder_params(req_copy.imp[i].ext.as_ref()) {
                Ok(v) => v,
                Err(m) => {
                    errs.push(BidderError::FailedToUnmarshal(m));
                    continue;
                }
            };
            merge_outbrain(&mut outbrain_ext, patch);
            if !outbrain_ext.tagid.is_empty() {
                req_copy.imp[i].tagid = outbrain_ext.tagid.clone();
            }
        }

        let publisher = Publisher {
            id: outbrain_ext.publisher.id.clone(),
            name: outbrain_ext.publisher.name.clone(),
            domain: outbrain_ext.publisher.domain.clone(),
            ..Default::default()
        };
        if let Some(site) = req_copy.site.as_mut() {
            site.publisher = Some(publisher);
        } else if let Some(app) = req_copy.app.as_mut() {
            app.publisher = Some(publisher);
        }

        if let Some(bcat) = outbrain_ext.bcat {
            req_copy.bcat = bcat;
        }
        if let Some(badv) = outbrain_ext.badv {
            req_copy.badv = Arc::from(badv);
        }

        let body = match crate::go_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::other(e.to_string()));
                return (vec![], errs);
            }
        };
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers: Header::new(),
                imp_ids: req_copy.imp.iter().map(|i| i.id.clone()).collect(),
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
        if is_response_status_code_no_content(response_data) {
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

        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        bid_response.currency = response.cur;

        let mut errs = Vec::new();
        for seat_bid in response.seatbid {
            for mut bid in seat_bid.bid {
                let bid_type = match get_media_type_for_imp(&bid.impid, &request.imp) {
                    Ok(t) => t,
                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                };
                if bid_type == BidType::Native {
                    let mut payload: NativeResponse = match jsonutil::unmarshal(bid.adm.as_bytes()) {
                        Ok(p) => p,
                        Err(e) => {
                            errs.push(e);
                            continue;
                        }
                    };
                    transform_event_trackers(&mut payload);
                    match crate::go_json::to_vec(&payload) {
                        Ok(b) => bid.adm = String::from_utf8_lossy(&b).into_owned(),
                        Err(e) => {
                            errs.push(BidderError::other(e.to_string()));
                            continue;
                        }
                    }
                }
                bid_response.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        (Some(bid_response), errs)
    }
}

/// Go `Unmarshal(bidderExt.Bidder, &outbrainExt)` decodes on top of the existing value: only the
/// keys present overwrite (`null` leaves a value alone, except slices which become nil).
#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpOutbrainPatch {
    publisher: Option<ExtImpOutbrainPublisherPatch>,
    tagid: Option<String>,
    #[serde(deserialize_with = "present")]
    bcat: Option<Option<Vec<String>>>,
    #[serde(deserialize_with = "present")]
    badv: Option<Option<Vec<String>>>,
}

/// `Some(x)` when the key is present (x is `None` for an explicit null).
fn present<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

fn merge_outbrain(dst: &mut ExtImpOutbrain, p: ExtImpOutbrainPatch) {
    if let Some(publisher) = p.publisher {
        if let Some(x) = publisher.id {
            dst.publisher.id = x;
        }
        if let Some(x) = publisher.name {
            dst.publisher.name = x;
        }
        if let Some(x) = publisher.domain {
            dst.publisher.domain = x;
        }
    }
    if let Some(t) = p.tagid {
        dst.tagid = t;
    }
    if let Some(b) = p.bcat {
        dst.bcat = b;
    }
    if let Some(b) = p.badv {
        dst.badv = b;
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpOutbrainPublisherPatch {
    id: Option<String>,
    name: Option<String>,
    domain: Option<String>,
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.native.is_some() {
                return Ok(BidType::Native);
            } else if imp.banner.is_some() {
                return Ok(BidType::Banner);
            } else if imp.video.is_some() {
                return Ok(BidType::Video);
            }
        }
    }
    Err(BidderError::bad_input(format!("Failed to find native/banner/video impression \"{imp_id}\" ")))
}

fn transform_event_trackers(payload: &mut NativeResponse) {
    for tracker in &payload.eventtrackers {
        if tracker.event != EventType::IMPRESSION {
            continue;
        }
        match tracker.method {
            EventTrackingMethod::IMAGE => payload.imptrackers.push(tracker.url.clone()),
            EventTrackingMethod::JS => {
                payload.jstracker = format!("<script src=\"{}\"></script>", tracker.url);
            }
            _ => {}
        }
    }
    payload.eventtrackers = Vec::new();
}
