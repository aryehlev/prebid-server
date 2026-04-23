use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct GumgumAdapter { pub endpoint: String }
impl GumgumAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Default, Deserialize)]
struct ExtImpGumGum {
    #[serde(default)]
    zone: String,
    #[serde(rename = "pubId", default)]
    pub_id: f64,
    #[serde(rename = "irisid", default)]
    iris_id: String,
    #[serde(default)]
    slot: f64,
    #[serde(default)]
    product: String,
}

#[derive(Debug, Serialize)]
struct ExtImpGumGumVideo {
    #[serde(rename = "irisid")]
    iris_id: String,
}

#[derive(Debug, Serialize)]
struct ExtImpGumGumBanner {
    #[serde(rename = "si")]
    si: f64,
    #[serde(rename = "maxw")]
    max_w: f64,
    #[serde(rename = "maxh")]
    max_h: f64,
}

fn get_bigger_format(formats: &[openrtb::Format]) -> (i32, i32) {
    let mut max_w = 0i32;
    let mut max_h = 0i32;
    let mut greatest_val = 0i32;
    for fmt in formats {
        let w = fmt.w.unwrap_or(0);
        let h = fmt.h.unwrap_or(0);
        let bigger_side = if w > h { w } else { h };
        if bigger_side > greatest_val || (bigger_side == greatest_val && w >= max_w && h >= max_h) {
            greatest_val = bigger_side;
            max_w = w;
            max_h = h;
        }
    }
    (max_w, max_h)
}

/// Preprocess an imp: set tagid from adunitcode, fix banner W/H, set banner/video ext,
/// set product in imp.ext. Returns the gumgum ext or an error.
fn preprocess(imp: &mut openrtb::Imp) -> Result<ExtImpGumGum, BidderError> {
    let bidder_val = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .cloned()
        .ok_or_else(|| BidderError::BadInput(format!("imp {} missing bidder ext", imp.id)))?;

    let gumgum_ext: ExtImpGumGum = serde_json::from_value(bidder_val)
        .map_err(|e| BidderError::BadInput(e.to_string()))?;

    // Set tagid from ext.prebid.adunitcode if present
    if let Some(ext) = &imp.ext {
        if let Some(prebid) = ext.get("prebid") {
            if let Some(auc) = prebid.get("adunitcode").and_then(|v| v.as_str()) {
                if !auc.is_empty() {
                    imp.tagid = Some(auc.to_string());
                }
            }
        }
    }

    // Fix banner: set W/H from first format if both are absent
    if let Some(banner) = imp.banner.as_mut() {
        if banner.w.is_none() && banner.h.is_none() {
            if let Some(formats) = banner.format.as_deref() {
                if !formats.is_empty() {
                    let first = &formats[0];
                    banner.w = first.w;
                    banner.h = first.h;

                    // Set banner ext with slot info if slot != 0
                    if gumgum_ext.slot != 0.0 {
                        let (max_w, max_h) = get_bigger_format(formats);
                        let banner_ext = ExtImpGumGumBanner {
                            si: gumgum_ext.slot,
                            max_w: max_w as f64,
                            max_h: max_h as f64,
                        };
                        banner.ext = serde_json::to_value(&banner_ext).ok();
                    }
                }
            }
        }
    }

    // Set video ext with irisid if present
    if imp.video.is_some() && !gumgum_ext.iris_id.is_empty() {
        let video_ext = ExtImpGumGumVideo { iris_id: gumgum_ext.iris_id.clone() };
        if let Some(video) = imp.video.as_mut() {
            video.ext = serde_json::to_value(&video_ext).ok();
        }
    }

    // Override imp.ext with product if present
    if !gumgum_ext.product.is_empty() {
        let mut product_map = serde_json::Map::new();
        product_map.insert("product".to_string(), serde_json::Value::String(gumgum_ext.product.clone()));
        imp.ext = Some(serde_json::Value::Object(product_map));
    }

    Ok(gumgum_ext)
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
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut valid_imps: Vec<openrtb::Imp> = Vec::new();
        let mut site_id: Option<String> = None;
        let mut publisher_id: Option<String> = None;

        for imp in &request.imp {
            let mut imp = imp.clone();
            match preprocess(&mut imp) {
                Err(e) => {
                    errs.push(e);
                    continue;
                }
                Ok(gumgum_ext) => {
                    if !gumgum_ext.zone.is_empty() {
                        site_id = Some(gumgum_ext.zone.clone());
                    }
                    if gumgum_ext.pub_id != 0.0 {
                        let s = if gumgum_ext.pub_id.fract() == 0.0 {
                            format!("{}", gumgum_ext.pub_id as i64)
                        } else {
                            gumgum_ext.pub_id.to_string()
                        };
                        publisher_id = Some(s);
                    }
                    valid_imps.push(imp);
                }
            }
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        let mut req = request.clone();
        req.imp = valid_imps;

        // Update site with zone and publisher
        if req.site.is_some() {
            let site = req.site.as_mut().unwrap();
            if let Some(sid) = site_id {
                site.id = Some(sid);
            }
            if let Some(pid) = publisher_id {
                let pub_obj = site.publisher.get_or_insert_with(Default::default);
                pub_obj.id = Some(pid);
            }
        }

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        (vec![RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers, imp_ids: get_imp_ids(&req.imp) }], errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = bid_resp.cur {
            if !cur.is_empty() {
                result.currency = cur;
            }
        }
        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                let bid_type = get_media_type_for_imp_id(&bid.impid, &internal.imp);
                // For video bids, substitute ${AUCTION_PRICE} macro with actual price
                if bid_type == BidType::Video {
                    let price_str = if bid.price.fract() == 0.0 {
                        format!("{}", bid.price as i64)
                    } else {
                        bid.price.to_string()
                    };
                    if let Some(adm) = bid.adm.take() {
                        bid.adm = Some(adm.replace("${AUCTION_PRICE}", &price_str));
                    }
                }
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_req() -> openrtb::BidRequest {
        openrtb::BidRequest {
            id: "r".to_string(),
            imp: vec![openrtb::Imp {
                id: "i1".to_string(),
                banner: Some(openrtb::Banner {
                    w: Some(300),
                    h: Some(250),
                    ..Default::default()
                }),
                ext: Some(serde_json::json!({"bidder": {"zone": "dc", "pubId": 12345}})),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_gumgum_url_and_headers() {
        let adapter = GumgumAdapter::new("https://g2.gumgum.com/hbid/imp".to_string());
        let (reqs, _) = adapter.make_requests(&make_req(), &ExtraRequestInfo::default());
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].uri, "https://g2.gumgum.com/hbid/imp");
        assert_eq!(
            reqs[0].headers.get("Content-Type").unwrap(),
            "application/json;charset=utf-8"
        );
        assert!(!reqs[0].body.is_empty());
    }

    #[test]
    fn test_gumgum_make_bids() {
        let adapter = GumgumAdapter::new("https://g2.gumgum.com/hbid/imp".to_string());
        let body = br#"{"id":"r","seatbid":[{"bid":[{"id":"b","impid":"i1","price":2.2,"crid":"c"}]}]}"#;
        let resp = ResponseData::new(200, body.to_vec());
        let result = adapter
            .make_bids(&make_req(), &RequestData::default(), &resp)
            .unwrap();
        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].bid_type, BidType::Banner);
    }
}
