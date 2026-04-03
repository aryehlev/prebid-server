use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct GumgumAdapter {
    pub endpoint: String,
}

impl GumgumAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

fn get_media_type_for_imp_id(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id && imp.banner.is_some() {
            return BidType::Banner;
        }
    }
    BidType::Video
}

impl Bidder for GumgumAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut valid_imps = Vec::new();
        let mut site_copy = request.site.as_ref().cloned().unwrap_or_default();

        for imp in &request.imp {
            let bidder_ext = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(b) => b.clone(),
                None => {
                    errs.push(BidderError::BadInput("missing bidder ext".to_string()));
                    continue;
                }
            };

            let zone = bidder_ext.get("zone").and_then(|v| v.as_str()).unwrap_or("");
            let pub_id = bidder_ext.get("pubId").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let slot = bidder_ext.get("slot").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let iris_id = bidder_ext.get("irisId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let product = bidder_ext.get("product").and_then(|v| v.as_str()).unwrap_or("").to_string();

            if !zone.is_empty() {
                site_copy.id = Some(zone.to_string());
            }

            if pub_id != 0.0 {
                let pub_id_str = format_float(pub_id);
                if let Some(publisher) = site_copy.publisher.as_mut() {
                    publisher.id = Some(pub_id_str);
                } else {
                    site_copy.publisher = Some(openrtb::Publisher {
                        id: Some(pub_id_str),
                        ..Default::default()
                    });
                }
            }

            let mut imp = imp.clone();

            // Set tag_id from prebid.adUnitCode if present
            if let Some(ext_val) = imp.ext.as_ref() {
                if let Some(ad_unit_code) = ext_val
                    .get("prebid")
                    .and_then(|p| p.get("adUnitCode"))
                    .and_then(|v| v.as_str())
                {
                    if !ad_unit_code.is_empty() {
                        imp.tagid = Some(ad_unit_code.to_string());
                    }
                }
            }

            // Modify banner
            if let Some(banner) = imp.banner.as_mut() {
                if banner.w.map(|w| w == 0).unwrap_or(true)
                    && banner.h.map(|h| h == 0).unwrap_or(true)
                    && !banner.format.is_empty()
                {
                    let first = banner.format[0].clone();
                    banner.w = Some(first.w);
                    banner.h = Some(first.h);

                    if slot != 0.0 {
                        // Find biggest format
                        let (max_w, max_h) = get_bigger_format(&banner.format, slot);
                        let banner_ext = serde_json::json!({
                            "si": slot,
                            "maxW": max_w,
                            "maxH": max_h,
                        });
                        banner.ext = Some(banner_ext);
                    }
                }
            }

            // Modify video with iris_id
            if let Some(video) = imp.video.as_mut() {
                if !iris_id.is_empty() {
                    video.ext = Some(serde_json::json!({"irisId": iris_id}));
                }
            }

            // Override imp ext with product if set
            if !product.is_empty() {
                imp.ext = Some(serde_json::json!({"product": product}));
            }

            valid_imps.push(imp);
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        let mut req = request.clone();
        req.imp = valid_imps;
        if request.site.is_some() {
            req.site = Some(site_copy);
        }

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::BadInput(e.to_string()));
                return (vec![], errs);
            }
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let imp_ids = get_imp_ids(&req.imp);
        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids,
            }],
            errs,
        )
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
            .map_err(|e| vec![BidderError::BadServerResponse(format!("Bad server response: {}. ", e))])?;

        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = bid_response.cur.as_deref() {
            if !cur.is_empty() {
                result.currency = cur.to_string();
            }
        }

        for sb in bid_response.seatbid {
            for mut bid in sb.bid {
                let media_type = get_media_type_for_imp_id(&bid.impid, &internal.imp);
                if media_type == BidType::Video {
                    let price_str = format!("{}", bid.price);
                    if let Some(adm) = bid.adm.as_mut() {
                        *adm = adm.replace("${AUCTION_PRICE}", &price_str);
                    }
                }
                result.bids.push(TypedBid::new(bid, media_type));
            }
        }

        Ok(result)
    }
}

fn format_float(f: f64) -> String {
    // Format without trailing zeros like Go's strconv.FormatFloat
    let s = format!("{}", f);
    s
}

fn get_bigger_format(formats: &[openrtb::Format], _slot: f64) -> (i64, i64) {
    let mut max_w: i64 = 0;
    let mut max_h: i64 = 0;
    let mut greatest: i64 = 0;
    for fmt in formats {
        let bigger_side = fmt.w.max(fmt.h);
        if bigger_side > greatest
            || (bigger_side == greatest && fmt.w >= max_w && fmt.h >= max_h)
        {
            greatest = bigger_side;
            max_w = fmt.w;
            max_h = fmt.h;
        }
    }
    (max_w, max_h)
}
