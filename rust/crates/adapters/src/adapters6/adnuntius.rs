use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub struct AdnuntiusAdapter {
    pub endpoint: String,
}
impl AdnuntiusAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

// ---- Adnuntius custom request types ----

#[derive(Serialize)]
struct AdnRequest {
    #[serde(rename = "adUnits")]
    ad_units: Vec<AdnRequestAdUnit>,
    #[serde(rename = "metaData", skip_serializing_if = "Option::is_none")]
    meta_data: Option<AdnMetaData>,
    #[serde(rename = "context", skip_serializing_if = "Option::is_none")]
    context: Option<String>,
    #[serde(rename = "kv", skip_serializing_if = "Option::is_none")]
    key_values: Option<Value>,
}

#[derive(Serialize)]
struct AdnMetaData {
    #[serde(rename = "usi", skip_serializing_if = "Option::is_none")]
    usi: Option<String>,
}

#[derive(Serialize)]
struct AdnRequestAdUnit {
    #[serde(rename = "auId")]
    au_id: String,
    #[serde(rename = "targetId")]
    target_id: String,
    #[serde(rename = "adType", skip_serializing_if = "Option::is_none")]
    ad_type: Option<String>,
    #[serde(rename = "dimensions", skip_serializing_if = "Option::is_none")]
    dimensions: Option<Vec<[i64; 2]>>,
    #[serde(rename = "maxDeals", skip_serializing_if = "Option::is_none")]
    max_deals: Option<i64>,
    #[serde(rename = "c", skip_serializing_if = "Option::is_none")]
    category: Option<Vec<String>>,
    #[serde(rename = "segments", skip_serializing_if = "Option::is_none")]
    segments: Option<Vec<String>>,
    #[serde(rename = "keywords", skip_serializing_if = "Option::is_none")]
    keywords: Option<Vec<String>>,
    #[serde(rename = "kv", skip_serializing_if = "Option::is_none")]
    key_values: Option<HashMap<String, Vec<String>>>,
    #[serde(rename = "auml", skip_serializing_if = "Option::is_none")]
    ad_unit_matching_label: Option<Vec<String>>,
}

// ---- Adnuntius custom response types ----

#[derive(Deserialize)]
struct AdnResponse {
    #[serde(rename = "adUnits", default)]
    ad_units: Vec<AdnAdUnit>,
}

#[derive(Deserialize)]
struct AdnAdUnit {
    #[serde(rename = "targetId")]
    target_id: String,
    #[serde(rename = "html", default)]
    html: String,
    #[serde(rename = "matchedAdCount", default)]
    matched_ad_count: i32,
    #[serde(rename = "ads", default)]
    ads: Vec<AdnAd>,
    #[serde(rename = "deals", default)]
    deals: Vec<AdnAd>,
    #[serde(rename = "nativeJson")]
    native_json: Option<Value>,
}

#[derive(Deserialize)]
struct AdnAd {
    #[serde(rename = "bid")]
    bid: AdnBidInfo,
    #[serde(rename = "netBid")]
    net_bid: Option<AdnSimpleAmount>,
    #[serde(rename = "grossBid")]
    gross_bid: Option<AdnSimpleAmount>,
    #[serde(rename = "adId", default)]
    ad_id: String,
    #[serde(rename = "creativeWidth", default)]
    creative_width: String,
    #[serde(rename = "creativeHeight", default)]
    creative_height: String,
    #[serde(rename = "creativeId", default)]
    creative_id: String,
    #[serde(rename = "lineItemId", default)]
    line_item_id: String,
    #[serde(rename = "html", default)]
    html: String,
    #[serde(rename = "dealId", default)]
    deal_id: String,
    #[serde(rename = "advertiserDomains", default)]
    advertiser_domains: Vec<String>,
}

#[derive(Deserialize)]
struct AdnBidInfo {
    #[serde(rename = "amount", default)]
    amount: f64,
    #[serde(rename = "currency", default)]
    currency: String,
}

#[derive(Deserialize)]
struct AdnSimpleAmount {
    #[serde(rename = "amount", default)]
    amount: f64,
}

// ---- Bidder extension types ----

#[derive(Deserialize)]
struct ExtImpBidder {
    bidder: Value,
}

#[allow(dead_code)]
#[derive(Deserialize, Default)]
struct ImpExtAdnuntius {
    #[serde(rename = "auId", default)]
    au_id: String,
    #[serde(rename = "noCookies", default)]
    no_cookies: bool,
    #[serde(rename = "maxDeals", default)]
    max_deals: i64,
    #[serde(rename = "network", default)]
    network: String,
    #[serde(rename = "bidType", default)]
    bid_type: String,
    #[serde(rename = "targeting", default)]
    targeting: AdnTargeting,
}

#[derive(Deserialize, Default)]
struct AdnTargeting {
    #[serde(rename = "c", default)]
    category: Vec<String>,
    #[serde(rename = "segments", default)]
    segments: Vec<String>,
    #[serde(rename = "keywords", default)]
    keywords: Vec<String>,
    #[serde(rename = "kv", default)]
    key_values: HashMap<String, Vec<String>>,
    #[serde(rename = "auml", default)]
    ad_unit_matching_label: Vec<String>,
}

const DEFAULT_NETWORK: &str = "default";

fn parse_bidder_ext(imp: &openrtb::Imp) -> Result<ImpExtAdnuntius, BidderError> {
    let ext = imp
        .ext
        .as_ref()
        .ok_or_else(|| BidderError::BadInput(format!("imp {} missing ext", imp.id)))?;
    let bidder_ext: ExtImpBidder = serde_json::from_value(ext.clone())
        .map_err(|e| BidderError::BadInput(format!("Error unmarshalling ExtImpBidder: {e}")))?;
    let adnuntius_ext: ImpExtAdnuntius = serde_json::from_value(bidder_ext.bidder)
        .map_err(|e| BidderError::BadInput(format!("Error unmarshalling ExtImpValues: {e}")))?;
    Ok(adnuntius_ext)
}

fn get_imp_sizes(imp: &openrtb::Imp) -> Option<Vec<[i64; 2]>> {
    if let Some(banner) = &imp.banner {
        if let Some(formats) = &banner.format {
            if !formats.is_empty() {
                return Some(
                    formats
                        .iter()
                        .map(|f| [f.w.unwrap_or(0) as i64, f.h.unwrap_or(0) as i64])
                        .collect(),
                );
            }
        }
        if let (Some(w), Some(h)) = (banner.w, banner.h) {
            return Some(vec![[w as i64, h as i64]]);
        }
    }
    None
}

fn build_ad_unit(imp: &openrtb::Imp, ext: &ImpExtAdnuntius, bid_type: &str) -> AdnRequestAdUnit {
    AdnRequestAdUnit {
        au_id: ext.au_id.clone(),
        target_id: format!("{}-{}:{}", ext.au_id, imp.id, bid_type),
        ad_type: if bid_type == "native" {
            Some("NATIVE".to_string())
        } else {
            None
        },
        dimensions: if bid_type == "banner" {
            get_imp_sizes(imp)
        } else {
            None
        },
        max_deals: if ext.max_deals > 0 {
            Some(ext.max_deals)
        } else {
            None
        },
        category: if ext.targeting.category.is_empty() {
            None
        } else {
            Some(ext.targeting.category.clone())
        },
        segments: if ext.targeting.segments.is_empty() {
            None
        } else {
            Some(ext.targeting.segments.clone())
        },
        keywords: if ext.targeting.keywords.is_empty() {
            None
        } else {
            Some(ext.targeting.keywords.clone())
        },
        key_values: if ext.targeting.key_values.is_empty() {
            None
        } else {
            Some(ext.targeting.key_values.clone())
        },
        ad_unit_matching_label: if ext.targeting.ad_unit_matching_label.is_empty() {
            None
        } else {
            Some(ext.targeting.ad_unit_matching_label.clone())
        },
    }
}

impl Bidder for AdnuntiusAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut network_adunit_map: HashMap<String, Vec<AdnRequestAdUnit>> = HashMap::new();
        let mut no_cookies = false;
        let mut all_imp_ids: Vec<String> = Vec::new();

        for imp in &request.imp {
            let ext = match parse_bidder_ext(imp) {
                Ok(e) => e,
                Err(e) => return (vec![], vec![e]),
            };

            if ext.no_cookies {
                no_cookies = true;
            }

            // Video not supported
            if imp.video.is_some() {
                return (
                    vec![],
                    vec![BidderError::BadInput(format!(
                        "ignoring imp id={}, Adnuntius supports only native and banner",
                        imp.id
                    ))],
                );
            }

            let network = if ext.network.is_empty() {
                DEFAULT_NETWORK.to_string()
            } else {
                ext.network.clone()
            };

            if imp.banner.is_some() {
                let ad_unit = build_ad_unit(imp, &ext, "banner");
                network_adunit_map.entry(network.clone()).or_default().push(ad_unit);
            }

            if imp.native.is_some() {
                let ad_unit = build_ad_unit(imp, &ext, "native");
                network_adunit_map.entry(network).or_default().push(ad_unit);
            }

            all_imp_ids.push(imp.id.clone());
        }

        let context = request
            .site
            .as_ref()
            .and_then(|s| s.page.as_ref().filter(|p| !p.is_empty()).cloned());

        let site_kv = request
            .site
            .as_ref()
            .and_then(|s| s.ext.as_ref())
            .and_then(|ext| ext.get("data").cloned());

        // Build user metadata (USI)
        let mut usi: Option<String> = None;
        if let Some(user) = &request.user {
            if let Some(id) = &user.id { if !id.is_empty() { usi = Some(id.clone()); } }
            if usi.is_none() {
                if let Some(ext) = &user.ext {
                    if let Some(eids) = ext.get("eids").and_then(|v| v.as_array()) {
                        if let Some(first_eid) = eids.first() {
                            if let Some(uids) = first_eid.get("uids").and_then(|v| v.as_array()) {
                                if let Some(first_uid) = uids.first() {
                                    if let Some(uid_id) =
                                        first_uid.get("id").and_then(|v| v.as_str())
                                    {
                                        usi = Some(uid_id.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json;charset=utf-8".to_string(),
        );
        headers.insert("Accept".to_string(), "application/json".to_string());
        if let Some(device) = &request.device {
            if let Some(ip) = &device.ip {
                if !ip.is_empty() {
                    headers.insert("X-Forwarded-For".to_string(), ip.clone());
                }
            }
            if let Some(ua) = &device.ua {
                if !ua.is_empty() {
                    headers.insert("user-agent".to_string(), ua.clone());
                }
            }
        }

        let endpoint = if no_cookies {
            if self.endpoint.contains('?') {
                format!("{}&noCookies=true", self.endpoint)
            } else {
                format!("{}?noCookies=true", self.endpoint)
            }
        } else {
            self.endpoint.clone()
        };

        let mut requests = Vec::new();
        for (_network, ad_units) in network_adunit_map {
            let meta_data = usi.as_ref().map(|u| AdnMetaData { usi: Some(u.clone()) });
            let adn_request = AdnRequest {
                ad_units,
                meta_data,
                context: context.clone(),
                key_values: site_kv.clone(),
            };

            let body = match serde_json::to_vec(&adn_request) {
                Ok(b) => b,
                Err(e) => {
                    return (
                        vec![],
                        vec![BidderError::BadInput(format!(
                            "Error marshalling adnuntius request: {e}"
                        ))],
                    )
                }
            };

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: endpoint.clone(),
                body,
                headers: headers.clone(),
                imp_ids: all_imp_ids.clone(),
            });
        }

        (requests, vec![])
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
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Status code: {}, Request malformed",
                response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Status code: {}, Something went wrong with your request",
                response.status_code
            ))]);
        }

        let adn_response: AdnResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        // Group ad units by base target key (strip ":bidType" suffix), keep highest bid
        let mut adunit_media_type_map: HashMap<String, Vec<&AdnAdUnit>> = HashMap::new();
        for adunit in &adn_response.ad_units {
            if adunit.matched_ad_count > 0 {
                let base_key = match adunit.target_id.rfind(':') {
                    Some(idx) => adunit.target_id[..idx].to_string(),
                    None => adunit.target_id.clone(),
                };
                adunit_media_type_map.entry(base_key).or_default().push(adunit);
            }
        }

        let mut adunit_map: HashMap<String, &AdnAdUnit> = HashMap::new();
        for (base_key, units) in &adunit_media_type_map {
            let best = units.iter().max_by(|a, b| {
                let a_bid = a.ads.first().map(|ad| ad.bid.amount).unwrap_or(0.0);
                let b_bid = b.ads.first().map(|ad| ad.bid.amount).unwrap_or(0.0);
                a_bid
                    .partial_cmp(&b_bid)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            if let Some(best) = best {
                adunit_map.insert(base_key.clone(), best);
            }
        }

        let mut result = BidderResponse::with_capacity(adn_response.ad_units.len());
        let mut currency = String::new();

        for imp in &internal.imp {
            let au_id = match imp
                .ext
                .as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("auId"))
                .and_then(|v| v.as_str())
            {
                Some(id) => id.to_string(),
                None => {
                    return Err(vec![BidderError::BadInput(format!(
                        "Error at Bidder auId for imp {}",
                        imp.id
                    ))])
                }
            };

            let bid_type_pref = imp
                .ext
                .as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("bidType"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();

            let target_id_base = format!("{}-{}", au_id, imp.id);
            let adunit = match adunit_map.get(&target_id_base) {
                Some(u) => u,
                None => continue,
            };

            if adunit.ads.is_empty() {
                continue;
            }

            let ad = &adunit.ads[0];

            // Determine html and media type
            let (html, bid_type) = if let Some(native_json) = &adunit.native_json {
                let native_html = native_json
                    .get("ortb")
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| adunit.html.clone());
                (native_html, BidType::Native)
            } else {
                (adunit.html.clone(), BidType::Banner)
            };

            // Determine price based on bidType preference
            let price = if bid_type_pref == "net" {
                ad.net_bid.as_ref().map(|b| b.amount).unwrap_or(ad.bid.amount)
            } else if bid_type_pref == "gross" {
                ad.gross_bid
                    .as_ref()
                    .map(|b| b.amount)
                    .unwrap_or(ad.bid.amount)
            } else {
                ad.bid.amount
            };

            if !ad.bid.currency.is_empty() {
                currency = ad.bid.currency.clone();
            }

            let w = ad.creative_width.parse::<i32>().ok();
            let h = ad.creative_height.parse::<i32>().ok();
            let mtype = Some(match bid_type {
                BidType::Native => 4_i32,
                _ => 1_i32,
            });

            let bid = openrtb::Bid {
                id: ad.ad_id.clone(),
                impid: imp.id.clone(),
                price: price * 1000.0,
                adid: if ad.ad_id.is_empty() {
                    None
                } else {
                    Some(ad.ad_id.clone())
                },
                adm: if html.is_empty() { None } else { Some(html) },
                adomain: if ad.advertiser_domains.is_empty() {
                    None
                } else {
                    Some(ad.advertiser_domains.clone())
                },
                cid: if ad.line_item_id.is_empty() {
                    None
                } else {
                    Some(ad.line_item_id.clone())
                },
                crid: if ad.creative_id.is_empty() {
                    None
                } else {
                    Some(ad.creative_id.clone())
                },
                dealid: if ad.deal_id.is_empty() {
                    None
                } else {
                    Some(ad.deal_id.clone())
                },
                w,
                h,
                mtype,
                ..Default::default()
            };
            result.bids.push(TypedBid::new(bid, bid_type));

            // Process deals
            for deal in &adunit.deals {
                let deal_price = if bid_type_pref == "net" {
                    deal.net_bid
                        .as_ref()
                        .map(|b| b.amount)
                        .unwrap_or(deal.bid.amount)
                } else if bid_type_pref == "gross" {
                    deal.gross_bid
                        .as_ref()
                        .map(|b| b.amount)
                        .unwrap_or(deal.bid.amount)
                } else {
                    deal.bid.amount
                };

                let deal_bid = openrtb::Bid {
                    id: deal.ad_id.clone(),
                    impid: imp.id.clone(),
                    price: deal_price * 1000.0,
                    adid: if deal.ad_id.is_empty() {
                        None
                    } else {
                        Some(deal.ad_id.clone())
                    },
                    adm: if deal.html.is_empty() {
                        None
                    } else {
                        Some(deal.html.clone())
                    },
                    adomain: if deal.advertiser_domains.is_empty() {
                        None
                    } else {
                        Some(deal.advertiser_domains.clone())
                    },
                    cid: if deal.line_item_id.is_empty() {
                        None
                    } else {
                        Some(deal.line_item_id.clone())
                    },
                    crid: if deal.creative_id.is_empty() {
                        None
                    } else {
                        Some(deal.creative_id.clone())
                    },
                    dealid: if deal.deal_id.is_empty() {
                        None
                    } else {
                        Some(deal.deal_id.clone())
                    },
                    w: deal.creative_width.parse::<i32>().ok(),
                    h: deal.creative_height.parse::<i32>().ok(),
                    mtype: Some(1_i32),
                    ..Default::default()
                };
                result.bids.push(TypedBid::new(deal_bid, BidType::Banner));
            }
        }

        if !currency.is_empty() {
            result.currency = currency;
        }

        Ok(result)
    }
}
