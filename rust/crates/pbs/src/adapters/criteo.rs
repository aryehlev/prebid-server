//! Go `adapters/criteo/criteo.go`.

use serde::Deserialize;

use crate::bid_types::{BidType, ExtBidPrebidMeta};
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse};
use crate::ortb::Ext;

pub struct Adapter {
    endpoint: String,
    bidder_name: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidExt {
    prebid: ExtPrebid,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtPrebid {
    #[serde(rename = "type")]
    bid_type: String,
    #[serde(rename = "networkName")]
    network_name: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct CriteoExt {
    igi: Vec<Option<CriteoExtIgi>>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct CriteoExtIgi {
    impid: String,
    igs: Vec<Option<CriteoExtIgs>>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct CriteoExtIgs {
    config: Option<Ext>,
}

impl Adapter {
    /// Go `Builder` for bidder name `criteo`.
    pub fn new(endpoint: impl Into<String>) -> Result<Self, BidderError> {
        Self::with_bidder_name(endpoint, "criteo")
    }

    /// Go `Builder` with an explicit `bidderName`.
    pub fn with_bidder_name(endpoint: impl Into<String>, bidder_name: &str) -> Result<Self, BidderError> {
        Ok(Self { endpoint: endpoint.into(), bidder_name: bidder_name.into() })
    }

    /// Go `ParseFledgeAuctionConfigs`, as the JSON array Go would marshal.
    fn parse_fledge_auction_configs(&self, response: &BidResponse) -> Option<Ext> {
        let ext: CriteoExt = response.ext.as_ref().and_then(|e| e.decode().ok())?;
        if ext.igi.is_empty() {
            return None;
        }
        let mut configs = Vec::new();
        for igi in ext.igi.into_iter().flatten() {
            if let Some(Some(igs)) = igi.igs.first() {
                if let Some(config) = &igs.config {
                    let mut o = serde_json::Map::new();
                    o.insert("impid".into(), serde_json::Value::String(igi.impid.clone()));
                    if !self.bidder_name.is_empty() {
                        o.insert("bidder".into(), serde_json::Value::String(self.bidder_name.clone()));
                    }
                    o.insert("config".into(), serde_json::from_str(&config.to_json()).ok()?);
                    configs.push(serde_json::Value::Object(o));
                }
            }
        }
        if configs.is_empty() {
            return None;
        }
        Ext::from_slice(&serde_json::to_vec(&configs).ok()?).ok()
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
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers: Default::default(),
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response_data.status_code == 204 {
            return (None, vec![]);
        }
        if response_data.status_code == 400 {
            return (
                None,
                vec![BidderError::bad_input("Unexpected status code: 400. Run with request.debug = 1 for more info.")],
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
        let mut bid_response = BidderResponse::new();
        bid_response.currency = response.cur.clone();
        bid_response.fledge_auction_configs = self.parse_fledge_auction_configs(&response);

        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                let missing = || {
                    BidderError::bad_server_response(format!(
                        "Missing ext.prebid.type in bid for impression : {}.",
                        bid.impid
                    ))
                };
                let bid_ext: BidExt = match bid.ext.as_ref().and_then(|e| e.decode().ok()) {
                    Some(b) => b,
                    None => return (None, vec![missing()]),
                };
                // Go keeps any string as the BidType; an unknown value cannot be represented, so it
                // is reported like a missing type.
                let bid_type = match BidType::parse(&bid_ext.prebid.bid_type) {
                    Ok(t) => t,
                    Err(_) => return (None, vec![missing()]),
                };
                let mut tb = TypedBid::new(bid, bid_type);
                if !bid_ext.prebid.network_name.is_empty() {
                    tb.bid_meta = Some(ExtBidPrebidMeta {
                        network_name: bid_ext.prebid.network_name.clone(),
                        ..Default::default()
                    });
                }
                bid_response.bids.push(tb);
            }
        }
        (Some(bid_response), vec![])
    }
}
