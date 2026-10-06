//! Go `adapters/kargo/kargo.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{
    is_response_status_code_no_content, Bidder, BidderResponse, ExtraRequestInfo, RequestData,
    ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse};
use crate::ortb::Ext;

pub struct Adapter {
    uri: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct KargoExt {
    #[serde(rename = "mediaType")]
    media_type: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Result<Self, BidderError> {
        Ok(Self { uri: endpoint.into() })
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
            uri: self.uri.clone(),
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
        bid_response.currency = response.cur.clone();
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                let bid_type = get_media_type_for_bid(bid.ext.as_ref());
                bid_response.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type_for_bid(ext: Option<&Ext>) -> BidType {
    if let Some(ext) = ext {
        if let Ok(e) = ext.decode::<KargoExt>() {
            match e.media_type.as_str() {
                "video" => return BidType::Video,
                "native" => return BidType::Native,
                _ => {}
            }
        }
    }
    BidType::Banner
}
