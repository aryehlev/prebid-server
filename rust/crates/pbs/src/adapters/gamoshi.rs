//! Go `adapters/gamoshi/gamoshi.go`.
#![allow(dead_code, unused_imports)]

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp};
use crate::ortb::Ext;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};



pub struct Adapter {
    uri: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { uri: endpoint.into() }
    }
}

#[derive(serde::Deserialize, Default)]
struct ExtImpGamoshi {
    #[serde(rename = "supplyPartnerId", default)]
    supply_partner_id: String,
}

fn add_header_if_non_empty(headers: &mut Header, name: &str, value: &str) {
    if !value.is_empty() {
        headers.add(name, value);
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = vec![];
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::bad_input("No impressions in the bid request")]);
        }

        // As of now, Gamoshi supports only banner and video impressions
        let mut request = request.clone();
        let mut valid_imp_exists = false;
        let mut kept: Vec<Imp> = Vec::with_capacity(request.imp.len());
        for mut imp in std::mem::take(&mut request.imp) {
            if let Some(banner) = imp.banner.as_mut() {
                if banner.w.is_none() && banner.h.is_none() && !banner.format.is_empty() {
                    let first = &banner.format[0];
                    banner.w = Some(first.w);
                    banner.h = Some(first.h);
                }
                valid_imp_exists = true;
                kept.push(imp);
            } else if imp.video.is_some() {
                valid_imp_exists = true;
                kept.push(imp);
            } else {
                errs.push(BidderError::bad_input(format!(
                    "Gamoshi only supports banner and video media types. Ignoring imp id={}",
                    imp.id
                )));
            }
        }
        request.imp = kept;

        if !valid_imp_exists {
            errs.push(BidderError::bad_input("No valid impression in the bid request"));
            return (vec![], errs);
        }

        let req_json = match crate::go_json::to_vec(&request) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::other(e.to_string()));
                return (vec![], errs);
            }
        };

        // Go returns only the ext errors here (earlier ignored-imp errors are dropped).
        let bidder_ext: ExtImpBidder = match unmarshal_ext(request.imp[0].ext.as_ref()) {
            Ok(e) => e,
            Err(_) => return (vec![], vec![BidderError::bad_input("ext.bidder not provided")]),
        };
        let gamoshi_ext: ExtImpGamoshi = match unmarshal_ext(bidder_ext.bidder.as_ref()) {
            Ok(e) => e,
            Err(_) => {
                return (vec![], vec![BidderError::bad_input("ext.bidder.supplyPartnerId not provided")])
            }
        };
        if gamoshi_ext.supply_partner_id.is_empty() {
            return (vec![], vec![BidderError::bad_input("supplyPartnerId is empty")]);
        }

        let mut this_uri = self.uri.clone();
        if this_uri.is_empty() {
            this_uri = "https://rtb.gamoshi.io".to_string();
        }
        let this_uri = format!("{this_uri}/r/{}/bidr?bidder=prebid-server", gamoshi_ext.supply_partner_id);

        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        headers.add("x-openrtb-version", "2.4");
        if let Some(device) = &request.device {
            add_header_if_non_empty(&mut headers, "User-Agent", &device.ua);
            add_header_if_non_empty(&mut headers, "X-Forwarded-For", &device.ip);
            add_header_if_non_empty(&mut headers, "Accept-Language", &device.language);
            if let Some(dnt) = device.dnt {
                add_header_if_non_empty(&mut headers, "DNT", &dnt.to_string());
            }
        }

        let data = RequestData {
            method: "POST".into(),
            uri: this_uri,
            body: req_json,
            headers,
            imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
        };
        (vec![data], vec![])
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response_data) {
            return (None, vec![]);
        }
        if response_data.status_code == 400 {
            return (
                None,
                vec![BidderError::bad_input(format!(
                    "Unexpected status code: {}. ",
                    response_data.status_code
                ))],
            );
        }
        if response_data.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "unexpected status code: {}. Run with request.debug = 1 for more info",
                    response_data.status_code
                ))],
            );
        }
        let unmarshaled: Result<BidResponse, BidderError> = if response_data.body.iter().all(|b| b" \t\r\n".contains(b)) {
            // Go (jsoniter) reads a NUL at EOF.
            Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()))
        } else {
            jsonutil::unmarshal(&response_data.body)
        };
        let bid_resp: BidResponse = match unmarshaled {
            Ok(r) => r,
            Err(e) => {
                return (None, vec![BidderError::bad_server_response(format!("bad server response: {e}. "))])
            }
        };

        // Go indexes `SeatBid[0]` and panics on an empty seatbid; report it instead.
        let Some(sb) = bid_resp.seatbid.into_iter().next() else {
            return (None, vec![BidderError::bad_server_response("no seatbid in the response")]);
        };
        let mut bid_response = BidderResponse::with_bids_capacity(sb.bid.len());
        for bid in sb.bid {
            let t = get_media_type(&bid.impid, &internal_request.imp);
            bid_response.bids.push(TypedBid::new(bid, t));
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type(imp_id: &str, imps: &[Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.video.is_some() {
                return BidType::Video;
            }
            return BidType::Banner;
        }
    }
    BidType::Banner
}

// ── local helpers (Go `adapters.ExtImpBidder`, `openrtb_ext.ExtBid`) ─────────────────────────

/// Go `adapters.ExtImpBidder` (only `bidder` is used here).
#[derive(serde::Deserialize, Default)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<Ext>,
}

/// Go `jsonutil.Unmarshal(raw, &v)` on a `json.RawMessage` field. An absent message is empty
/// input, which json-iterator rejects with the same `expect { or n, but found` text.
fn unmarshal_ext<T: serde::de::DeserializeOwned>(ext: Option<&Ext>) -> Result<T, BidderError> {
    match ext {
        Some(e) => jsonutil::unmarshal(e.to_json().as_bytes()),
        None => Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string())),
    }
}

/// Go: unmarshal `imp.ext` into `ExtImpBidder`, then `.Bidder` into `T`; the two errors are
/// returned separately so adapters can wrap them differently.
fn parse_imp_ext<T: serde::de::DeserializeOwned>(
    imp: &Imp,
    wrap_bidder_ext: impl Fn(BidderError) -> BidderError,
    wrap_params: impl Fn(BidderError) -> BidderError,
) -> Result<T, BidderError> {
    let bidder_ext: ExtImpBidder = unmarshal_ext(imp.ext.as_ref()).map_err(wrap_bidder_ext)?;
    unmarshal_ext(bidder_ext.bidder.as_ref()).map_err(wrap_params)
}

/// Go `openrtb_ext.ExtBid` (`bid.ext.prebid.type`).
#[derive(serde::Deserialize, Default)]
struct ExtBid {
    #[serde(default)]
    prebid: Option<ExtBidPrebid>,
}

#[derive(serde::Deserialize, Default)]
struct ExtBidPrebid {
    #[serde(rename = "type", default)]
    bid_type: String,
}

/// Go `jsonutil.Unmarshal(bid.Ext, &ExtBid)`: `Err` on a malformed ext.
fn parse_bid_ext(bid: &Bid) -> Option<Result<ExtBid, BidderError>> {
    bid.ext.as_ref().map(|e| jsonutil::unmarshal(e.to_json().as_bytes()))
}

/// Go `strconv.FormatFloat(price, 'f', -1, 64)`.
fn format_price(price: f64) -> String {
    format!("{price}")
}

/// Go `template.New("endpointTemplate").Parse(endpoint)`. `{{Malformed}}` is a Go parse error
/// (unknown function), which `EndpointTemplate::parse` only reports at resolve time, so a dry
/// run with empty params catches it here.
fn parse_template(endpoint: &str) -> Result<EndpointTemplate, String> {
    let t = EndpointTemplate::parse(endpoint)?;
    t.resolve(&EndpointTemplateParams::default())?;
    Ok(t)
}
