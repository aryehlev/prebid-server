use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct SmaatoAdapter {
    pub endpoint: String,
}

impl SmaatoAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

fn get_bid_type_from_adtype(adtype: &str) -> Result<BidType, BidderError> {
    match adtype {
        "Img" | "Richmedia" => Ok(BidType::Banner),
        "Video" => Ok(BidType::Video),
        "Native" => Ok(BidType::Native),
        other => Err(BidderError::BadServerResponse(format!(
            "Unknown markup type {}.", other
        ))),
    }
}

impl Bidder for SmaatoAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No impressions in bid request.".to_string())]);
        }

        let mut errs = Vec::new();
        let mut requests = Vec::new();

        // Determine publisher_id from first imp's bidder ext
        let publisher_id = request.imp.first()
            .and_then(|imp| imp.ext.as_ref())
            .and_then(|e| e.get("bidder"))
            .and_then(|b| b.get("publisherId"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let is_video_entry = info.pbs_entry_point == "video";

        if is_video_entry {
            // Pod requests: group by pod (first part of imp.id before "_")
            let mut pods: std::collections::BTreeMap<String, Vec<openrtb::Imp>> = std::collections::BTreeMap::new();
            for imp in &request.imp {
                if imp.video.is_none() {
                    errs.push(BidderError::BadInput("Invalid MediaType. Smaato only supports Video for AdPod.".to_string()));
                    continue;
                }
                let pod = imp.id.split('_').next().unwrap_or(&imp.id).to_string();
                pods.entry(pod).or_default().push(imp.clone());
            }
            for (_pod, imps) in pods {
                let mut req = request.clone();
                req.imp = imps;
                if !publisher_id.is_empty() {
                    set_publisher_id(&mut req, &publisher_id);
                }
                set_client_ext(&mut req);
                match build_request_data(&req, &self.endpoint) {
                    Ok(rd) => requests.push(rd),
                    Err(e) => errs.push(e),
                }
            }
        } else {
            // Individual requests: one per imp per media type
            for imp in &request.imp {
                let mut split_imps = Vec::new();
                if let Some(banner) = imp.banner.clone() {
                    let mut imp_copy = imp.clone();
                    imp_copy.video = None;
                    imp_copy.native = None;
                    split_imps.push(imp_copy);
                }
                if imp.video.is_some() {
                    let mut imp_copy = imp.clone();
                    imp_copy.banner = None;
                    imp_copy.native = None;
                    split_imps.push(imp_copy);
                }
                if imp.native.is_some() {
                    let mut imp_copy = imp.clone();
                    imp_copy.banner = None;
                    imp_copy.video = None;
                    split_imps.push(imp_copy);
                }
                if split_imps.is_empty() {
                    errs.push(BidderError::BadInput("Invalid MediaType. Smaato only supports Banner, Video and Native.".to_string()));
                    continue;
                }
                for split_imp in split_imps {
                    // Extract adspaceId from bidder ext and set as tag_id
                    let adspace_id = split_imp.ext.as_ref()
                        .and_then(|e| e.get("bidder"))
                        .and_then(|b| b.get("adspaceId"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let mut req = request.clone();
                    let mut imp_mod = split_imp;
                    if !adspace_id.is_empty() {
                        imp_mod.tagid = Some(adspace_id);
                    }
                    // Remove bidder from imp ext
                    if let Some(ext) = imp_mod.ext.as_mut() {
                        if let Some(obj) = ext.as_object_mut() {
                            obj.remove("bidder");
                            if obj.is_empty() {
                                imp_mod.ext = None;
                            }
                        }
                    }
                    req.imp = vec![imp_mod];
                    if !publisher_id.is_empty() {
                        set_publisher_id(&mut req, &publisher_id);
                    }
                    set_client_ext(&mut req);
                    match build_request_data(&req, &self.endpoint) {
                        Ok(rd) => requests.push(rd),
                        Err(e) => errs.push(e),
                    }
                }
            }
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
                "Unexpected status code: {}. Run with request.debug = 1 for more info.", response.status_code
            ))]);
        }

        // Get X-Smt-Adtype header
        let adtype = response.headers.get("X-Smt-Adtype")
            .map(|s| s.as_str())
            .unwrap_or("");
        if adtype.is_empty() {
            return Err(vec![BidderError::BadServerResponse("X-Smt-Adtype header is missing.".to_string())]);
        }

        let bid_type = get_bid_type_from_adtype(adtype)
            .map_err(|e| vec![e])?;

        let bid_response: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        let mut errs: Vec<BidderError> = Vec::new();

        for sb in bid_response.seatbid {
            for bid in sb.bid {
                result.bids.push(TypedBid::new(bid, bid_type.clone()));
            }
        }

        if errs.is_empty() {
            Ok(result)
        } else {
            Ok(result)
        }
    }
}

fn set_publisher_id(request: &mut openrtb::BidRequest, publisher_id: &str) {
    let pub_obj = openrtb::Publisher {
        id: Some(publisher_id.to_string()),
        ..Default::default()
    };
    if let Some(site) = request.site.as_mut() {
        site.publisher = Some(pub_obj);
    } else if let Some(app) = request.app.as_mut() {
        app.publisher = Some(pub_obj);
    }
}

fn set_client_ext(request: &mut openrtb::BidRequest) {
    // Set request.ext.client = "prebid_server_1.2"
    let ext = serde_json::json!({"client": "prebid_server_1.2"});
    request.ext = Some(ext);
}

fn build_request_data(request: &openrtb::BidRequest, endpoint: &str) -> Result<RequestData, BidderError> {
    let body = serde_json::to_vec(request)
        .map_err(|e| BidderError::BadInput(e.to_string()))?;
    let mut headers = HashMap::new();
    headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
    headers.insert("Accept".to_string(), "application/json".to_string());
    Ok(RequestData {
        method: "POST".to_string(),
        uri: endpoint.to_string(),
        body,
        headers,
        imp_ids: get_imp_ids(&request.imp),
    })
}
