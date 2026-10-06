//! Go `adapters/limelightDigital/limelightDigital.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

pub struct Adapter {
    endpoint_template: EndpointTemplate,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

/// Go `openrtb_ext.ImpExtLimelightDigital` (`PublisherID` is a `json.Number`).
#[derive(Deserialize, Default)]
#[serde(default)]
struct ImpExtLimelightDigital {
    host: String,
    #[serde(rename = "publisherId")]
    publisher_id: Option<Ext>,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Result<Self, BidderError> {
        let endpoint = endpoint.into();
        if endpoint.is_empty() {
            return Err(BidderError::other("Endpoint  adapter parameter is not provided"));
        }
        let t = EndpointTemplate::parse(&endpoint)
            .map_err(|e| BidderError::other(format!("unable to parse endpoint url template: {e}")))?;
        Ok(Self { endpoint_template: t })
    }
}

fn get_impression_ext(imp: &Imp) -> Result<ImpExtLimelightDigital, BidderError> {
    imp.ext
        .as_ref()
        .and_then(|e| e.decode::<ExtImpBidder>().ok())
        .and_then(|b| b.bidder)
        .and_then(|b| b.decode().ok())
        .ok_or_else(|| BidderError::bad_input("ext.bidder is not provided"))
}

fn number_string(v: &Option<Ext>) -> String {
    use sonic_rs::JsonValueTrait;
    match v {
        None => String::new(),
        Some(v) => match v.0.as_str() {
            Some(s) => s.to_string(),
            None => v.to_json(),
        },
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errors = Vec::new();
        let mut request_copy = request.clone();
        for imp in &request.imp {
            let ext = match get_impression_ext(imp) {
                Ok(e) => e,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            let params = EndpointTemplateParams {
                host: ext.host.clone(),
                publisher_id: number_string(&ext.publisher_id),
                ..Default::default()
            };
            let url = match self.endpoint_template.resolve(&params) {
                Ok(u) => u,
                Err(e) => {
                    errors.push(BidderError::other(e));
                    continue;
                }
            };
            let mut imp = imp.clone();
            imp.ext = None;
            if imp.bidfloor > 0.0 && !imp.bidfloorcur.is_empty() && imp.bidfloorcur.to_uppercase() != "USD" {
                match req_info.convert_currency(imp.bidfloor, &imp.bidfloorcur, "USD") {
                    Ok(v) => {
                        imp.bidfloorcur = "USD".into();
                        imp.bidfloor = v;
                    }
                    Err(e) => {
                        errors.push(e);
                        continue;
                    }
                }
            }
            request_copy.id = format!("{}-{}", request.id, imp.id);
            request_copy.imp = vec![imp];
            request_copy.ext = None;
            match crate::go_json::to_vec(&request_copy) {
                Ok(body) => requests.push(RequestData {
                    method: "POST".into(),
                    uri: url,
                    body,
                    headers: Default::default(),
                    imp_ids: request_copy.imp.iter().map(|i| i.id.clone()).collect(),
                }),
                Err(e) => errors.push(BidderError::other(e.to_string())),
            }
        }
        (requests, errors)
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response_data.status_code == 204 {
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
        if response_data.body.is_empty() {
            return (None, vec![]);
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        bid_response.currency = response.cur.clone();
        let mut errs = Vec::new();
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                match get_media_type_for_bid(&bid.impid, &request.imp) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        (Some(bid_response), errs)
    }
}

fn get_media_type_for_bid(imp_id: &str, imps: &[Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return Ok(BidType::Banner);
            } else if imp.video.is_some() {
                return Ok(BidType::Video);
            } else if imp.audio.is_some() {
                return Ok(BidType::Audio);
            } else if imp.native.is_some() {
                return Ok(BidType::Native);
            }
            return Err(BidderError::other(format!("unknown media type of imp: {imp_id}")));
        }
    }
    Err(BidderError::other(format!("bid contains unknown imp id: {imp_id}")))
}
