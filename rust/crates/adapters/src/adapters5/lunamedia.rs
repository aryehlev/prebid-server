use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct LunamediaAdapter { pub endpoint: String }
impl LunamediaAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Deserialize, Default, Clone, PartialEq, Eq, Hash)]
struct LunamediaImpExt {
    #[serde(rename = "pubid", default)]
    pubid: String,
    #[serde(rename = "placement", default)]
    placement: String,
}

fn get_impression_ext(imp: &openrtb::Imp) -> Result<LunamediaImpExt, BidderError> {
    let ext = imp.ext.as_ref()
        .ok_or_else(|| BidderError::BadInput("missing imp.ext".to_string()))?;
    let bidder_val = ext.get("bidder")
        .ok_or_else(|| BidderError::BadInput("missing imp.ext.bidder".to_string()))?;
    serde_json::from_value::<LunamediaImpExt>(bidder_val.clone())
        .map_err(|e| BidderError::BadInput(e.to_string()))
}

/// Set banner W/H from first format entry if not already set.
fn compat_banner_imp(imp: &mut openrtb::Imp) -> Result<(), BidderError> {
    if let Some(banner) = imp.banner.as_mut() {
        if banner.w.is_none() || banner.h.is_none() {
            let formats = banner.format.as_deref().unwrap_or(&[]);
            if formats.is_empty() {
                return Err(BidderError::BadInput(
                    "Expected at least one banner.format entry or explicit w/h".to_string(),
                ));
            }
            let first = formats[0].clone();
            let remaining: Vec<_> = formats[1..].to_vec();
            banner.w = first.w;
            banner.h = first.h;
            banner.format = if remaining.is_empty() { None } else { Some(remaining) };
        }
    }
    Ok(())
}

fn get_media_type_for_imp_id(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id && imp.video.is_some() {
            return BidType::Video;
        }
    }
    BidType::Banner
}

impl Bidder for LunamediaAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No impression in the bid request".to_string())]);
        }

        let mut errs = Vec::new();
        // Group imps by LunaMedia ext key (pubid)
        let mut groups: HashMap<LunamediaImpExt, Vec<openrtb::Imp>> = HashMap::new();

        for imp in &request.imp {
            let imp_ext = match get_impression_ext(imp) {
                Ok(e) => e,
                Err(e) => { errs.push(e); continue; }
            };
            if imp_ext.pubid.is_empty() {
                errs.push(BidderError::BadInput("No pubid value provided".to_string()));
                continue;
            }

            let mut imp_copy = imp.clone();
            // Clear ext — don't forward to LunaMedia platform
            imp_copy.ext = None;
            // Set tagid to placement
            if !imp_ext.placement.is_empty() {
                imp_copy.tagid = Some(imp_ext.placement.clone());
            }
            // Compat banner
            if imp_copy.banner.is_some() {
                if let Err(e) = compat_banner_imp(&mut imp_copy) {
                    errs.push(e);
                    continue;
                }
            }

            groups.entry(imp_ext).or_default().push(imp_copy);
        }

        if groups.is_empty() {
            return (vec![], errs);
        }

        let mut requests = Vec::new();

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("x-openrtb-version".to_string(), "2.5".to_string());

        for (ext_key, imps) in groups {
            let mut req_copy = request.clone();
            req_copy.imp = imps;

            // Clear publisher from site/app
            if let Some(site) = req_copy.site.as_mut() {
                site.publisher = None;
                site.domain = None;
            }
            if let Some(app) = req_copy.app.as_mut() {
                app.publisher = None;
            }

            // Build URI: replace {{.PublisherID}} macro
            let uri = self.endpoint.replace("{{.PublisherID}}", &ext_key.pubid);

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); return (vec![], errs); }
            };

            requests.push(RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers: headers.clone(),
                imp_ids: get_imp_ids(&req_copy.imp),
            });
        }

        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected http status code: {}", response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("Bad server response: {}", e))])?;

        if bid_resp.seatbid.len() != 1 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Invalid SeatBids count: {}", bid_resp.seatbid.len()
            ))]);
        }

        let seat_bid = &bid_resp.seatbid[0];
        let mut result = BidderResponse::with_capacity(seat_bid.bid.len());

        for bid in &seat_bid.bid {
            let bid_type = get_media_type_for_imp_id(&bid.impid, &internal.imp);
            result.bids.push(TypedBid::new(bid.clone(), bid_type));
        }

        Ok(result)
    }
}
