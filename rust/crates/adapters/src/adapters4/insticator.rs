use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct InsticatorAdapter {
    pub endpoint: String,
}

impl InsticatorAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(serde::Deserialize)]
struct ExtImpBidder {
    bidder: InsticatorImpExt,
}

#[derive(serde::Deserialize)]
struct InsticatorImpExt {
    #[serde(rename = "adUnitId", default)]
    ad_unit_id: String,
    #[serde(rename = "publisherId", default)]
    publisher_id: String,
}

#[derive(serde::Serialize)]
struct OutgoingImpExt {
    insticator: OutgoingInsticatorImpExt,
}

#[derive(serde::Serialize)]
struct OutgoingInsticatorImpExt {
    #[serde(rename = "adUnitId")]
    ad_unit_id: String,
    #[serde(rename = "publisherId")]
    publisher_id: String,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct ReqExt {
    #[serde(skip_serializing_if = "Option::is_none")]
    insticator: Option<ReqInsticatorExt>,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct ReqInsticatorExt {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    caller: Vec<InsticatorCaller>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct InsticatorCaller {
    name: String,
    version: String,
}

fn get_media_type_for_bid(mtype: Option<u32>) -> BidType {
    match mtype {
        Some(1) => BidType::Banner,
        Some(2) => BidType::Video,
        _ => BidType::Banner,
    }
}

impl Bidder for InsticatorAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors = Vec::new();

        // Build the caller-augmented request ext.
        let mut req_ext: ReqExt = request.ext.as_ref()
            .and_then(|e| serde_json::from_str(e.get()).ok())
            .unwrap_or_default();

        let insticator_ext = req_ext.insticator.get_or_insert_with(ReqInsticatorExt::default);
        insticator_ext.caller.push(InsticatorCaller {
            name: "Prebid-Server".to_string(),
            version: "n/a".to_string(),
        });

        let req_ext_val = match serde_json::to_value(&req_ext) {
            Ok(v) => v,
            Err(e) => {
                return (vec![], vec![BidderError::BadInput(e.to_string())]);
            }
        };

        // Group imps by adUnitId.
        let mut grouped: HashMap<String, Vec<openrtb::Imp>> = HashMap::new();
        let mut publisher_id = String::new();
        let mut publisher_id_set = false;

        for imp in &request.imp {
            let ins_ext = match imp.ext.as_ref()
                .and_then(|e| serde_json::from_str::<ExtImpBidder>(e.get()).ok())
            {
                Some(e) => e.bidder,
                None => {
                    errors.push(BidderError::BadInput(
                        "failed to parse insticator imp ext".to_string(),
                    ));
                    continue;
                }
            };

            if !publisher_id_set {
                publisher_id = ins_ext.publisher_id.clone();
                publisher_id_set = true;
            }

            let outgoing_ext = OutgoingImpExt {
                insticator: OutgoingInsticatorImpExt {
                    ad_unit_id: ins_ext.ad_unit_id.clone(),
                    publisher_id: ins_ext.publisher_id,
                },
            };

            let new_ext = match serde_json::to_value(&outgoing_ext) {
                Ok(v) => v,
                Err(e) => {
                    errors.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(new_ext);

            grouped.entry(ins_ext.ad_unit_id).or_default().push(imp_copy);
        }

        if grouped.is_empty() {
            return (vec![], errors);
        }

        // Adjust publisher ID in site/app.
        let mut req_copy = request.clone();
        req_copy.ext = Some(req_ext_val);

        if !publisher_id.is_empty() {
            if let Some(site) = &req_copy.site {
                let mut site_copy = site.clone();
                let mut pub_copy = site_copy.publisher.clone().unwrap_or_default();
                pub_copy.id = Some(publisher_id.clone());
                site_copy.publisher = Some(pub_copy);
                req_copy.site = Some(site_copy);
            }
            if let Some(app) = &req_copy.app {
                let mut app_copy = app.clone();
                let mut pub_copy = app_copy.publisher.clone().unwrap_or_default();
                pub_copy.id = Some(publisher_id.clone());
                app_copy.publisher = Some(pub_copy);
                req_copy.app = Some(app_copy);
            }
        }

        let mut requests = Vec::new();

        for imp_list in grouped.into_values() {
            let mut r = req_copy.clone();
            r.imp = imp_list;

            // Build endpoint URL with publisherId query param.
            let uri = if !publisher_id.is_empty() {
                if self.endpoint.contains('?') {
                    format!("{}&publisherId={}", self.endpoint, publisher_id)
                } else {
                    format!("{}?publisherId={}", self.endpoint, publisher_id)
                }
            } else {
                self.endpoint.clone()
            };

            let body = match serde_json::to_vec(&r) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());

            if let Some(device) = &request.device {
                if let Some(ua) = &device.ua {
                    if !ua.is_empty() {
                        headers.insert("User-Agent".to_string(), ua.clone());
                    }
                }
                if let Some(ip) = &device.ip {
                    if !ip.is_empty() {
                        headers.insert("X-Forwarded-For".to_string(), ip.clone());
                        headers.insert("IP".to_string(), ip.clone());
                    }
                } else if let Some(ipv6) = &device.ipv6 {
                    if !ipv6.is_empty() {
                        headers.insert("X-Forwarded-For".to_string(), ipv6.clone());
                    }
                }
            }

            requests.push(RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers,
                imp_ids: get_imp_ids(&r.imp),
            });
        }

        (requests, errors)
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
        if let Err(e) = pbs_adapters::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        if bid_resp.cur.as_deref().map(|c| !c.is_empty()).unwrap_or(false) {
            result.currency = bid_resp.cur.unwrap();
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_media_type_for_bid(bid.mtype);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
