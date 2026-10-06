//! Go `adapters/readpeak/readpeak.go`.

#![allow(dead_code, unused_imports)]

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

use crate::ortb::openrtb2::Publisher;

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

/// Go `openrtb_ext.ImpExtReadpeak`; Go reuses one struct across imps, so a later imp that omits
/// a field keeps the previous value. `None` marks "not in this JSON".
#[derive(serde::Deserialize, Default)]
struct ImpExtReadpeak {
    #[serde(rename = "publisherId", default)]
    publisher_id: Option<String>,
    #[serde(rename = "siteId", default)]
    site_id: Option<String>,
    #[serde(default)]
    bidfloor: Option<f64>,
    #[serde(rename = "tagId", default)]
    tag_id: Option<String>,
}

#[derive(Default)]
struct RpExt {
    publisher_id: String,
    site_id: String,
    bidfloor: f64,
    tag_id: String,
}

impl RpExt {
    fn merge(&mut self, o: ImpExtReadpeak) {
        if let Some(v) = o.publisher_id {
            self.publisher_id = v;
        }
        if let Some(v) = o.site_id {
            self.site_id = v;
        }
        if let Some(v) = o.bidfloor {
            self.bidfloor = v;
        }
        if let Some(v) = o.tag_id {
            self.tag_id = v;
        }
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors = vec![];
        let mut rp_ext = RpExt::default();
        let mut imps: Vec<Imp> = vec![];
        for src in &request.imp {
            let params: ImpExtReadpeak = match parse_imp_ext(src, |e| e, |e| e) {
                Ok(p) => p,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            rp_ext.merge(params);
            let mut imp = src.clone();
            if !rp_ext.tag_id.is_empty() {
                imp.tagid = rp_ext.tag_id.clone();
            }
            if rp_ext.bidfloor != 0.0 {
                imp.bidfloor = rp_ext.bidfloor;
            }
            imps.push(imp);
        }

        if imps.is_empty() {
            return (
                vec![],
                vec![BidderError::bad_input(format!(
                    "Failed to find compatible impressions for request {}",
                    request.id
                ))],
            );
        }

        let mut request_copy = request.clone();
        request_copy.imp = imps;
        let publisher = Publisher { id: rp_ext.publisher_id.clone(), ..Default::default() };
        if let Some(site) = request_copy.site.as_mut() {
            if !rp_ext.site_id.is_empty() {
                site.id = rp_ext.site_id.clone();
            }
            site.publisher = Some(publisher);
        } else if let Some(app) = request_copy.app.as_mut() {
            if !rp_ext.site_id.is_empty() {
                app.id = rp_ext.site_id.clone();
            }
            app.publisher = Some(publisher);
        }

        let body = match crate::go_json::to_vec(&request_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        let data = RequestData {
            method: "POST".into(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
        };
        (vec![data], errors)
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
        if !response.cur.is_empty() {
            bid_response.currency = response.cur.clone();
        }
        let mut errors = vec![];
        for seat_bid in response.seatbid {
            for mut bid in seat_bid.bid {
                let bid_type = match bid.mtype {
                    crate::ortb::openrtb2::MarkupType::BANNER => BidType::Banner,
                    crate::ortb::openrtb2::MarkupType::NATIVE => BidType::Native,
                    _ => {
                        errors.push(BidderError::other(format!(
                            "Failed to find impression type \"{}\"",
                            bid.impid
                        )));
                        continue;
                    }
                };
                let price = format_price(bid.price);
                bid.nurl = bid.nurl.replace("${AUCTION_PRICE}", &price);
                bid.adm = bid.adm.replace("${AUCTION_PRICE}", &price);
                bid.burl = bid.burl.replace("${AUCTION_PRICE}", &price);
                let mut typed = TypedBid::new(bid, bid_type);
                typed.bid_meta = Some(crate::bid_types::ExtBidPrebidMeta {
                    advertiser_domains: typed.bid.adomain.clone(),
                    ..Default::default()
                });
                bid_response.bids.push(typed);
            }
        }
        (Some(bid_response), errors)
    }
}

// ── local helpers (Go `adapters.ExtImpBidder`, `openrtb_ext.ExtBid`) ─────────────────────────

/// Go `adapters.ExtImpBidder` (only `bidder` is used here).
#[derive(serde::Deserialize, Default)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<Ext>,
}

/// Go `jsonutil.Unmarshal(raw, &v)` on a `json.RawMessage` field. An absent message is empty
/// input, which json-iterator rejects with the same `expect { or n, but found` text.
fn unmarshal_ext<T: serde::de::DeserializeOwned>(ext: Option<&Ext>) -> Result<T, BidderError> {
    match ext {
        Some(e) => jsonutil::unmarshal(e.to_json().as_bytes()),
        None => Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string())),
    }
}

/// Go: unmarshal `imp.ext` into `ExtImpBidder`, then `.Bidder` into `T`; the two errors are
/// returned separately so adapters can wrap them differently.
fn parse_imp_ext<T: serde::de::DeserializeOwned>(
    imp: &Imp,
    wrap_bidder_ext: impl Fn(BidderError) -> BidderError,
    wrap_params: impl Fn(BidderError) -> BidderError,
) -> Result<T, BidderError> {
    let bidder_ext: ExtImpBidder = unmarshal_ext(imp.ext.as_ref()).map_err(wrap_bidder_ext)?;
    unmarshal_ext(bidder_ext.bidder.as_ref()).map_err(wrap_params)
}

/// Go `openrtb_ext.ExtBid` (`bid.ext.prebid.type`).
#[derive(serde::Deserialize, Default)]
struct ExtBid {
    #[serde(default)]
    prebid: Option<ExtBidPrebid>,
}

#[derive(serde::Deserialize, Default)]
struct ExtBidPrebid {
    #[serde(rename = "type", default)]
    bid_type: String,
}

/// Go `jsonutil.Unmarshal(bid.Ext, &ExtBid)`: `Err` on a malformed ext.
fn parse_bid_ext(bid: &Bid) -> Option<Result<ExtBid, BidderError>> {
    bid.ext.as_ref().map(|e| jsonutil::unmarshal(e.to_json().as_bytes()))
}

/// Go `strconv.FormatFloat(price, 'f', -1, 64)`.
fn format_price(price: f64) -> String {
    format!("{price}")
}
