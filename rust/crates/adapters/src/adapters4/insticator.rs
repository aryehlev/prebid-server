use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::{BidType, ExtBidPrebidMeta, ExtBidPrebidVideo};
use serde::{Deserialize, Serialize};

pub struct InsticatorAdapter { pub endpoint: String }
impl InsticatorAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ExtImpInsticator {
    #[serde(default)]
    ad_unit_id: String,
    #[serde(default)]
    publisher_id: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ImpInsticatorExt {
    insticator: ImpInsticatorExtData,
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ImpInsticatorExtData {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    ad_unit_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    publisher_id: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ReqInsticatorExt {
    insticator: Option<ReqInsticatorExtData>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ReqInsticatorExtData {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    caller: Vec<InsticatorCaller>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
struct InsticatorCaller {
    name: String,
    version: String,
}

fn get_media_type_for_bid(mtype: Option<i32>) -> BidType {
    match mtype.unwrap_or(0) {
        1 => BidType::Banner,
        2 => BidType::Video,
        _ => BidType::Banner,
    }
}

fn make_req_ext(request: &openrtb::BidRequest) -> Result<serde_json::Value, BidderError> {
    let mut req_ext: ReqInsticatorExt = if let Some(ext) = &request.ext {
        serde_json::from_value(ext.clone()).unwrap_or_default()
    } else {
        ReqInsticatorExt::default()
    };

    let caller = InsticatorCaller {
        name: "Prebid-Server".to_string(),
        version: "n/a".to_string(),
    };

    if req_ext.insticator.is_none() {
        req_ext.insticator = Some(ReqInsticatorExtData::default());
    }
    if let Some(inst) = req_ext.insticator.as_mut() {
        inst.caller.push(caller);
    }

    serde_json::to_value(&req_ext).map_err(|e| BidderError::BadInput(e.to_string()))
}

impl Bidder for InsticatorAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut requests = Vec::new();

        // Build request ext with caller info
        let req_ext = match make_req_ext(request) {
            Ok(e) => e,
            Err(e) => { errs.push(e); return (vec![], errs); }
        };

        let mut req_copy = request.clone();
        req_copy.ext = Some(req_ext);

        // Group imps by ad_unit_id
        let mut grouped: HashMap<String, (Vec<openrtb::Imp>, String)> = HashMap::new();
        let mut publisher_id = String::new();

        for imp in &request.imp {
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput("missing bidder ext".to_string()));
                    continue;
                }
            };
            let inst_ext: ExtImpInsticator = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            if publisher_id.is_empty() {
                publisher_id = inst_ext.publisher_id.clone();
            }

            // Validate video if present
            if let Some(video) = &imp.video {
                if video.w.is_none() || video.w == Some(0) || video.h.is_none() || video.h == Some(0) || video.mimes.as_deref().unwrap_or(&[]).is_empty() {
                    errs.push(BidderError::BadInput("One or more invalid or missing video field(s) w, h, mimes".to_string()));
                    continue;
                }
            }

            // Build new imp ext
            let new_imp_ext = ImpInsticatorExt {
                insticator: ImpInsticatorExtData {
                    ad_unit_id: inst_ext.ad_unit_id.clone(),
                    publisher_id: inst_ext.publisher_id.clone(),
                },
            };
            let imp_ext_val = match serde_json::to_value(&new_imp_ext) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(imp_ext_val);

            let entry = grouped.entry(inst_ext.ad_unit_id.clone()).or_insert_with(|| (Vec::new(), inst_ext.publisher_id.clone()));
            entry.0.push(imp_copy);
        }

        // Populate publisher.id in site/app
        if !publisher_id.is_empty() {
            if let Some(site) = req_copy.site.as_mut() {
                if site.publisher.is_none() {
                    site.publisher = Some(openrtb::Publisher::default());
                }
                if let Some(pub_) = site.publisher.as_mut() {
                    pub_.id = Some(publisher_id.clone());
                }
            }
            if let Some(app) = req_copy.app.as_mut() {
                if app.publisher.is_none() {
                    app.publisher = Some(openrtb::Publisher::default());
                }
                if let Some(pub_) = app.publisher.as_mut() {
                    pub_.id = Some(publisher_id.clone());
                }
            }
        }

        for (_, (imps, pub_id)) in grouped {
            let mut this_req = req_copy.clone();
            this_req.imp = imps;

            let body = match serde_json::to_vec(&this_req) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            // Build URL with publisherId query param
            let uri = if !pub_id.is_empty() {
                if self.endpoint.contains('?') {
                    format!("{}&publisherId={}", self.endpoint, pub_id)
                } else {
                    format!("{}?publisherId={}", self.endpoint, pub_id)
                }
            } else {
                self.endpoint.clone()
            };

            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());

            if let Some(device) = &request.device {
                if let Some(ua) = &device.ua { if !ua.is_empty() { headers.insert("User-Agent".to_string(), ua.clone()); } }
                if let Some(ip) = &device.ip { if !ip.is_empty() { headers.insert("X-Forwarded-For".to_string(), ip.clone()); headers.insert("IP".to_string(), ip.clone()); } }
                else if let Some(ipv6) = &device.ipv6 { if !ipv6.is_empty() { headers.insert("X-Forwarded-For".to_string(), ipv6.clone()); } }
            }

            let imp_ids = this_req.imp.iter().map(|i| i.id.clone()).collect();
            requests.push(RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers,
                imp_ids,
            });
        }

        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() { result.currency = cur.clone(); }
        }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_media_type_for_bid(bid.mtype);
                let bid_meta = get_bid_meta(&bid, bid_type.clone());
                let bid_video = get_bid_video(&bid, bid_type.clone());
                let mut typed = TypedBid::new(bid, bid_type);
                typed.bid_meta = Some(bid_meta);
                typed.bid_video = bid_video;
                result.bids.push(typed);
            }
        }
        Ok(result)
    }
}

/// getBidMeta extracts metadata from the bid for brand safety and reporting.
fn get_bid_meta(bid: &openrtb::Bid, bid_type: BidType) -> ExtBidPrebidMeta {
    let mut meta = ExtBidPrebidMeta {
        media_type: Some(bid_type.to_string()),
        ..Default::default()
    };

    if let Some(adomain) = &bid.adomain {
        if !adomain.is_empty() {
            meta.advertiser_domains = Some(adomain.clone());
        }
    }

    if let Some(cat) = &bid.cat {
        if !cat.is_empty() {
            meta.primary_category_id = Some(cat[0].clone());
            if cat.len() > 1 {
                meta.secondary_category_ids = Some(cat[1..].to_vec());
            }
        }
    }

    meta
}

/// getBidVideo extracts video-specific metadata from the bid.
fn get_bid_video(bid: &openrtb::Bid, bid_type: BidType) -> Option<ExtBidPrebidVideo> {
    if bid_type != BidType::Video {
        return None;
    }

    let primary_category = bid.cat.as_deref()
        .and_then(|c| c.first())
        .cloned()
        .unwrap_or_default();
    // Note: Rust openrtb::Bid does not expose a `dur` field; default to 0.
    Some(ExtBidPrebidVideo {
        duration: 0,
        primary_category,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_requests_grouped_by_adunit() {
        let adapter = InsticatorAdapter::new("https://insticator.example/rtb".to_string());
        let mut req = openrtb::BidRequest::default();
        req.id = "r".to_string();
        req.imp = vec![openrtb::Imp {
            id: "imp1".to_string(),
            banner: Some(Default::default()),
            ext: Some(serde_json::json!({"bidder": {"adUnitId":"au1","publisherId":"pub1"}})),
            ..Default::default()
        }];
        let info = ExtraRequestInfo::default();
        let (requests, errs) = adapter.make_requests(&req, &info);
        assert!(errs.is_empty());
        assert_eq!(requests.len(), 1);
        assert!(requests[0].uri.contains("publisherId=pub1"));
    }
}
