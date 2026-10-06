//! Go `adapters/consumable/consumable.go`.

use serde::Deserialize;
use sonic_rs::JsonValueTrait;

use crate::bid_types::{BidType, ExtBidPrebidVideo};
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp, MarkupType};
use crate::ortb::Ext;

/// Go `openrtb_ext.ExtImpConsumable`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtImpConsumable {
    #[serde(rename = "networkId", deserialize_with = "crate::ortb::de::int")]
    network_id: i64,
    #[serde(rename = "siteId", deserialize_with = "crate::ortb::de::int")]
    site_id: i64,
    #[serde(rename = "unitId", deserialize_with = "crate::ortb::de::int")]
    unit_id: i64,
    #[serde(rename = "unitName")]
    unit_name: String,
    #[serde(rename = "placementid", alias = "placementId")] // Go matches keys case-insensitively
    placement_id: String,
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

/// Go: `jsonutil.Unmarshal(imp.Ext, &ExtImpBidder)` then `jsonutil.Unmarshal(bidderExt.Bidder, &T)`.
/// Returns the Go error message text of whichever step fails.
fn imp_bidder_params<T: serde::de::DeserializeOwned>(ext: Option<&Ext>) -> Result<T, String> {
    fn not_obj(v: &sonic_rs::Value) -> Option<String> {
        if v.is_object() || v.is_null() {
            return None;
        }
        let first = sonic_rs::to_string(v).ok()?.chars().next().unwrap_or('\u{0}');
        Some(format!("expect {{ or n, but found {first}"))
    }
    let Some(ext) = ext else {
        return Err("expect { or n, but found \u{0}".to_string());
    };
    if let Some(m) = not_obj(&ext.0) {
        return Err(m);
    }
    match ext.0.get("bidder") {
        None => Err("expect { or n, but found \u{0}".to_string()),
        Some(b) => {
            if let Some(m) = not_obj(b) {
                return Err(m);
            }
            sonic_rs::from_value::<T>(b).map_err(|e| e.to_string())
        }
    }
}

/// Go `extractExtensions` (only the bidder params are used by the caller).
fn extract_extensions(imp: &Imp) -> Result<ExtImpConsumable, Vec<BidderError>> {
    imp_bidder_params(imp.ext.as_ref()).map_err(|m| vec![BidderError::bad_input(m)])
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json");
        headers.add("Accept", "application/json");
        let body = match crate::go_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };

        // Go indexes `request.Imp[0]` and panics on an empty imp list.
        let Some(imp0) = request.imp.first() else {
            return (vec![], vec![BidderError::bad_input("no imps in the request")]);
        };
        let ext = match extract_extensions(imp0) {
            Ok(e) => e,
            Err(errs) => return (vec![], errs),
        };

        let uri = if request.site.is_some() {
            if ext.site_id == 0 && ext.network_id == 0 && ext.unit_id == 0 {
                return (
                    vec![],
                    vec![BidderError::failed_to_request_bids(
                        "SiteId, NetworkId and UnitId are all required for site requests",
                    )],
                );
            }
            format!("{}/sb/rtb", self.endpoint)
        } else {
            if ext.placement_id.is_empty() {
                return (
                    vec![],
                    vec![BidderError::failed_to_request_bids("PlacementId is required for non-site requests")],
                );
            }
            format!("{}/rtb/bid?s={}", self.endpoint, ext.placement_id)
        };
        (
            vec![RequestData {
                method: "POST".into(),
                uri,
                body,
                headers,
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
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
        bid_response.currency = response.cur;
        for seat_bid in response.seatbid {
            for mut bid in seat_bid.bid {
                // Go swallows the error and skips the bid.
                let Ok(bid_type) = get_media_type_for_bid(&bid) else { continue };
                let bid_video = (bid_type == BidType::Video)
                    .then(|| ExtBidPrebidVideo { duration: bid.dur as i32, ..Default::default() });
                match bid_type {
                    BidType::Audio => bid.mtype = MarkupType::AUDIO,
                    BidType::Video => bid.mtype = MarkupType::VIDEO,
                    BidType::Banner => bid.mtype = MarkupType::BANNER,
                    BidType::Native | BidType::Other => {}
                }
                let mut typed = TypedBid::new(bid, bid_type);
                typed.bid_video = bid_video;
                bid_response.bids.push(typed);
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type_for_bid(bid: &Bid) -> Result<BidType, BidderError> {
    match bid.mtype {
        MarkupType::BANNER => return Ok(BidType::Banner),
        MarkupType::VIDEO => return Ok(BidType::Video),
        MarkupType::AUDIO => return Ok(BidType::Audio),
        _ => {}
    }
    Err(BidderError::bad_server_response(format!("Failed to parse impression \"{}\" mediatype", bid.impid)))
}
