//! Go `adapters/oms/oms.go`.

use serde::Deserialize;

use crate::bid_types::{BidType, ExtBidPrebidVideo};
use crate::bidder::{
    Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, MarkupType};
use crate::ortb::Ext;

pub struct Adapter {
    endpoint: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

/// Go `openrtb_ext.ExtImpOms`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpOms {
    pid: String,
    #[serde(rename = "publisherid")]
    publisher_id: i64,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Result<Self, BidderError> {
        Ok(Self { endpoint: endpoint.into() })
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

        let mut uri = self.endpoint.clone();
        // Go indexes `request.Imp[0]` and would panic on an empty imp list: error instead.
        let Some(imp) = request.imp.first() else {
            return (vec![], vec![BidderError::other("index out of range [0] with length 0")]);
        };
        if let Some(ext) = &imp.ext {
            let bidder_ext: ExtImpBidder = match decode_ci(ext) {
                Ok(v) => v,
                Err(e) => return (vec![], vec![e]),
            };
            let oms: ExtImpOms = match bidder_ext.bidder {
                Some(b) => match decode_ci(&b) {
                    Ok(v) => v,
                    Err(e) => return (vec![], vec![e]),
                },
                None => return (vec![], vec![BidderError::FailedToUnmarshal("unexpected end of JSON input".into())]),
            };
            uri = format!("{}?publisherId={}", self.endpoint, oms.pid);
            if oms.pid.is_empty() && oms.publisher_id > 0 {
                uri = format!("{}?publisherId={}", self.endpoint, oms.publisher_id);
            }
        }

        let data = RequestData {
            method: "POST".into(),
            uri,
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
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        // Go: `if len(response.Cur) == 0 { Currency = response.Cur }` (sets it to "").
        if response.cur.is_empty() {
            bid_response.currency = response.cur.clone();
        }
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                let bid_type = if bid.mtype == MarkupType::VIDEO { BidType::Video } else { BidType::Banner };
                let video = get_bid_video(bid_type, &bid);
                let mut tb = TypedBid::new(bid, bid_type);
                tb.bid_video = video;
                bid_response.bids.push(tb);
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_bid_video(bid_type: BidType, bid: &Bid) -> Option<ExtBidPrebidVideo> {
    if bid_type != BidType::Video {
        return None;
    }
    Some(ExtBidPrebidVideo {
        duration: bid.dur as i32,
        primary_category: bid.cat.first().cloned().unwrap_or_default(),
    })
}

// ---- local helper: Go `jsonutil.Unmarshal` on an ext (json-iterator matches keys case-insensitively).
fn decode_ci<T: serde::de::DeserializeOwned>(ext: &crate::ortb::Ext) -> Result<T, BidderError> {
    let text = ext.to_json();
    if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(&text) {
        let lowered: serde_json::Map<String, serde_json::Value> =
            map.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect();
        return serde_json::from_value(serde_json::Value::Object(lowered))
            .map_err(|e| BidderError::FailedToUnmarshal(e.to_string()));
    }
    jsonutil::unmarshal(text.as_bytes())
}
