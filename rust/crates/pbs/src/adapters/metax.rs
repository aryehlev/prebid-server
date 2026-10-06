//! Go `adapters/metax/metax.go`.

use serde::Deserialize;

use crate::bid_types::{BidType, ExtBidPrebidVideo};
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder,
    BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{Banner, Bid, BidRequest, BidResponse, Imp, MarkupType};
use crate::ortb::Ext;

pub struct Adapter {
    template: EndpointTemplate,
}

/// Go `openrtb_ext.ExtImpMetaX`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpMetaX {
    #[serde(rename = "publisherid")]
    publisher_id: i64,
    adunit: i64,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Result<Self, BidderError> {
        let endpoint = endpoint.into();
        if endpoint.is_empty() {
            return Err(BidderError::other("endpoint is empty"));
        }
        check_template(&endpoint).map_err(|e| BidderError::other(format!("unable to parse endpoint: {e}")))?;
        let t = EndpointTemplate::parse(&endpoint)
            .map_err(|e| BidderError::other(format!("unable to parse endpoint: {e}")))?;
        Ok(Self { template: t })
    }

    fn get_endpoint(&self, ext: &ExtImpMetaX) -> Result<String, BidderError> {
        let params = EndpointTemplateParams {
            publisher_id: ext.publisher_id.to_string(),
            ad_unit: ext.adunit.to_string(),
            ..Default::default()
        };
        self.template.resolve(&params).map_err(BidderError::other)
    }
}

/// Go's `template.Parse` rejects `{{}}` and bare identifiers (undefined functions) but accepts
/// unknown `.Field`s (those fail when the template runs).
fn check_template(endpoint: &str) -> Result<(), String> {
    let t = EndpointTemplate::parse(endpoint)?;
    match t.resolve(&EndpointTemplateParams::default()) {
        Err(e) if !e.contains("function \".") => Err(e),
        _ => Ok(()),
    }
}

fn parse_bidder_ext(imp: &Imp) -> Result<ExtImpMetaX, BidderError> {
    let bidder_ext: ExtImpBidder = unmarshal_ext(imp.ext.as_ref())?;
    unmarshal_ext(bidder_ext.bidder.as_ref()).map_err(|_| BidderError::other("Wrong MetaX bidder ext"))
}

fn assign_banner_size(banner: &mut Banner) {
    if banner.w.is_some() && banner.h.is_some() {
        return;
    }
    if banner.format.is_empty() {
        return;
    }
    banner.w = Some(banner.format[0].w);
    banner.h = Some(banner.format[0].h);
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut req_datas = Vec::with_capacity(request.imp.len());
        for imp in &request.imp {
            let metax = match parse_bidder_ext(imp) {
                Ok(m) => m,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            let mut imp = imp.clone();
            if let Some(b) = imp.banner.as_mut() {
                assign_banner_size(b);
            }
            let endpoint = match self.get_endpoint(&metax) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            let mut request_copy = request.clone();
            request_copy.imp = vec![imp];
            let body = match crate::go_json::to_vec(&request_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::other(e.to_string()));
                    return (vec![], errs);
                }
            };
            let mut headers = Header::new();
            headers.add("Content-Type", "application/json;charset=utf-8");
            headers.add("Accept", "application/json");
            req_datas.push(RequestData {
                method: "POST".into(),
                uri: endpoint,
                body,
                headers,
                imp_ids: request_copy.imp.iter().map(|i| i.id.clone()).collect(),
            });
        }
        (req_datas, errs)
    }

    fn make_bids(
        &self,
        _bid_req: &BidRequest,
        _req_data: &RequestData,
        resp_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(resp_data) {
            return (None, vec![]);
        }
        if let Some(err) = check_response_status_code_for_errors(resp_data) {
            return (None, vec![err]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&resp_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        if bid_resp.seatbid.is_empty() || bid_resp.seatbid[0].bid.is_empty() {
            return (None, vec![]);
        }
        let mut resp = BidderResponse::with_bids_capacity(bid_resp.seatbid[0].bid.len());
        if !bid_resp.cur.is_empty() {
            resp.currency = bid_resp.cur.clone();
        }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = match get_bid_type(&bid) {
                    Ok(t) => t,
                    Err(e) => return (None, vec![e]),
                };
                let video = get_bid_video(&bid);
                let mut tb = TypedBid::new(bid, bid_type);
                tb.bid_video = Some(video);
                resp.bids.push(tb);
            }
        }
        (Some(resp), vec![])
    }
}

fn get_bid_type(bid: &Bid) -> Result<BidType, BidderError> {
    match bid.mtype {
        MarkupType::BANNER => Ok(BidType::Banner),
        MarkupType::VIDEO => Ok(BidType::Video),
        MarkupType::NATIVE => Ok(BidType::Native),
        MarkupType::AUDIO => Ok(BidType::Audio),
        m => Err(BidderError::bad_server_response(format!("Unsupported MType {}", m.0))),
    }
}

fn get_bid_video(bid: &Bid) -> ExtBidPrebidVideo {
    let mut v = ExtBidPrebidVideo::default();
    if let Some(c) = bid.cat.first() {
        v.primary_category = c.clone();
    }
    if bid.dur > 0 {
        v.duration = bid.dur as i32;
    }
    v
}

// ---- local helpers (shared foundation untouched) ----

/// Go `jsonutil.Unmarshal(ext, &T)`: json-iterator matches keys case-insensitively; a non-object
/// (other than null) reports `expect { or n, but found X`; absent ext is empty input.
fn unmarshal_ext<T: serde::de::DeserializeOwned + Default>(ext: Option<&Ext>) -> Result<T, BidderError> {
    use sonic_rs::JsonValueTrait;
    let Some(ext) = ext else {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into()));
    };
    if ext.0.is_null() {
        return Ok(T::default());
    }
    let text = ext.to_json();
    if !ext.0.is_object() {
        let c = text.chars().next().unwrap_or('\u{0}');
        return Err(BidderError::FailedToUnmarshal(format!("expect {{ or n, but found {c}")));
    }
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text)
        .map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))?;
    let lowered: serde_json::Map<String, serde_json::Value> =
        map.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect();
    serde_json::from_value(serde_json::Value::Object(lowered))
        .map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}
