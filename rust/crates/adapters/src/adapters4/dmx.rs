use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct DmxAdapter {
    pub endpoint: String,
}

impl DmxAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(serde::Deserialize, Default)]
struct DmxExt {
    #[serde(default)]
    bidder: DmxParams,
}

#[derive(serde::Deserialize, Default)]
struct DmxParams {
    #[serde(rename = "tagid", default)]
    tag_id: String,
    #[serde(rename = "dmxid", default)]
    dmx_id: String,
    #[serde(rename = "memberid", default)]
    member_id: String,
    #[serde(rename = "publisher_id", default)]
    publisher_id: String,
    #[serde(rename = "seller_id", default)]
    seller_id: String,
    #[serde(default)]
    bidfloor: f64,
}

impl Bidder for DmxAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        // DMX requires either a user ID / EIDs, or an app with an identifier.
        let has_user = request.user.as_ref().map(|u| !u.id.as_deref().unwrap_or("").is_empty()).unwrap_or(false);
        let has_app = request.app.is_some();
        if !has_user && !has_app {
            return (vec![], vec![BidderError::BadInput(
                "No user id or app id found. Could not send request to DMX.".to_string(),
            )]);
        }

        let mut errors = Vec::new();

        // Get params from first imp ext.
        let params: DmxParams = request.imp.first()
            .and_then(|imp| imp.ext.as_ref())
            .and_then(|ext| serde_json::from_str::<DmxExt>(ext.get()).ok())
            .map(|e| e.bidder)
            .unwrap_or_default();

        let seller_id = params.seller_id.clone();

        // Build a copy of the request with adjusted imp array.
        let mut req_copy = request.clone();

        // Process imps: set TagID, bidfloor, ensure banner/video exist.
        let mut imps = Vec::new();
        for imp in &request.imp {
            let imp_params: DmxParams = imp.ext.as_ref()
                .and_then(|ext| serde_json::from_str::<DmxExt>(ext.get()).ok())
                .map(|e| e.bidder)
                .unwrap_or_default();

            if imp_params.publisher_id.is_empty() && imp_params.member_id.is_empty() {
                errors.push(BidderError::BadInput("Missing Params for auction to be send".to_string()));
                return (vec![], errors);
            }

            let mut imp_copy = imp.clone();

            if imp_params.bidfloor != 0.0 {
                imp_copy.bidfloor = Some(imp_params.bidfloor);
            }

            let tag_id = if !imp_params.dmx_id.is_empty() {
                Some(imp_params.dmx_id.clone())
            } else if !imp_params.tag_id.is_empty() {
                Some(imp_params.tag_id.clone())
            } else {
                None
            };

            let Some(tid) = tag_id else {
                // No tagId — skip this imp.
                continue;
            };

            imp_copy.tagid = Some(tid);
            let secure: i8 = 1;
            imp_copy.secure = Some(secure);

            // Require banner with format, or video.
            let has_banner = imp.banner.as_ref().map(|b| !b.format.is_empty()).unwrap_or(false);
            let has_video = imp.video.is_some();

            if !has_banner && !has_video {
                continue;
            }

            imps.push(imp_copy);
        }

        req_copy.imp = imps;

        if req_copy.imp.is_empty() {
            return (vec![], errors);
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        // Append seller_id query param if present.
        let uri = if !seller_id.is_empty() {
            format!("{}?sellerid={}", self.endpoint, urlencoding_encode(&seller_id))
        } else {
            self.endpoint.clone()
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers,
                imp_ids: get_imp_ids(&req_copy.imp),
            }],
            errors,
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

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);

        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                let bid_type = internal
                    .imp
                    .iter()
                    .find(|i| i.id == bid.impid)
                    .map(|imp| {
                        if imp.banner.is_none() && imp.video.is_some() {
                            BidType::Video
                        } else {
                            BidType::Banner
                        }
                    })
                    .unwrap_or(BidType::Banner);

                // For video bids, wrap NURL into the AdM impression tag.
                if bid_type == BidType::Video {
                    if let (Some(adm), Some(nurl)) = (bid.adm.as_ref(), bid.nurl.as_ref()) {
                        let wrapped = adm.replace(
                            "</Impression>",
                            &format!("</Impression><Impression><![CDATA[{}]]></Impression>", nurl),
                        );
                        bid.adm = Some(wrapped);
                    }
                }

                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}

/// Percent-encode a string for use in a query parameter value.
fn urlencoding_encode(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~' => encoded.push(byte as char),
            b => encoded.push_str(&format!("%{:02X}", b)),
        }
    }
    encoded
}
