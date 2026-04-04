use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct AdkernelAdapter {
    pub endpoint: String,
}

impl AdkernelAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

fn get_bid_type_from_mtype(mtype: u32) -> Result<BidType, BidderError> {
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

        // Group imps by zone_id from ext.bidder.zoneId
        let mut zone_to_imps: std::collections::HashMap<i64, Vec<openrtb::Imp>> = std::collections::HashMap::new();

        for imp in &request.imp {
            let zone_id = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("zoneId").or_else(|| b.get("zone_id")))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            if zone_id < 1 {
                errs.push(BidderError::BadInput(format!(
                    "Invalid zoneId value: {}. Ignoring imp id={}", zone_id, imp.id
                )));
                continue;
            }

            let mut imp_copy = imp.clone();
            imp_copy.ext = None;
            zone_to_imps.entry(zone_id).or_default().push(imp_copy);
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
            let mut req = request.clone();
            // Clear publisher info per Go impl
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
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![e]);
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

        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
        }

        for mut bid in seat_bid.bid.clone() {
            // Strip multi-format suffix from impid if present
            const MF_SUFFIX: &str = "__mf";
            if bid.impid.ends_with(MF_SUFFIX) {
                let new_len = bid.impid.len() - MF_SUFFIX.len() - 1;
                bid.impid = bid.impid[..new_len].to_string();
            }

            let mtype = bid.mtype.unwrap_or(0) as u32;
            let bid_type = get_bid_type_from_mtype(mtype)
                .map_err(|e| vec![e])?;
            result.bids.push(TypedBid::new(bid, bid_type));
        }

        Ok(result)
    }
}
