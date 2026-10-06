//! Go `adapters/amx/amx.go`.

use crate::bid_types::{BidType, ExtBidPrebidMeta};
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Publisher};
use serde::Deserialize;

const NBR_HEADER_NAME: &str = "x-nbr";
const ADAPTER_VERSION: &str = "pbs1.2";

/// Go `openrtb_ext.ExtImpAMX`.
#[derive(Debug, Default, Deserialize)]
struct ExtImpAmx {
    #[serde(rename = "tagId", default)]
    tag_id: String,
    #[serde(rename = "adUnitId", default)]
    ad_unit_id: String,
}

#[derive(Debug, Default, Deserialize)]
struct AmxExt {
    #[serde(default)]
    bidder: ExtImpAmx,
}

#[derive(Debug, Default, Deserialize)]
struct AmxBidExt {
    #[serde(default)]
    startdelay: Option<i64>,
    #[serde(rename = "ct", default)]
    creative_type: Option<i64>,
    #[serde(rename = "ds", default)]
    demand_source: Option<String>,
    #[serde(rename = "bc", default)]
    bidder_code: Option<String>,
}
/// Go `jsonutil.Unmarshal(ext, &target)` on a `json.RawMessage`; the failure text is the
/// json-iterator top-level one (`expect { or n, but found X`).
fn decode_ext<T: serde::de::DeserializeOwned>(ext: Option<&crate::ortb::Ext>) -> Result<T, BidderError> {
    let Some(ext) = ext else {
        // Go: Unmarshal of an empty RawMessage fails (never happens: PBS core validates imp.ext).
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into()));
    };
    use sonic_rs::JsonValueTrait;
    // json-iterator picks the object decoder from the first byte: anything but `{` / `null`
    // fails with its top-level message, while serde would accept an array for a struct.
    if ext.0.is_object() || ext.0.is_null() {
        if let Ok(v) = ext.decode::<T>() {
            return Ok(v);
        }
    }
    jsonutil::unmarshal::<T>(ext.to_json().as_bytes())
}
/// Go `adapters.ExtImpBidder` (only `bidder` is used).
#[derive(Debug, Default, serde::Deserialize)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<crate::ortb::Ext>,
}
pub struct Adapter {
    endpoint: String,
}

/// Go `url.ParseQuery` fails on a bad percent-escape (`%gh`) or a semicolon.
fn parse_query_checked(q: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for pair in q.split('&') {
        if pair.is_empty() {
            continue;
        }
        if pair.contains(';') {
            return Err(format!("invalid semicolon separator in query"));
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        for part in [k, v] {
            let b = part.as_bytes();
            let mut i = 0;
            while i < b.len() {
                if b[i] == b'%' {
                    if i + 2 >= b.len() + 0 && !(i + 2 < b.len() + 1 && i + 2 <= b.len() - 1 + 0) {
                        // fallthrough to the check below
                    }
                    let ok = i + 2 < b.len() + 0 + 1
                        && b.get(i + 1).is_some_and(u8::is_ascii_hexdigit)
                        && b.get(i + 2).is_some_and(u8::is_ascii_hexdigit);
                    if !ok {
                        return Err(format!("invalid URL escape \"{}\"", &part[i..(i + 3).min(part.len())]));
                    }
                    i += 3;
                } else {
                    i += 1;
                }
            }
        }
        let dec = |s: &str| url::form_urlencoded::parse(s.as_bytes()).next().map(|(k, _)| k.into_owned()).unwrap_or_default();
        let key = dec(k);
        let value = url::form_urlencoded::parse(format!("x={v}").as_bytes())
            .next()
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default();
        out.push((key, value));
    }
    Ok(out)
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: &str) -> Result<Self, String> {
        // Go's url.Parse rejects leading whitespace / control characters; the url crate trims them.
        if endpoint.chars().any(|c| c.is_control()) || endpoint.starts_with(' ') {
            return Err(format!("invalid endpoint: parse \"{endpoint}\": first path segment in URL cannot contain colon"));
        }
        let mut u = url::Url::parse(endpoint).map_err(|e| format!("invalid endpoint: {e}"))?;
        let mut qs = parse_query_checked(u.query().unwrap_or(""))
            .map_err(|e| format!("invalid query parameters in the endpoint: {e}"))?;
        qs.push(("v".into(), ADAPTER_VERSION.into()));
        // Go url.Values.Encode sorts by key (stable within a key).
        qs.sort_by(|a, b| a.0.cmp(&b.0));
        let q = url::form_urlencoded::Serializer::new(String::new()).extend_pairs(qs).finish();
        u.set_query(Some(&q));
        Ok(Self { endpoint: u.to_string() })
    }
}

fn ensure_publisher_with_id(pub_: Option<&Publisher>, publisher_id: &str) -> Publisher {
    match pub_ {
        None => Publisher { id: publisher_id.to_string(), ..Default::default() },
        Some(p) => {
            let mut c = p.clone();
            c.id = publisher_id.to_string();
            c
        }
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req_copy = request.clone();
        let mut publisher_id = String::new();
        for imp in req_copy.imp.iter_mut() {
            if let Some(ext) = imp.ext.as_ref() {
                if let Ok(params) = decode_ext::<AmxExt>(Some(ext)) {
                    if !params.bidder.tag_id.is_empty() {
                        publisher_id = params.bidder.tag_id;
                    }
                    // if it has an adUnitId, set as the tagid
                    if !params.bidder.ad_unit_id.is_empty() {
                        imp.tagid = params.bidder.ad_unit_id;
                    }
                }
            }
        }
        if !publisher_id.is_empty() {
            if let Some(app) = req_copy.app.as_mut() {
                app.publisher = Some(ensure_publisher_with_id(app.publisher.as_ref(), &publisher_id));
            }
            if let Some(site) = req_copy.site.as_mut() {
                site.publisher = Some(ensure_publisher_with_id(site.publisher.as_ref(), &publisher_id));
            }
        }
        let encoded = match crate::go_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body: encoded,
                headers,
                imp_ids: req_copy.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        _external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        let mut errs = Vec::new();
        if response.status_code == 204 {
            return (None, vec![]);
        }
        if response.status_code == 400 {
            let nbr = response.headers.get(NBR_HEADER_NAME);
            return (None, vec![BidderError::bad_input(format!("Invalid Request: 400. Error Code: {nbr}"))]);
        }
        if response.status_code != 200 {
            let nbr = response.headers.get(NBR_HEADER_NAME);
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected response: {}. Error Code: {nbr}",
                    response.status_code
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(5);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_ext = match get_bid_ext(bid.ext.as_ref()) {
                    Ok(e) => e,
                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                };
                let demand_source = bid_ext.demand_source.clone().unwrap_or_default();
                let bid_type = get_media_type_for_bid(&bid_ext);
                let mut b = TypedBid::new(bid.clone(), bid_type);
                b.bid_meta = Some(ExtBidPrebidMeta {
                    advertiser_domains: bid.adomain.clone(),
                    demand_source,
                    ..Default::default()
                });
                if let Some(code) = bid_ext.bidder_code {
                    b.seat = code;
                }
                bid_response.bids.push(b);
            }
        }
        (Some(bid_response), errs)
    }
}

fn get_bid_ext(ext: Option<&crate::ortb::Ext>) -> Result<AmxBidExt, BidderError> {
    if ext.is_none() {
        return Ok(AmxBidExt::default());
    }
    decode_ext(ext)
}

fn get_media_type_for_bid(bid_ext: &AmxBidExt) -> BidType {
    if bid_ext.startdelay.is_some() {
        return BidType::Video;
    }
    if bid_ext.creative_type == Some(10) {
        return BidType::Native;
    }
    BidType::Banner
}
