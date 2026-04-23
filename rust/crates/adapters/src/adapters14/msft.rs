use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::{BidType, ExtBidPrebidVideo};
use serde::{Deserialize, Serialize};

const MAX_IMPS_PER_REQ: usize = 10;
const DEFAULT_HB_SOURCE: i32 = 5;
const DEFAULT_HB_SOURCE_VIDEO: i32 = 6;

pub struct MsftAdapter {
    pub endpoint: String,
    pub hb_source: i32,
    pub hb_source_video: i32,
}

impl MsftAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint, hb_source: DEFAULT_HB_SOURCE, hb_source_video: DEFAULT_HB_SOURCE_VIDEO }
    }
}

#[derive(Debug, Deserialize, Default)]
struct ImpExtBidderMsft {
    #[serde(rename = "placementId", default)]
    placement_id: i64,
    #[serde(rename = "invCode", default)]
    inv_code: String,
    #[serde(rename = "member", default)]
    member: i64,
    #[serde(rename = "trafficSourceCode", default)]
    traffic_source_code: String,
    #[serde(rename = "keywords", default)]
    keywords: String,
    #[serde(rename = "pubClick", default)]
    pub_click: String,
    #[serde(rename = "extInvCode", default)]
    ext_inv_code: String,
    #[serde(rename = "extImpId", default)]
    ext_imp_id: String,
    #[serde(rename = "usePaymentRule", default)]
    use_payment_rule: Option<bool>,
    #[serde(rename = "allowSmallerSizes", default)]
    allow_smaller_sizes: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
struct ImpExtIncoming {
    #[serde(default)]
    bidder: ImpExtBidderMsft,
    #[serde(default)]
    gpid: String,
}

#[derive(Debug, Serialize, Default)]
struct ImpExtOutgoingAppnexus {
    #[serde(rename = "placement_id", skip_serializing_if = "is_zero_i64")]
    placement_id: i64,
    #[serde(rename = "allow_smaller_sizes", skip_serializing_if = "Option::is_none")]
    allow_smaller_sizes: Option<bool>,
    #[serde(rename = "use_pmt_rule", skip_serializing_if = "Option::is_none")]
    use_pmt_rule: Option<bool>,
    #[serde(rename = "keywords", skip_serializing_if = "String::is_empty")]
    keywords: String,
    #[serde(rename = "traffic_source_code", skip_serializing_if = "String::is_empty")]
    traffic_source_code: String,
    #[serde(rename = "pub_click", skip_serializing_if = "String::is_empty")]
    pub_click: String,
    #[serde(rename = "ext_inv_code", skip_serializing_if = "String::is_empty")]
    ext_inv_code: String,
    #[serde(rename = "ext_imp_id", skip_serializing_if = "String::is_empty")]
    ext_imp_id: String,
}

#[derive(Debug, Serialize, Default)]
struct ImpExtOutgoing {
    appnexus: ImpExtOutgoingAppnexus,
    #[serde(skip_serializing_if = "String::is_empty")]
    gpid: String,
}

fn is_zero_i64(v: &i64) -> bool { *v == 0 }

#[derive(Debug, Deserialize, Default)]
struct BidExtAppnexus {
    #[serde(rename = "bid_ad_type", default)]
    bid_type: i32,
    #[serde(rename = "brand_category_id", default)]
    brand_category: i32,
    #[serde(rename = "deal_priority", default)]
    deal_priority: i32,
    #[serde(default)]
    creative_info: BidExtCreativeInfo,
}

#[derive(Debug, Deserialize, Default)]
struct BidExtCreativeInfo {
    #[serde(default)]
    video: BidExtVideo,
}

#[derive(Debug, Deserialize, Default)]
struct BidExtVideo {
    #[serde(default)]
    duration: i32,
}

#[derive(Debug, Deserialize, Default)]
struct BidExt {
    #[serde(default)]
    appnexus: BidExtAppnexus,
}

/// Request-level ext appnexus object
#[derive(Debug, Serialize, Deserialize, Default)]
struct ReqExtAppnexus {
    #[serde(rename = "include_brand_category", skip_serializing_if = "Option::is_none")]
    include_brand_category: Option<bool>,
    #[serde(rename = "brand_category_uniqueness", skip_serializing_if = "Option::is_none")]
    brand_category_uniqueness: Option<bool>,
    #[serde(rename = "is_amp", skip_serializing_if = "is_zero_i32")]
    is_amp: i32,
    #[serde(rename = "hb_source", skip_serializing_if = "is_zero_i32")]
    hb_source: i32,
}

fn is_zero_i32(v: &i32) -> bool { *v == 0 }

fn get_media_type_for_bid(bid_type: i32) -> Result<BidType, BidderError> {
    match bid_type {
        0 => Ok(BidType::Banner),
        1 => Ok(BidType::Video),
        3 => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(format!("unrecognized bid_ad_type in response: {}", bid_type))),
    }
}

/// Maps Microsoft brand_category_id to an IAB category string.
fn find_iab_category(brand_category: i32) -> Option<&'static str> {
    match brand_category {
        1   => Some("IAB20-3"),
        2   => Some("IAB18-5"),
        3   => Some("IAB10-1"),
        4   => Some("IAB2-3"),
        5   => Some("IAB19-8"),
        6   => Some("IAB22-1"),
        7   => Some("IAB18-1"),
        8   => Some("IAB12-3"),
        9   => Some("IAB5-1"),
        10  => Some("IAB4-5"),
        11  => Some("IAB13-4"),
        12  => Some("IAB8-7"),
        13  => Some("IAB9-7"),
        14  => Some("IAB7-1"),
        15  => Some("IAB20-18"),
        16  => Some("IAB10-7"),
        17  => Some("IAB19-18"),
        18  => Some("IAB13-6"),
        19  => Some("IAB18-4"),
        20  => Some("IAB1-5"),
        21  => Some("IAB1-6"),
        22  => Some("IAB3-4"),
        23  => Some("IAB19-13"),
        24  => Some("IAB22-2"),
        25  => Some("IAB3-9"),
        26  => Some("IAB17-18"),
        27  => Some("IAB19-6"),
        28  => Some("IAB1-7"),
        29  => Some("IAB9-30"),
        30  => Some("IAB20-7"),
        31  => Some("IAB20-17"),
        32  => Some("IAB7-32"),
        33  => Some("IAB16-5"),
        34  => Some("IAB19-34"),
        35  => Some("IAB11-5"),
        36  => Some("IAB12-3"),
        37  => Some("IAB11-4"),
        38  => Some("IAB12-3"),
        39  => Some("IAB9-30"),
        41  => Some("IAB7-44"),
        42  => Some("IAB7-1"),
        43  => Some("IAB7-30"),
        50  => Some("IAB19-30"),
        51  => Some("IAB17-12"),
        52  => Some("IAB19-30"),
        53  => Some("IAB3-1"),
        55  => Some("IAB13-2"),
        56  => Some("IAB19-30"),
        57  => Some("IAB19-30"),
        58  => Some("IAB7-39"),
        59  => Some("IAB22-1"),
        60  => Some("IAB7-39"),
        61  => Some("IAB21-3"),
        62  => Some("IAB5-1"),
        63  => Some("IAB12-3"),
        64  => Some("IAB20-18"),
        65  => Some("IAB11-2"),
        66  => Some("IAB17-18"),
        67  => Some("IAB9-9"),
        68  => Some("IAB9-5"),
        69  => Some("IAB7-44"),
        71  => Some("IAB22-3"),
        73  => Some("IAB19-30"),
        74  => Some("IAB8-5"),
        78  => Some("IAB22-1"),
        85  => Some("IAB12-2"),
        86  => Some("IAB22-3"),
        87  => Some("IAB11-3"),
        112 => Some("IAB7-32"),
        113 => Some("IAB7-32"),
        114 => Some("IAB7-32"),
        115 => Some("IAB7-32"),
        118 => Some("IAB9-5"),
        119 => Some("IAB9-5"),
        120 => Some("IAB9-5"),
        121 => Some("IAB9-5"),
        122 => Some("IAB9-5"),
        123 => Some("IAB9-5"),
        124 => Some("IAB9-5"),
        125 => Some("IAB9-5"),
        126 => Some("IAB9-5"),
        127 => Some("IAB22-1"),
        132 => Some("IAB1-2"),
        133 => Some("IAB19-30"),
        137 => Some("IAB3-9"),
        138 => Some("IAB19-3"),
        140 => Some("IAB2-3"),
        141 => Some("IAB2-1"),
        142 => Some("IAB2-3"),
        143 => Some("IAB17-13"),
        166 => Some("IAB11-4"),
        175 => Some("IAB3-1"),
        176 => Some("IAB13-4"),
        182 => Some("IAB8-9"),
        183 => Some("IAB3-5"),
        _   => None,
    }
}

impl Bidder for MsftAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, info: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("x-openrtb-version".to_string(), "2.6".to_string());

        let mut valid_imps = Vec::new();
        let mut unique_member_id: i64 = 0;

        for imp in &request.imp {
            let ext_val = match &imp.ext {
                Some(v) => v.clone(),
                None => { errs.push(BidderError::BadInput(format!("failed to parse ext for impression id '{}'", imp.id))); continue; }
            };
            let imp_ext: ImpExtIncoming = match serde_json::from_value(ext_val) {
                Ok(v) => v,
                Err(_) => { errs.push(BidderError::BadInput(format!("failed to parse ext for impression id '{}'", imp.id))); continue; }
            };

            let member_id = imp_ext.bidder.member;
            if member_id != 0 {
                if unique_member_id == 0 {
                    unique_member_id = member_id;
                } else if unique_member_id != member_id {
                    errs.push(BidderError::BadInput("member id mismatch: all impressions must use the same member id".to_string()));
                    return (vec![], errs);
                }
            }

            let mut imp_copy = imp.clone();
            // Apply inv_code as tagid
            if !imp_ext.bidder.inv_code.is_empty() {
                imp_copy.tagid = Some(imp_ext.bidder.inv_code.clone());
            }
            // Assign banner size from first format if missing
            if let Some(banner) = imp_copy.banner.as_mut() {
                if banner.w.is_none() && banner.h.is_none() {
                    if let Some(fmt) = banner.format.as_deref().and_then(|f| f.first()) {
                        banner.w = Some(fmt.w.unwrap_or(0));
                        banner.h = Some(fmt.h.unwrap_or(0));
                    }
                }
            }
            // Build outgoing ext
            let out_ext = ImpExtOutgoing {
                appnexus: ImpExtOutgoingAppnexus {
                    placement_id: imp_ext.bidder.placement_id,
                    allow_smaller_sizes: imp_ext.bidder.allow_smaller_sizes,
                    use_pmt_rule: imp_ext.bidder.use_payment_rule,
                    keywords: imp_ext.bidder.keywords,
                    traffic_source_code: imp_ext.bidder.traffic_source_code,
                    pub_click: imp_ext.bidder.pub_click,
                    ext_inv_code: imp_ext.bidder.ext_inv_code,
                    ext_imp_id: imp_ext.bidder.ext_imp_id,
                },
                gpid: imp_ext.gpid,
            };
            imp_copy.ext = match serde_json::to_value(out_ext) {
                Ok(v) => Some(v),
                Err(_) => { errs.push(BidderError::BadInput(format!("failed to build ext for impression id '{}'", imp.id))); continue; }
            };
            valid_imps.push(imp_copy);
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        // Build URI with member_id if needed
        let uri = if unique_member_id != 0 {
            if self.endpoint.contains('?') {
                format!("{}&member_id={}", self.endpoint, unique_member_id)
            } else {
                format!("{}?member_id={}", self.endpoint, unique_member_id)
            }
        } else {
            self.endpoint.clone()
        };

        // Build modified request ext with appnexus section
        let mut req_copy = request.clone();
        req_copy.imp = valid_imps.clone();
        match self.modify_request_ext(&mut req_copy, info) {
            Ok(()) => {}
            Err(e) => { errs.push(e); return (vec![], errs); }
        }

        // Split into chunks of MAX_IMPS_PER_REQ
        let mut requests = Vec::new();
        for chunk in valid_imps.chunks(MAX_IMPS_PER_REQ) {
            let mut chunk_req = req_copy.clone();
            chunk_req.imp = chunk.to_vec();
            let imp_ids = chunk_req.imp.iter().map(|i| i.id.clone()).collect();
            let body = match serde_json::to_vec(&chunk_req) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); return (vec![], errs); }
            };
            requests.push(RequestData { method: "POST".to_string(), uri: uri.clone(), body, headers: headers.clone(), imp_ids });
        }

        (requests, errs)
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
                let bid_ext: BidExt = bid.ext.as_ref()
                    .and_then(|e| serde_json::from_value(e.clone()).ok())
                    .unwrap_or_default();
                match get_media_type_for_bid(bid_ext.appnexus.bid_type) {
                    Ok(bid_type) => {
                        // Apply IAB category mapping
                        let brand_cat = bid_ext.appnexus.brand_category;
                        if let Some(iab_cat) = find_iab_category(brand_cat) {
                            bid.cat = Some(vec![iab_cat.to_string()]);
                        } else if bid.cat.as_ref().map(|c| c.len() > 1).unwrap_or(false) {
                            // Force rejection: multiple cats but none mapped → empty list
                            bid.cat = Some(vec![]);
                        }
                        let duration = bid_ext.appnexus.creative_info.video.duration;
                        let mut typed_bid = TypedBid::new(bid, bid_type);
                        typed_bid.bid_video = Some(ExtBidPrebidVideo { duration, primary_category: String::new() });
                        typed_bid.deal_priority = bid_ext.appnexus.deal_priority;
                        result.bids.push(typed_bid);
                    }
                    Err(e) => errs.push(e),
                }
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}

impl MsftAdapter {
    /// Modify the request ext to include appnexus-specific fields (hb_source, is_amp, brand category flags).
    fn modify_request_ext(&self, request: &mut openrtb::BidRequest, info: &ExtraRequestInfo) -> Result<(), BidderError> {
        // Parse existing ext as a map
        let mut ext_map: HashMap<String, serde_json::Value> = request.ext
            .as_ref()
            .and_then(|e| serde_json::from_value(e.clone()).ok())
            .unwrap_or_default();

        // Extract or default the appnexus section
        let mut appnexus_ext: ReqExtAppnexus = ext_map
            .get("appnexus")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        // Check for prebid.targeting.includebrandcategory
        if let Some(prebid_val) = ext_map.get("prebid") {
            let has_brand_cat = prebid_val
                .get("targeting")
                .and_then(|t| t.get("includebrandcategory"))
                .map(|v| v.is_object())
                .unwrap_or(false);
            if has_brand_cat {
                appnexus_ext.brand_category_uniqueness = Some(true);
                appnexus_ext.include_brand_category = Some(true);
            }
        }

        // Set is_amp
        if info.pbs_entry_point == "amp" {
            appnexus_ext.is_amp = 1;
        }

        // Set hb_source
        if info.pbs_entry_point == "video" {
            appnexus_ext.hb_source = self.hb_source_video;
        } else {
            appnexus_ext.hb_source = self.hb_source;
        }

        let appnexus_val = serde_json::to_value(&appnexus_ext)
            .map_err(|e| BidderError::BadInput(e.to_string()))?;
        ext_map.insert("appnexus".to_string(), appnexus_val);

        request.ext = Some(serde_json::to_value(ext_map)
            .map_err(|e| BidderError::BadInput(e.to_string()))?);
        Ok(())
    }
}
