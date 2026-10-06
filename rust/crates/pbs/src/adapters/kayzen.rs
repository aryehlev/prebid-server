//! Go `adapters/kayzen/kayzen.go`.

use serde::Deserialize;
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

pub struct Adapter {
    endpoint: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: &str) -> Result<Self, BidderError> {
        let template = EndpointTemplate::parse(endpoint).map_err(|e| {
            BidderError::other(format!("unable to parse endpoint url template: {e}"))
        })?;
        // Go's `template.Parse` rejects an action that is not a field (`{{Malformed}}`).
        template.resolve(&EndpointTemplateParams::default()).map_err(|e| {
            BidderError::other(format!("unable to parse endpoint url template: {e}"))
        })?;
        Ok(Self { endpoint: template })
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtKayzen {
    zone: String,
    exchange: String,
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

fn get_impression_ext(imp: &Imp) -> Result<ExtKayzen, BidderError> {
    let bidder_ext: ExtImpBidder = unmarshal_ext(imp.ext.as_ref()).map_err(|_| {
        BidderError::bad_input("Bidder extension not provided or can't be unmarshalled")
    })?;
    unmarshal_ext(bidder_ext.bidder.as_ref())
        .map_err(|_| BidderError::bad_input("Error while unmarshaling bidder extension"))
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return BidType::Banner;
            } else if imp.video.is_some() {
                return BidType::Video;
            } else if imp.native.is_some() {
                return BidType::Native;
            }
        }
    }
    BidType::Banner
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let Some(first) = request.imp.first() else {
            return (vec![], vec![BidderError::bad_input("Missing Imp Object")]);
        };
        let kayzen_ext = match get_impression_ext(first) {
            Ok(e) => e,
            Err(e) => return (vec![], vec![e]),
        };
        // Go clears `request.Imp[0].Ext` in place; a clone keeps the caller's request intact.
        let mut copy = request.clone();
        copy.imp[0].ext = None;

        let params = EndpointTemplateParams {
            zone_id: kayzen_ext.zone,
            account_id: kayzen_ext.exchange,
            ..Default::default()
        };
        let url = match self.endpoint.resolve(&params) {
            Ok(u) => u,
            Err(e) => return (vec![], vec![BidderError::other(e)]),
        };
        let body = match crate::go_json::to_vec(&copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: url,
                body,
                headers,
                imp_ids: copy.imp.iter().map(|i| i.id.clone()).collect(),
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
        match response.status_code {
            204 => return (None, vec![]),
            400 => {
                return (
                    None,
                    vec![BidderError::bad_input(
                        "Unexpected status code: 400. Bad request from publisher. Run with request.debug = 1 for more info.",
                    )],
                )
            }
            200 => {}
            code => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "Unexpected status code: {code}. Run with request.debug = 1 for more info."
                    ))],
                )
            }
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(_) => return (None, vec![BidderError::bad_server_response("Bad Server Response")]),
        };
        let mut out = BidderResponse::with_bids_capacity(1);
        for seat_bid in bid_resp.seatbid {
            for bid in seat_bid.bid {
                let t = get_media_type_for_imp(&bid.impid, &internal_request.imp);
                out.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(out), vec![])
    }
}
