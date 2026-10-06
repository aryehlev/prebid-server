//! Go `adapters/trafficgate/trafficgate.go`.

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use serde::Deserialize;

/// Go `openrtb_ext.ExtImpTrafficGate` (also the grouping key of `splitImpressions`).
#[derive(Debug, Default, Deserialize, Clone, PartialEq, Eq)]
struct ExtImpTrafficGate {
    #[serde(rename = "placementId", default)]
    placement_id: String,
    #[serde(default)]
    host: String,
}

#[derive(Debug, Default, Deserialize)]
struct BidResponseExtPrebid {
    #[serde(rename = "Type", alias = "type", default)]
    r#type: String,
}

#[derive(Debug, Default, Deserialize)]
struct BidResponseExt {
    #[serde(rename = "Prebid", alias = "prebid", default)]
    prebid: BidResponseExtPrebid,
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
    endpoint_template: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: &str) -> Result<Self, String> {
        let tmpl = EndpointTemplate::parse(endpoint)
            .map_err(|e| format!("unable to parse endpoint url template: {e}"))?;
        Ok(Self { endpoint_template: tmpl })
    }
}

fn get_bidder_params(imp: &Imp) -> Result<ExtImpTrafficGate, BidderError> {
    let bidder_ext: ExtImpBidder =
        decode_ext(imp.ext.as_ref()).map_err(|_| BidderError::bad_input("Missing bidder ext"))?;
    decode_ext(bidder_ext.bidder.as_ref()).map_err(|_| BidderError::bad_input("Bidder parameters required"))
}

/// Go `splitImpressions`: groups imps by their params. Go ranges over a map (random order);
/// first-seen order is used here.
fn split_impressions(imps: &[Imp]) -> Result<Vec<(ExtImpTrafficGate, Vec<Imp>)>, BidderError> {
    let mut groups: Vec<(ExtImpTrafficGate, Vec<Imp>)> = Vec::new();
    for imp in imps {
        let params = get_bidder_params(imp)?;
        match groups.iter_mut().find(|(k, _)| *k == params) {
            Some((_, v)) => v.push(imp.clone()),
            None => groups.push((params, vec![imp.clone()])),
        }
    }
    Ok(groups)
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut headers = Header::new();
        headers.set("Content-Type", "application/json");
        headers.set("Accept", "application/json");
        let groups = match split_impressions(&request.imp) {
            Ok(g) => g,
            Err(e) => return (vec![], vec![e]),
        };
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        for (req_ext, req_imp) in groups {
            let mut req = request.clone();
            req.imp = req_imp;
            let body = match crate::go_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::other(e.to_string()));
                    continue;
                }
            };
            let params = EndpointTemplateParams { host: req_ext.host, ..Default::default() };
            let url = match self.endpoint_template.resolve(&params) {
                Ok(u) => u,
                Err(e) => {
                    errs.push(BidderError::other(e));
                    continue;
                }
            };
            requests.push(RequestData {
                method: "POST".into(),
                uri: url,
                body,
                headers: headers.clone(),
                imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
            });
        }
        (requests, errs)
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
                vec![BidderError::bad_server_response(format!("Error response with status {}", response.status_code))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![BidderError::bad_server_response(e.to_string())]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(internal_request.imp.len());
        bid_response.currency = bid_resp.cur.clone();
        for seat_bid in bid_resp.seatbid {
            for bid in seat_bid.bid {
                let bid_ext: BidResponseExt = match decode_ext(bid.ext.as_ref()) {
                    Ok(v) => v,
                    Err(_) => return (None, vec![BidderError::bad_server_response("Missing response ext")]),
                };
                if bid_ext.prebid.r#type.is_empty() {
                    return (None, vec![BidderError::bad_server_response("Unable to read bid.ext.prebid.type")]);
                }
                let t = get_media_type_for_imp(&bid_ext.prebid.r#type);
                bid_response.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type_for_imp(bid_type: &str) -> BidType {
    match bid_type {
        "video" => BidType::Video,
        "native" => BidType::Native,
        "audio" => BidType::Audio,
        _ => BidType::Banner,
    }
}
