use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, get_bid_type_from_mtype};
use openrtb_ext::ExtBidPrebidMeta;
use serde::Deserialize;

pub struct ReadpeakAdapter { pub endpoint: String }
impl ReadpeakAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Debug, Deserialize, Default)]
struct ImpExtReadpeak {
    #[serde(rename = "publisherId", default)]
    publisher_id: String,
    #[serde(rename = "siteId", default)]
    site_id: String,
    #[serde(rename = "bidfloor", default)]
    bidfloor: f64,
    #[serde(rename = "tagId", default)]
    tag_id: String,
}

impl Bidder for ReadpeakAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut req_copy = request.clone();
        let mut rp_ext = ImpExtReadpeak::default();
        let mut valid_imps = Vec::new();

        for imp in &req_copy.imp {
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(v) => v.clone(),
                None => { errs.push(BidderError::BadInput("missing bidder ext".to_string())); continue; }
            };

            let imp_rp_ext: ImpExtReadpeak = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(err) => { errs.push(BidderError::BadInput(err.to_string())); continue; }
            };

            rp_ext = imp_rp_ext;
            let mut imp_copy = imp.clone();
            if !rp_ext.tag_id.is_empty() {
                imp_copy.tagid = Some(rp_ext.tag_id.clone());
            }
            if rp_ext.bidfloor != 0.0 {
                imp_copy.bidfloor = Some(rp_ext.bidfloor);
            }
            valid_imps.push(imp_copy);
        }

        if valid_imps.is_empty() {
            let err = BidderError::BadInput(format!("Failed to find compatible impressions for request {}", request.id));
            return (vec![], vec![err]);
        }

        req_copy.imp = valid_imps;

        let publisher = openrtb::Publisher {
            id: Some(rp_ext.publisher_id.clone()),
            ..Default::default()
        };

        if let Some(site) = &request.site {
            let mut site_copy = site.clone();
            if !rp_ext.site_id.is_empty() {
                site_copy.id = Some(rp_ext.site_id.clone());
            }
            site_copy.publisher = Some(publisher);
            req_copy.site = Some(site_copy);
        } else if let Some(app) = &request.app {
            let mut app_copy = app.clone();
            if !rp_ext.site_id.is_empty() {
                app_copy.id = Some(rp_ext.site_id.clone());
            }
            app_copy.publisher = Some(publisher);
            req_copy.app = Some(app_copy);
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur { if !cur.is_empty() { result.currency = cur.clone(); } }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0);
                // Only banner (1) and native (4) supported
                if mtype == 0 || (mtype != 1 && mtype != 4) {
                    errs.push(BidderError::BadServerResponse(
                        format!("Failed to find impression type {}", bid.impid)
                    ));
                    continue;
                }
                // Resolve macros
                let price = bid.price;
                let price_str = format!("{}", price);
                if let Some(nurl) = &bid.nurl {
                    bid.nurl = Some(nurl.replace("${AUCTION_PRICE}", &price_str));
                }
                if let Some(adm) = &bid.adm {
                    bid.adm = Some(adm.replace("${AUCTION_PRICE}", &price_str));
                }
                if let Some(burl) = &bid.burl {
                    bid.burl = Some(burl.replace("${AUCTION_PRICE}", &price_str));
                }
                let adomain = bid.adomain.clone().unwrap_or_default();
                let bid_meta = Some(ExtBidPrebidMeta {
                    advertiser_domains: if adomain.is_empty() { None } else { Some(adomain) },
                    ..Default::default()
                });
                let bid_type = get_bid_type_from_mtype(mtype);
                let mut typed_bid = TypedBid::new(bid, bid_type);
                typed_bid.bid_meta = bid_meta;
                result.bids.push(typed_bid);
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
