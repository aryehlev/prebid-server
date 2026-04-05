use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

const MF_SUFFIX: &str = "__mf";
const MF_SUFFIX_BANNER: &str = "b__mf";
const MF_SUFFIX_VIDEO: &str = "v__mf";
const MF_SUFFIX_AUDIO: &str = "a__mf";
const MF_SUFFIX_NATIVE: &str = "n__mf";

pub struct AdkernelAdapter {
    pub endpoint: String,
}

impl AdkernelAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

fn get_bid_type_from_mtype(mtype: i32) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        3 => Ok(BidType::Audio),
        4 => Ok(BidType::Native),
        other => Err(BidderError::BadServerResponse(format!(
            "Unsupported MType {}", other
        ))),
    }
}

/// Returns true if the impression has more than one format (banner, video, audio, native).
fn is_multi_format_imp(imp: &openrtb::Imp) -> bool {
    let mut count = 0;
    if imp.banner.is_some() { count += 1; }
    if imp.video.is_some() { count += 1; }
    if imp.audio.is_some() { count += 1; }
    if imp.native.is_some() { count += 1; }
    count > 1
}

/// Split a multi-format impression into separate single-format impressions, each with a suffix on the ID.
fn split_multi_format_imp(imp: &openrtb::Imp) -> Vec<openrtb::Imp> {
    let mut split = Vec::with_capacity(4);
    if imp.banner.is_some() {
        let mut c = imp.clone();
        c.video = None;
        c.native = None;
        c.audio = None;
        c.id = format!("{}{}", imp.id, MF_SUFFIX_BANNER);
        split.push(c);
    }
    if imp.video.is_some() {
        let mut c = imp.clone();
        c.banner = None;
        c.native = None;
        c.audio = None;
        c.id = format!("{}{}", imp.id, MF_SUFFIX_VIDEO);
        split.push(c);
    }
    if imp.native.is_some() {
        let mut c = imp.clone();
        c.banner = None;
        c.video = None;
        c.audio = None;
        c.id = format!("{}{}", imp.id, MF_SUFFIX_NATIVE);
        split.push(c);
    }
    if imp.audio.is_some() {
        let mut c = imp.clone();
        c.banner = None;
        c.video = None;
        c.native = None;
        c.id = format!("{}{}", imp.id, MF_SUFFIX_AUDIO);
        split.push(c);
    }
    split
}

/// Parse zone_id from impression extension bidder field.
fn parse_zone_id(imp: &openrtb::Imp) -> Option<i64> {
    imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .and_then(|b| b.get("zoneId").or_else(|| b.get("zone_id")))
        .and_then(|v| v.as_i64())
}

impl Bidder for AdkernelAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No impression in the bid request".to_string())]);
        }

        let mut errs = Vec::new();

        // Validate impressions and extract zone_id; group by zone_id
        let mut zone_to_imps: HashMap<i64, Vec<openrtb::Imp>> = HashMap::new();

        for imp in &request.imp {
            let zone_id = parse_zone_id(imp).unwrap_or(0);

            if zone_id < 1 {
                errs.push(BidderError::BadInput(format!(
                    "Invalid zoneId value: {}. Ignoring imp id={}", zone_id, imp.id
                )));
                continue;
            }

            let mut imp_copy = imp.clone();
            imp_copy.ext = None;

            // Split multi-format impressions into individual ones
            let imps_to_add = if is_multi_format_imp(&imp_copy) {
                split_multi_format_imp(&imp_copy)
            } else {
                vec![imp_copy]
            };

            zone_to_imps.entry(zone_id).or_default().extend(imps_to_add);
        }

        if zone_to_imps.is_empty() {
            return (vec![], errs);
        }

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("x-openrtb-version".to_string(), "2.5".to_string());

        let mut requests = Vec::new();
        for (zone_id, imps) in zone_to_imps {
            // Build per-zone request: clear publisher info from site/app
            let mut req = request.clone();
            if let Some(site) = req.site.as_mut() {
                site.publisher = None;
            }
            if let Some(app) = req.app.as_mut() {
                app.publisher = None;
            }
            req.imp = imps;

            let body = match serde_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            // Resolve endpoint URL by substituting the zone ID macro
            let uri = self.endpoint.replace("{{.ZoneID}}", &zone_id.to_string());
            let imp_ids = get_imp_ids(&req.imp);
            requests.push(RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers: headers.clone(),
                imp_ids,
            });
        }

        (requests, errs)
    }

    fn make_bids(
        &self,
        _internal: &openrtb::BidRequest,
        _external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
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

        // AdKernel always returns exactly one SeatBid
        if bid_resp.seatbid.len() != 1 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Invalid SeatBids count: {}", bid_resp.seatbid.len()
            ))]);
        }

        let seat_bid = &bid_resp.seatbid[0];
        let mut result = BidderResponse::with_capacity(seat_bid.bid.len());

        // Propagate response currency if present
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
        }

        for mut bid in seat_bid.bid.clone() {
            // Strip multi-format suffix from impid if present (e.g. "imp1b__mf" → "imp1")
            if bid.impid.ends_with(MF_SUFFIX) {
                // Remove the format-char prefix too: len - len("__mf") - 1
                let new_len = bid.impid.len() - MF_SUFFIX.len() - 1;
                bid.impid = bid.impid[..new_len].to_string();
            }

            let mtype = bid.mtype.unwrap_or(0);
            let bid_type = get_bid_type_from_mtype(mtype)
                .map_err(|e| vec![e])?;
            result.bids.push(TypedBid::new(bid, bid_type));
        }

        Ok(result)
    }
}
