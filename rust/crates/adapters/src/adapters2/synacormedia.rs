use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, get_bid_type_from_imp};
use serde::{Deserialize, Serialize};

/// SynacorMedia is a deprecated alias for the `imds` bidder.
/// This implementation mirrors the `imds` Go adapter (adapters/imds/imds.go):
/// - validates each imp has seatId and tagId in ext
/// - sets imp.tagId from the ext tagId
/// - places seatId in request.ext
/// - builds the endpoint URL using the AccountID/SourceId macros
pub struct SynacormediaAdapter {
    pub endpoint: String,
}

impl SynacormediaAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

const ADAPTER_VERSION: &str = "pbs-go/1.0.0";

#[derive(Debug, Default, Deserialize)]
struct ExtImpImds {
    #[serde(rename = "seatId", default)]
    seat_id: String,
    #[serde(rename = "tagId", default)]
    tag_id: String,
}

#[derive(Debug, Serialize)]
struct ReqExt {
    #[serde(rename = "seatId")]
    seat_id: String,
}

fn build_endpoint_url(template: &str, account_id: &str, source_id: &str) -> String {
    template
        .replace("{{.AccountID}}", account_id)
        .replace("{{.SourceId}}", source_id)
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}

impl Bidder for SynacormediaAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors = Vec::new();
        let mut valid_imps: Vec<openrtb::Imp> = Vec::new();
        let mut first_ext_imp: Option<ExtImpImds> = None;

        for imp in &request.imp {
            // Parse bidder ext
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")).cloned() {
                Some(v) => v,
                None => {
                    errors.push(BidderError::BadInput("ext was not provided".to_string()));
                    continue;
                }
            };

            let ext_imp: ExtImpImds = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(e) => {
                    errors.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            if ext_imp.seat_id.is_empty() || ext_imp.tag_id.is_empty() {
                errors.push(BidderError::BadServerResponse("Invalid Impression".to_string()));
                continue;
            }

            let mut imp_copy = imp.clone();
            imp_copy.tagid = Some(ext_imp.tag_id.clone());

            if first_ext_imp.is_none() {
                first_ext_imp = Some(ExtImpImds {
                    seat_id: ext_imp.seat_id.clone(),
                    tag_id: ext_imp.tag_id.clone(),
                });
            }

            valid_imps.push(imp_copy);
        }

        if valid_imps.is_empty() {
            return (vec![], errors);
        }

        let first = match first_ext_imp {
            Some(f) => f,
            None => return (vec![], errors),
        };

        let req_ext = ReqExt { seat_id: first.seat_id.clone() };
        let req_ext_json = match serde_json::to_value(&req_ext) {
            Ok(v) => v,
            Err(e) => {
                errors.push(BidderError::BadInput(e.to_string()));
                return (vec![], errors);
            }
        };

        let mut req_copy = request.clone();
        req_copy.imp = valid_imps;
        req_copy.ext = Some(req_ext_json);

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => {
                errors.push(BidderError::BadInput(e.to_string()));
                return (vec![], errors);
            }
        };

        let encoded_seat = url_encode(&first.seat_id);
        let encoded_source = url_encode(ADAPTER_VERSION);
        let uri = build_endpoint_url(&self.endpoint, &encoded_seat, &encoded_source);

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let imp_ids = get_imp_ids(&req_copy.imp);
        (
            vec![RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers,
                imp_ids,
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
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(1);

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = internal
                    .imp
                    .iter()
                    .find(|i| i.id == bid.impid)
                    .map(get_bid_type_from_imp)
                    .unwrap_or(openrtb_ext::BidType::Banner);

                // Like Go imds: only accept banner and video bids
                if bid_type != openrtb_ext::BidType::Banner && bid_type != openrtb_ext::BidType::Video {
                    continue;
                }

                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}
