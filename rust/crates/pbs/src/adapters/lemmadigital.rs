//! Go `adapters/lemmadigital/lemmadigital.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder,
    BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{BidRequest, BidResponse};
use crate::ortb::Ext;

pub struct Adapter {
    endpoint: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: &str) -> Result<Self, String> {
        let endpoint = EndpointTemplate::parse(endpoint)
            .map_err(|e| format!("unable to parse endpoint url template: {e}"))?;
        Ok(Self { endpoint })
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ImpExtLemmaDigital {
    pid: i64,
    aid: i64,
}

fn ext_text(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

/// `jsonutil::unmarshal`, plus json-iterator's wording for empty input (Go reports the NUL byte
/// it reads at EOF; `jsonutil::unmarshal` falls through to serde's "EOF while parsing").
fn unmarshal_obj<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T, BidderError> {
    if data.iter().all(|b| b" \t\r\n".contains(b)) {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()));
    }
    jsonutil::unmarshal(data)
}

/// Go reports a wrongly typed `pid` / `aid` as `cannot unmarshal <Go path>: ...`; serde's
/// wording is replaced by the Go path (the trailing json-iterator detail is approximated).
fn bidder_params_error(data: &[u8], serde_err: BidderError) -> String {
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(data) {
        for (key, field) in [("pid", "PublisherId"), ("aid", "AdId")] {
            if let Some(val) = v.get(key) {
                if !val.is_i64() && !val.is_u64() {
                    return format!(
                        "cannot unmarshal openrtb_ext.ImpExtLemmaDigital.{field}: unexpected character"
                    );
                }
            }
        }
    }
    serde_err.to_string()
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let Some(first) = request.imp.first() else {
            return (vec![], vec![BidderError::other("Impression array should not be empty")]);
        };
        let bidder_ext: ExtImpBidder = match unmarshal_obj(&ext_text(&first.ext)) {
            Ok(e) => e,
            Err(e) => {
                return (
                    vec![],
                    vec![BidderError::bad_input(format!(
                        "Invalid imp.ext for impression index {}. Error Infomation: {}",
                        0, e
                    ))],
                )
            }
        };
        let bidder_bytes = ext_text(&bidder_ext.bidder);
        let imp_ext: ImpExtLemmaDigital = match unmarshal_obj(&bidder_bytes) {
            Ok(e) => e,
            Err(e) => {
                return (
                    vec![],
                    vec![BidderError::bad_input(format!(
                        "Invalid imp.ext.bidder for impression index {}. Error Infomation: {}",
                        0,
                        bidder_params_error(&bidder_bytes, e)
                    ))],
                )
            }
        };
        let params = EndpointTemplateParams {
            publisher_id: imp_ext.pid.to_string(),
            ad_unit: imp_ext.aid.to_string(),
            ..Default::default()
        };
        let endpoint = match self.endpoint.resolve(&params) {
            Ok(e) => e,
            Err(e) => return (vec![], vec![BidderError::other(e)]),
        };
        let body = match crate::go_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        (
            vec![RequestData {
                method: "POST".into(),
                uri: endpoint,
                body,
                headers: Header::new(),
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response_data) {
            return (None, vec![]);
        }
        if let Some(err) = check_response_status_code_for_errors(response_data) {
            return (None, vec![err]);
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        // Go indexes `request.Imp[0]` and panics on an empty imp list; report an error instead.
        let Some(first) = request.imp.first() else {
            return (None, vec![BidderError::other("Impression array should not be empty")]);
        };
        let bid_type = if first.video.is_some() { BidType::Video } else { BidType::Banner };
        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        if !response.cur.is_empty() {
            bid_response.currency = response.cur.clone();
        }
        if let Some(sb) = response.seatbid.into_iter().next() {
            for bid in sb.bid {
                bid_response.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        (Some(bid_response), vec![])
    }
}
