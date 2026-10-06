//! Go `adapters/axonix/axonix.go`.

use serde::Deserialize;
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{
    is_response_status_code_no_content, Bidder, BidderResponse, ExtraRequestInfo, RequestData,
    ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

/// Go `openrtb_ext.ExtImpAxonix`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtImpAxonix {
    #[serde(rename = "supplyId")]
    supply_id: String,
}

pub struct Adapter {
    endpoint_template: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: &str) -> Result<Self, BidderError> {
        let t = EndpointTemplate::parse(endpoint)
            .map_err(|e| BidderError::other(format!("unable to parse endpoint url template: {e}")))?;
        Ok(Self { endpoint_template: t })
    }

    fn get_endpoint(&self, ext: &ExtImpAxonix) -> Result<String, BidderError> {
        let p = EndpointTemplateParams { account_id: path_escape(&ext.supply_id), ..Default::default() };
        self.endpoint_template.resolve(&p).map_err(BidderError::other)
    }
}

/// Go: `jsonutil.Unmarshal(imp.Ext, &ExtImpBidder)` then `jsonutil.Unmarshal(bidderExt.Bidder, &T)`.
/// Returns the Go error message text of whichever step fails.
fn imp_bidder_params<T: serde::de::DeserializeOwned>(ext: Option<&Ext>) -> Result<T, String> {
    fn not_obj(v: &sonic_rs::Value) -> Option<String> {
        if v.is_object() || v.is_null() {
            return None;
        }
        let first = sonic_rs::to_string(v).ok()?.chars().next().unwrap_or('\u{0}');
        Some(format!("expect {{ or n, but found {first}"))
    }
    let Some(ext) = ext else {
        return Err("expect { or n, but found \u{0}".to_string());
    };
    if let Some(m) = not_obj(&ext.0) {
        return Err(m);
    }
    match ext.0.get("bidder") {
        None => Err("expect { or n, but found \u{0}".to_string()),
        Some(b) => {
            if let Some(m) = not_obj(b) {
                return Err(m);
            }
            sonic_rs::from_value::<T>(b).map_err(|e| e.to_string())
        }
    }
}

/// Go `url.PathEscape`.
fn path_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b'$' | b'&' | b'+' | b'=' | b':' | b'@' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        // Go indexes `request.Imp[0]` and panics on an empty imp list.
        let Some(imp0) = request.imp.first() else {
            return (vec![], vec![BidderError::bad_input("no imps in the request")]);
        };
        let ext: ExtImpAxonix = match imp_bidder_params(imp0.ext.as_ref()) {
            Ok(e) => e,
            Err(m) => return (vec![], vec![BidderError::bad_input(m)]),
        };
        let endpoint = match self.get_endpoint(&ext) {
            Ok(e) => e,
            Err(e) => return (vec![], vec![e]),
        };
        let body = match crate::go_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: endpoint,
                body,
                headers,
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
        if response_data.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {}.",
                    response_data.status_code
                ))],
            );
        }
        let response: BidResponse = match unmarshal_resp(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        bid_response.currency = response.cur;
        for seat_bid in response.seatbid {
            for mut bid in seat_bid.bid {
                resolve_macros(&mut bid);
                let t = get_media_type(&bid.impid, &request.imp);
                bid_response.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type(imp_id: &str, imps: &[Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.native.is_some() {
                return BidType::Native;
            } else if imp.video.is_some() {
                return BidType::Video;
            }
            return BidType::Banner;
        }
    }
    BidType::Banner
}

/// Go `strconv.FormatFloat(price, 'f', -1, 64)`.
fn format_price(p: f64) -> String {
    format!("{p}")
}

fn resolve_macros(bid: &mut Bid) {
    let price = format_price(bid.price);
    bid.nurl = bid.nurl.replace("${AUCTION_PRICE}", &price);
    bid.adm = bid.adm.replace("${AUCTION_PRICE}", &price);
}

/// Go `jsonutil.Unmarshal` into a response; an empty body is jsoniter's `expect { or n, but found \x00`
/// (the shared `jsonutil::unmarshal` reports serde's EOF text there).
fn unmarshal_resp<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, BidderError> {
    if body.iter().all(|b| b" \t\r\n".contains(b)) {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()));
    }
    jsonutil::unmarshal(body)
}
