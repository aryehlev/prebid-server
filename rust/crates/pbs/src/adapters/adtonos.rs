//! Go `adapters/adtonos/adtonos.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder,
    BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp, MarkupType};
use crate::ortb::Ext;

pub struct Adapter {
    endpoint_template: EndpointTemplate,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ImpExtAdTonos {
    #[serde(rename = "supplierId")]
    supplier_id: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Result<Self, BidderError> {
        let endpoint = endpoint.into();
        let t = EndpointTemplate::parse(&endpoint).map_err(|e| {
            BidderError::other(format!("unable to parse endpoint url template: {e}"))
        })?;
        Ok(Self { endpoint_template: t })
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        // Go indexes `request.Imp[0]` (panics when empty): error instead.
        let Some(imp0) = request.imp.first() else {
            return (vec![], vec![BidderError::other("index out of range [0] with length 0")]);
        };
        let bidder_ext: ExtImpBidder = match imp0.ext.as_ref().map(|e| e.decode()) {
            Some(Ok(v)) => v,
            Some(Err(e)) => return (vec![], vec![bad_ext("imp.ext", e.to_string())]),
            None => return (vec![], vec![bad_ext("imp.ext", "unexpected end of JSON input".into())]),
        };
        let imp_ext: ImpExtAdTonos = match bidder_ext.bidder.as_ref().map(|e| e.decode()) {
            Some(Ok(v)) => v,
            Some(Err(e)) => return (vec![], vec![bad_ext("imp.ext.bidder", e.to_string())]),
            None => return (vec![], vec![bad_ext("imp.ext.bidder", "unexpected end of JSON input".into())]),
        };

        let params = EndpointTemplateParams { publisher_id: imp_ext.supplier_id, ..Default::default() };
        let endpoint = match self.endpoint_template.resolve(&params) {
            Ok(e) => e,
            Err(e) => return (vec![], vec![BidderError::other(e)]),
        };
        let body = match crate::go_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        let data = RequestData {
            method: "POST".into(),
            uri: endpoint,
            body,
            headers,
            imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
        };
        (vec![data], vec![])
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
        if let Some(err) = check_response_status_code_for_errors(response_data) {
            return (None, vec![err]);
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        bid_response.currency = response.cur.clone();
        let mut errors = Vec::new();
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                match get_media_type_for_bid(&bid, &request.imp) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errors.push(e),
                }
            }
        }
        (Some(bid_response), errors)
    }
}

fn bad_ext(what: &str, err: String) -> BidderError {
    BidderError::bad_input(format!(
        "Invalid {what} for impression index {} . Error Infomation: {err}",
        0
    ))
    .fix_spacing()
}

trait FixSpacing {
    fn fix_spacing(self) -> Self;
}
impl FixSpacing for BidderError {
    // Go formats "...index %d. Error Infomation: %s" (no space before the period).
    fn fix_spacing(self) -> Self {
        match self {
            BidderError::BadInput(m) => BidderError::BadInput(m.replace("index 0 . Error", "index 0. Error")),
            o => o,
        }
    }
}

fn get_media_type_for_bid(bid: &Bid, imps: &[Imp]) -> Result<BidType, BidderError> {
    if bid.mtype.0 != 0 {
        match bid.mtype {
            MarkupType::AUDIO => return Ok(BidType::Audio),
            MarkupType::VIDEO => return Ok(BidType::Video),
            MarkupType::BANNER => return Ok(BidType::Banner),
            MarkupType::NATIVE => return Ok(BidType::Native),
            _ => {}
        }
    }
    for imp in imps {
        if imp.id == bid.impid {
            if imp.audio.is_some() {
                return Ok(BidType::Audio);
            } else if imp.video.is_some() {
                return Ok(BidType::Video);
            } else {
                return Err(BidderError::bad_input(format!("Unsupported bidtype for bid: \"{}\"", bid.impid)));
            }
        }
    }
    Err(BidderError::bad_input(format!("Failed to find impression: \"{}\"", bid.impid)))
}
