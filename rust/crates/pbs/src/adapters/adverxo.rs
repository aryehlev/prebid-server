//! Go `adapters/adverxo/adverxo.go`.

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::jsonutil;
use crate::header::Header;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp, MarkupType};
use serde::Deserialize;
use sonic_rs::JsonValueTrait;

/// Go `openrtb_ext.ImpExtAdverxo`.
#[derive(Debug, Default, Deserialize)]
struct ImpExtAdverxo {
    #[serde(rename = "adUnitId", default)]
    ad_unit_id: i64,
    #[serde(default)]
    auth: String,
}

#[derive(Debug, Default, Deserialize)]
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

    fn build_endpoint_url(&self, params: &ImpExtAdverxo) -> Result<String, String> {
        self.endpoint_template.resolve(&EndpointTemplateParams {
            ad_unit: params.ad_unit_id.to_string(),
            token_id: params.auth.clone(),
            ..Default::default()
        })
    }
}

/// Go `encoding/json.Unmarshal` error text for a value that is not an object.
fn go_std_type_error(v: &sonic_rs::Value) -> String {
    let kind = if v.is_str() {
        "string"
    } else if v.is_number() {
        "number"
    } else if v.is_boolean() {
        "bool"
    } else {
        "array"
    };
    format!("json: cannot unmarshal {kind} into Go value of type openrtb_ext.ImpExtAdverxo")
}

fn get_ad_units_params(imp: &Imp) -> Result<ImpExtAdverxo, BidderError> {
    let unmarshal_ext = || BidderError::bad_input(format!("imp {}: unable to unmarshal ext", imp.id));
    let ext = imp.ext.as_ref().ok_or_else(unmarshal_ext)?;
    if !(ext.0.is_object() || ext.0.is_null()) {
        return Err(unmarshal_ext());
    }
    let bidder_ext: ExtImpBidder = ext.decode().map_err(|_| unmarshal_ext())?;
    let wrap = |m: String| BidderError::bad_input(format!("imp {}: unable to unmarshal ext.bidder: {m}", imp.id));
    let Some(bidder) = bidder_ext.bidder.as_ref() else {
        return Err(wrap("unexpected end of JSON input".into()));
    };
    if !(bidder.0.is_object() || bidder.0.is_null()) {
        return Err(wrap(go_std_type_error(&bidder.0)));
    }
    bidder.decode::<ImpExtAdverxo>().map_err(|e| wrap(e.to_string()))
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        request_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut result = Vec::new();
        let mut errors = Vec::new();
        for imp in &request.imp {
            let mut imp = imp.clone();
            let params = match get_ad_units_params(&imp) {
                Ok(p) => p,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            let endpoint_url = match self.build_endpoint_url(&params) {
                Ok(u) => u,
                Err(e) => {
                    errors.push(BidderError::other(e));
                    continue;
                }
            };
            if imp.bidfloor > 0.0 && !imp.bidfloorcur.is_empty() && imp.bidfloorcur.to_uppercase() != "USD" {
                match request_info.convert_currency(imp.bidfloor, &imp.bidfloorcur, "USD") {
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
            let imp_id = imp.id.clone();
            let mut this_request = request.clone();
            this_request.imp = vec![imp];
            let body = match crate::go_json::to_vec(&this_request) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(BidderError::other(e.to_string()));
                    continue;
                }
            };
            result.push(RequestData {
                method: "POST".into(),
                uri: endpoint_url,
                body,
                headers: Header::new(),
                imp_ids: vec![imp_id],
            });
        }
        (result, errors)
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
        if response_data.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {}. Run with request.debug = 1 for more info.",
                    response_data.status_code
                ))],
            );
        }
        // Go parses this with encoding/json (not jsonutil).
        let response: BidResponse = match jsonutil::unmarshal_std(&response_data.body, "openrtb2.BidResponse") {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        bid_response.currency = response.cur.clone();
        for seat_bid in response.seatbid {
            for mut bid in seat_bid.bid {
                let bid_type = match get_media_type_for_bid(&bid) {
                    Ok(t) => t,
                    Err(e) => return (None, vec![e]),
                };
                // for native bid responses fix Adm field
                if bid_type == BidType::Native {
                    match get_native_adm(&bid.adm) {
                        Ok(a) => bid.adm = a,
                        Err(e) => return (None, vec![e]),
                    }
                }
                resolve_macros(&mut bid);
                bid_response.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type_for_bid(bid: &Bid) -> Result<BidType, BidderError> {
    match bid.mtype {
        MarkupType::BANNER => Ok(BidType::Banner),
        MarkupType::NATIVE => Ok(BidType::Native),
        MarkupType::VIDEO => Ok(BidType::Video),
        other => Err(BidderError::bad_server_response(format!("unsupported MType {}", other.0))),
    }
}

fn get_native_adm(adm: &str) -> Result<String, BidderError> {
    let native_adm: std::collections::BTreeMap<String, Box<serde_json::value::RawValue>> =
        serde_json::from_str(adm).map_err(|_| BidderError::other("unable to unmarshal native adm"))?;
    // move bid.adm.native to bid.adm (raw text, like jsonparser, without re-encoding)
    if let Some(value) = native_adm.get("native") {
        if !value.get().trim_start().starts_with('{') {
            return Err(BidderError::other("unable to get native adm"));
        }
        return Ok(value.get().to_string());
    }
    Ok(adm.to_string())
}

fn resolve_macros(bid: &mut Bid) {
    // Go `strconv.FormatFloat(price, 'f', -1, 64)`; Rust's Display is also the shortest form.
    let price = format!("{}", bid.price);
    bid.adm = bid.adm.replace("${AUCTION_PRICE}", &price);
}
