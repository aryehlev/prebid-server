//! Go `adapters/vox/vox.go`.

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, MarkupType};

/// Go `openrtb_ext.ImpExtVox` (never read by the adapter).
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[allow(dead_code)]
pub struct ImpExtVox {
    #[serde(rename = "placementId")]
    pub placement_id: String,
    #[serde(rename = "imageUrl")]
    pub image_url: String,
    #[serde(rename = "displaySizes")]
    pub display_sizes: Vec<String>,
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

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let body = match crate::go_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        let data = RequestData {
            method: "POST".into(),
            uri: self.endpoint.clone(),
            body,
            headers: Header::new(),
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
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                let bid_type = match get_media_type_for_bid(&bid) {
                    Ok(t) => t,
                    Err(e) => return (None, vec![e]),
                };
                bid_response.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type_for_bid(bid: &Bid) -> Result<BidType, BidderError> {
    match bid.mtype {
        MarkupType::BANNER => Ok(BidType::Banner),
        MarkupType::VIDEO => Ok(BidType::Video),
        MarkupType::AUDIO => Ok(BidType::Audio),
        MarkupType::NATIVE => Ok(BidType::Native),
        _ => Err(BidderError::other(format!("Unable to fetch mediaType in multi-format: {}", bid.impid))),
    }
}
