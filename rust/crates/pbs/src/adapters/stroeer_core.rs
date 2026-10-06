//! Go `adapters/stroeerCore/stroeercore.go`.

use serde::{Deserialize, Serialize};

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, MarkupType};
use crate::ortb::Ext;

pub struct Adapter {
    url: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { url: endpoint.into() }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Response {
    bids: Vec<BidResponse>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidResponse {
    id: String,
    #[serde(rename = "bidId")]
    bid_id: String,
    cpm: f64,
    width: i64,
    height: i64,
    ad: String,
    crid: String,
    mtype: String,
    dsa: Option<Ext>,
    adomain: Vec<String>,
}

#[derive(Serialize)]
struct BidExt<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    dsa: Option<&'a Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpStroeerCore {
    sid: String,
}

fn ext_text(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

impl BidResponse {
    fn resolve_media_type(&self) -> Result<(MarkupType, BidType), String> {
        match self.mtype.as_str() {
            "banner" => Ok((MarkupType::BANNER, BidType::Banner)),
            "video" => Ok((MarkupType::VIDEO, BidType::Video)),
            _ => Err(format!("unable to determine media type for bid with id \"{}\"", self.bid_id)),
        }
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        bid_request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors = vec![];
        let mut req = bid_request.clone();
        for imp in req.imp.iter_mut() {
            let bidder_ext: ExtImpBidder = match jsonutil::unmarshal(&ext_text(&imp.ext)) {
                Ok(e) => e,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            let stroeer_ext: ExtImpStroeerCore = match jsonutil::unmarshal(&ext_text(&bidder_ext.bidder)) {
                Ok(e) => e,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            imp.tagid = stroeer_ext.sid;
        }
        let body = match crate::go_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => {
                errors.push(BidderError::other(e.to_string()));
                return (vec![], errors);
            }
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.url.clone(),
                body,
                headers,
                imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            errors,
        )
    }

    fn make_bids(
        &self,
        _bid_request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response_data.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected http status code: {}.",
                    response_data.status_code
                ))],
            );
        }
        let mut errors = vec![];
        let stroeer_response: Response = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bidder_response = BidderResponse::with_bids_capacity(stroeer_response.bids.len());
        bidder_response.currency = "EUR".to_string();
        for bid in stroeer_response.bids {
            let (markup_type, bid_type) = match bid.resolve_media_type() {
                Ok(t) => t,
                Err(e) => {
                    errors.push(BidderError::bad_server_response(format!("Bid media type error: {e}")));
                    continue;
                }
            };
            let mut open_rtb_bid = Bid {
                id: bid.id,
                impid: bid.bid_id,
                w: bid.width,
                h: bid.height,
                price: bid.cpm,
                adm: bid.ad,
                crid: bid.crid,
                mtype: markup_type,
                adomain: bid.adomain,
                ..Default::default()
            };
            if bid.dsa.is_some() {
                match Ext::from_serialize(&BidExt { dsa: bid.dsa.as_ref() }) {
                    Ok(e) => open_rtb_bid.ext = Some(e),
                    Err(e) => errors.push(BidderError::other(e.to_string())),
                }
            }
            bidder_response.bids.push(TypedBid::new(open_rtb_bid, bid_type));
        }
        (Some(bidder_response), errors)
    }
}
