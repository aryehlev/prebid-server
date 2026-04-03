use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct TaboolaAdapter {
    pub endpoint: String,
}

impl TaboolaAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }

    fn build_url(&self, publisher_id: &str, media_type: &str) -> String {
        // Replace template macros {{.PublisherID}} and {{.MediaType}} in endpoint
        self.endpoint
            .replace("{{.PublisherID}}", publisher_id)
            .replace("{{.MediaType}}", media_type)
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return Ok(BidType::Banner);
            } else if imp.native.is_some() {
                return Ok(BidType::Native);
            }
        }
    }
    Err(BidderError::BadInput(format!(
        "Failed to find banner/native impression \"{}\"",
        imp_id
    )))
}

impl Bidder for TaboolaAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut native_imps: Vec<openrtb::Imp> = Vec::new();
        let mut banner_imps: Vec<openrtb::Imp> = Vec::new();
        let mut taboola_publisher_id = String::new();

        let mut modified_request = request.clone();

        for (i, imp) in request.imp.iter().enumerate() {
            let ext = match imp.ext.as_ref() {
                Some(e) => e,
                None => {
                    errs.push(BidderError::BadInput("missing imp ext".to_string()));
                    continue;
                }
            };

            let bidder_ext = match ext.get("bidder") {
                Some(b) => b,
                None => {
                    errs.push(BidderError::BadInput("missing bidder ext".to_string()));
                    continue;
                }
            };

            let tag_id = bidder_ext.get("tagId")
                .or_else(|| bidder_ext.get("tagid"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let bid_floor = bidder_ext.get("bidFloor")
                .and_then(|v| v.as_f64());

            let publisher_id = bidder_ext.get("publisherId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if taboola_publisher_id.is_empty() && !publisher_id.is_empty() {
                taboola_publisher_id = publisher_id.clone();
            }

            let mut modified_imp = imp.clone();
            if !tag_id.is_empty() {
                modified_imp.tagid = Some(tag_id);
            }
            if let Some(floor) = bid_floor {
                if floor != 0.0 {
                    modified_imp.bidfloor = Some(floor);
                }
            }

            if modified_imp.banner.is_some() {
                banner_imps.push(modified_imp);
            } else if modified_imp.native.is_some() {
                native_imps.push(modified_imp);
            }
        }

        // Set publisher info on site/app
        if !taboola_publisher_id.is_empty() {
            if let Some(site) = modified_request.site.as_mut() {
                site.id = Some(taboola_publisher_id.clone());
                // name field
            }
            if let Some(app) = modified_request.app.as_mut() {
                app.id = Some(taboola_publisher_id.clone());
            }
        }

        let publisher_id_for_url = modified_request.site.as_ref()
            .and_then(|s| s.id.as_deref())
            .or_else(|| modified_request.app.as_ref().and_then(|a| a.id.as_deref()))
            .unwrap_or("");

        let mut requests = Vec::new();

        // Build request for native imps
        if !native_imps.is_empty() {
            let mut req = modified_request.clone();
            req.imp = native_imps;
            let url = self.build_url(publisher_id_for_url, "native");
            match serde_json::to_vec(&req) {
                Ok(body) => {
                    requests.push(RequestData {
                        method: "POST".to_string(),
                        uri: url,
                        body,
                        headers: HashMap::new(),
                        imp_ids: get_imp_ids(&req.imp),
                    });
                }
                Err(e) => errs.push(BidderError::BadInput(e.to_string())),
            }
        }

        // Build request for banner imps
        if !banner_imps.is_empty() {
            let mut req = modified_request.clone();
            req.imp = banner_imps;
            let url = self.build_url(publisher_id_for_url, "display");
            match serde_json::to_vec(&req) {
                Ok(body) => {
                    requests.push(RequestData {
                        method: "POST".to_string(),
                        uri: url,
                        body,
                        headers: HashMap::new(),
                        imp_ids: get_imp_ids(&req.imp),
                    });
                }
                Err(e) => errs.push(BidderError::BadInput(e.to_string())),
            }
        }

        (requests, errs)
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
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_response: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = bid_response.cur.as_deref() {
            if !cur.is_empty() {
                result.currency = cur.to_string();
            }
        }

        let mut errs = Vec::new();
        for sb in bid_response.seatbid {
            for mut bid in sb.bid {
                // Resolve macros
                let price_str = format!("{}", bid.price);
                if let Some(nurl) = bid.nurl.as_mut() {
                    *nurl = nurl.replace("${AUCTION_PRICE}", &price_str);
                }
                if let Some(adm) = bid.adm.as_mut() {
                    *adm = adm.replace("${AUCTION_PRICE}", &price_str);
                }

                match get_media_type_for_imp(&bid.impid, &internal.imp) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    Err(e) => errs.push(e),
                }
            }
        }

        if errs.is_empty() {
            Ok(result)
        } else {
            Ok(result)
        }
    }
}
