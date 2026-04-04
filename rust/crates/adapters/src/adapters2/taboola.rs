use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct TaboolaAdapter { pub endpoint: String }
impl TaboolaAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Default, Deserialize)]
struct ExtImpTaboola {
    #[serde(rename = "publisherId", default)]
    publisher_id: String,
    #[serde(rename = "publisherDomain", default)]
    publisher_domain: String,
    #[serde(rename = "tagId", default)]
    tag_id: String,
    #[serde(rename = "tagid", default)]
    tagid: String,
    /// bidFloor can appear as "bidFloor" or "bidfloor" in publisher ext
    #[serde(rename = "bidFloor", alias = "bidfloor", default)]
    bid_floor: f64,
    #[serde(rename = "bcat", default)]
    bcat: Option<Vec<String>>,
    #[serde(rename = "badv", default)]
    badv: Option<Vec<String>>,
    #[serde(rename = "pageType", default)]
    page_type: String,
    #[serde(rename = "position", default)]
    position: Option<i32>,
}

/// Request-level ext for pageType, matching Go's RequestExt struct.
#[derive(Debug, Serialize)]
struct TaboolaRequestExt {
    #[serde(rename = "pageType")]
    page_type: String,
}

fn get_media_type(imp_id: &str, imps: &[openrtb::Imp]) -> Option<BidType> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return Some(BidType::Banner);
            } else if imp.native.is_some() {
                return Some(BidType::Native);
            }
        }
    }
    None
}

impl Bidder for TaboolaAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req = request.clone();
        let mut errs = Vec::new();
        let mut taboola_ext = ExtImpTaboola::default();

        // Collect banner and native imps separately
        let mut banner_imps: Vec<openrtb::Imp> = Vec::new();
        let mut native_imps: Vec<openrtb::Imp> = Vec::new();

        for imp in req.imp.iter_mut() {
            let bidder_val = match imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .cloned()
            {
                Some(v) => v,
                None => continue,
            };
            let ext: ExtImpTaboola = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let tag_id = if !ext.tag_id.is_empty() { ext.tag_id.clone() } else { ext.tagid.clone() };
            if !tag_id.is_empty() {
                imp.tagid = Some(tag_id);
            }
            if ext.bid_floor != 0.0 {
                imp.bidfloor = Some(ext.bid_floor);
            }

            // Set banner position if provided
            if let Some(pos) = ext.position {
                if let Some(banner) = imp.banner.as_mut() {
                    banner.pos = Some(pos);
                }
            }

            taboola_ext = ext;

            if imp.banner.is_some() {
                banner_imps.push(imp.clone());
            } else if imp.native.is_some() {
                native_imps.push(imp.clone());
            }
        }

        // Set publisher info
        let publisher_id = taboola_ext.publisher_id.clone();
        let publisher = openrtb::Publisher {
            id: if publisher_id.is_empty() { None } else { Some(publisher_id.clone()) },
            ..Default::default()
        };

        // evaluate_domain: use publisherDomain from ext, else fall back to site.domain
        let publisher_domain = if !taboola_ext.publisher_domain.is_empty() {
            taboola_ext.publisher_domain.clone()
        } else {
            request.site.as_ref().and_then(|s| s.domain.clone()).unwrap_or_default()
        };

        if let Some(site) = req.site.as_mut() {
            if !publisher_id.is_empty() {
                site.id = Some(publisher_id.clone());
                site.name = Some(publisher_id.clone());
            }
            if !publisher_domain.is_empty() {
                site.domain = Some(publisher_domain);
            }
            site.publisher = Some(publisher.clone());
        }
        if let Some(app) = req.app.as_mut() {
            if !publisher_id.is_empty() {
                app.id = Some(publisher_id.clone());
            }
            app.publisher = Some(publisher.clone());
        }

        if let Some(bcat) = taboola_ext.bcat {
            req.bcat = Some(bcat);
        }
        if let Some(badv) = taboola_ext.badv {
            req.badv = Some(badv);
        }

        // Set request.ext with pageType if present
        if !taboola_ext.page_type.is_empty() {
            match serde_json::to_value(&TaboolaRequestExt { page_type: taboola_ext.page_type.clone() }) {
                Ok(v) => req.ext = Some(v),
                Err(e) => errs.push(BidderError::BadInput(e.to_string())),
            }
        }

        let mut requests = Vec::new();

        // Build request helper
        let build_req = |imps: Vec<openrtb::Imp>, media_type: &str, base_req: &openrtb::BidRequest, endpoint: &str| -> Option<RequestData> {
            if imps.is_empty() {
                return None;
            }
            let mut r = base_req.clone();
            r.imp = imps;

            // Extract publisher id from site or app
            let pub_id = r.site.as_ref().and_then(|s| s.id.clone())
                .or_else(|| r.app.as_ref().and_then(|a| a.id.clone()))
                .unwrap_or_default();

            // Build URL: replace {{.PublisherID}} and {{.MediaType}} macros
            let url = endpoint
                .replace("{{.PublisherID}}", &pub_id)
                .replace("{{.MediaType}}", media_type)
                .replace("{{.GvlID}}", "");

            let body = serde_json::to_vec(&r).ok()?;
            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());
            let imp_ids = get_imp_ids(&r.imp);
            Some(RequestData { method: "POST".to_string(), uri: url, body, headers, imp_ids })
        };

        if let Some(rd) = build_req(native_imps, "native", &req, &self.endpoint) {
            requests.push(rd);
        }
        if let Some(rd) = build_req(banner_imps, "display", &req, &self.endpoint) {
            requests.push(rd);
        }

        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = bid_resp.cur {
            result.currency = cur;
        }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                // Resolve AUCTION_PRICE macro
                let price_str = format!("{}", bid.price);
                if let Some(ref nurl) = bid.nurl {
                    bid.nurl = Some(nurl.replace("${AUCTION_PRICE}", &price_str));
                }
                if let Some(ref adm) = bid.adm {
                    bid.adm = Some(adm.replace("${AUCTION_PRICE}", &price_str));
                }
                match get_media_type(&bid.impid, &internal.imp) {
                    Some(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    None => errs.push(BidderError::BadInput(format!(
                        "Failed to find banner/native impression \"{}\"", bid.impid
                    ))),
                }
            }
        }
        if !errs.is_empty() && result.bids.is_empty() {
            return Err(errs);
        }
        Ok(result)
    }
}
