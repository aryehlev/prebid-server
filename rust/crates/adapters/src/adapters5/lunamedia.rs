use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct LunamediaAdapter {
    pub endpoint: String,
}

impl LunamediaAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize, Clone, PartialEq, Eq, Hash, Default)]
struct LunamediaImpExt {
    #[serde(rename = "pubId", default)]
    pub_id: String,
    #[serde(rename = "placement", default)]
    placement: String,
}

/// Return the bid type based on whether the matching imp has a Video field.
fn get_media_type_for_imp_id(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id && imp.video.is_some() {
            return BidType::Video;
        }
    }
    BidType::Banner
}

impl Bidder for LunamediaAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (
                vec![],
                vec![BidderError::BadInput(
                    "No impression in the bid request".to_string(),
                )],
            );
        }

        // Group impressions by (pub_id, placement)
        let mut groups: HashMap<LunamediaImpExt, Vec<openrtb::Imp>> = HashMap::new();
        let mut errors: Vec<BidderError> = Vec::new();

        for imp in &request.imp {
            let ext = match imp.ext.as_ref() {
                Some(e) => e,
                None => {
                    errors.push(BidderError::BadInput(format!(
                        "imp.ext missing for imp {}",
                        imp.id
                    )));
                    continue;
                }
            };

            let bidder_val = match ext.get("bidder") {
                Some(v) => v.clone(),
                None => {
                    errors.push(BidderError::BadInput(format!(
                        "imp.ext.bidder missing for imp {}",
                        imp.id
                    )));
                    continue;
                }
            };

            let luna_ext: LunamediaImpExt = match serde_json::from_value(bidder_val) {
                Ok(v) => v,
                Err(e) => {
                    errors.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            if luna_ext.pub_id.is_empty() {
                errors.push(BidderError::BadInput("No pubid value provided".to_string()));
                continue;
            }

            // Clear imp.ext and handle banner w/h
            let mut imp_copy = imp.clone();
            imp_copy.ext = None;
            imp_copy.tagid = Some(luna_ext.placement.clone());

            if let Some(banner) = &imp.banner {
                let mut banner_copy = banner.clone();
                if banner_copy.w.is_none() || banner_copy.h.is_none() {
                    if banner_copy.format.is_empty() {
                        errors.push(BidderError::BadInput(
                            "Expected at least one banner.format entry or explicit w/h"
                                .to_string(),
                        ));
                        continue;
                    }
                    let fmt = banner_copy.format.remove(0);
                    banner_copy.w = Some(fmt.w);
                    banner_copy.h = Some(fmt.h);
                }
                imp_copy.banner = Some(banner_copy);
            }

            groups.entry(luna_ext).or_default().push(imp_copy);
        }

        if groups.is_empty() {
            return (vec![], errors);
        }

        let mut requests = Vec::new();

        for (params, imps) in groups {
            let mut req_copy = request.clone();
            // Clear site publisher and domain, clear app publisher
            if let Some(site) = req_copy.site.as_mut() {
                site.publisher = None;
                site.domain = None;
            }
            if let Some(app) = req_copy.app.as_mut() {
                app.publisher = None;
            }
            req_copy.imp = imps;

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            // Build URL by substituting {{.PublisherID}} macro
            let uri = self.endpoint.replace("{{.PublisherID}}", &params.pub_id);

            let mut headers = HashMap::new();
            headers.insert(
                "Content-Type".to_string(),
                "application/json;charset=utf-8".to_string(),
            );
            headers.insert("Accept".to_string(), "application/json".to_string());
            headers.insert("x-openrtb-version".to_string(), "2.5".to_string());

            requests.push(RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers,
                imp_ids: get_imp_ids(&req_copy.imp),
            });
        }

        (requests, errors)
    }

    fn make_bids(
        &self,
        internal: &openrtb::BidRequest,
        _external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected http status code: {}",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| {
                vec![BidderError::BadServerResponse(format!(
                    "Bad server response: {}",
                    e
                ))]
            })?;

        if bid_resp.seatbid.len() != 1 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Invalid SeatBids count: {}",
                bid_resp.seatbid.len()
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
