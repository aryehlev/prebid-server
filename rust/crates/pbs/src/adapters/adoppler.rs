//! Go `adapters/adoppler/adoppler.go`.

#![allow(unused_imports, dead_code)]

use serde::{Deserialize, Serialize};

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp, MarkupType};
use crate::ortb::Ext;

/// Go `adapters.ExtImpBidder`.
#[derive(Deserialize, Default)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<Ext>,
}

fn ext_bytes(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

/// `Unmarshal(imp.Ext, &bidderExt)` then `Unmarshal(bidderExt.Bidder, &params)`.
fn parse_bidder_ext<T: serde::de::DeserializeOwned>(imp: &Imp) -> Result<T, BidderError> {
    let outer: ExtImpBidder = jsonutil::unmarshal(&ext_bytes(&imp.ext))?;
    jsonutil::unmarshal(&ext_bytes(&outer.bidder))
}

fn json_headers() -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");
    headers
}

use crate::bid_types::ExtBidPrebidVideo;

/// Go `jsonutil.Unmarshal` of an absent `json.RawMessage`: jsoniter reports a NUL byte.
fn unmarshal_raw<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T, BidderError> {
    if data.iter().all(|b| b" \t\r\n".contains(b)) {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()));
    }
    jsonutil::unmarshal(data)
}

const DEFAULT_CLIENT: &str = "app";

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpAdoppler {
    client: String,
    adunit: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct AdsVideoExt {
    duration: i64,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct AdsImpExt {
    video: Option<AdsVideoExt>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidExtAds {
    ads: Option<AdsImpExt>,
}

pub struct Adapter {
    endpoint: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl AsRef<str>) -> Result<Self, BidderError> {
        let endpoint = EndpointTemplate::parse(endpoint.as_ref())
            .map_err(|e| BidderError::other(format!("unable to parse endpoint url template: {e}")))?;
        Ok(Self { endpoint })
    }

    fn bid_uri(&self, ext: &ExtImpAdoppler) -> Result<String, String> {
        let account_id = if ext.client.is_empty() { DEFAULT_CLIENT.to_string() } else { path_escape(&ext.client) };
        let params = EndpointTemplateParams { ad_unit: path_escape(&ext.adunit), account_id, ..Default::default() };
        self.endpoint.resolve(&params)
    }
}

/// Go `url.PathEscape`.
fn path_escape(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b'$' | b'&' | b'+' | b'=' | b':' | b'@' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn unmarshal_ext(imp: &Imp) -> Result<ExtImpAdoppler, BidderError> {
    let outer: ExtImpBidder = unmarshal_raw(&ext_bytes(&imp.ext))?;
    let ext: ExtImpAdoppler = unmarshal_raw(&ext_bytes(&outer.bidder))?;
    if ext.adunit.is_empty() {
        return Err(BidderError::other("$.imp.ext.adoppler.adunit required"));
    }
    Ok(ext)
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        req: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if req.imp.is_empty() {
            return (vec![], vec![]);
        }
        let mut datas = Vec::new();
        let mut errs = Vec::new();
        let mut headers = Header::new();
        headers.add("Accept", "application/json");
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("X-OpenRTB-Version", "2.5");
        let mut r = req.clone();
        for imp in &req.imp {
            let ext = match unmarshal_ext(imp) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(BidderError::bad_input(e.to_string()));
                    continue;
                }
            };
            r.id = format!("{}-{}", req.id, ext.adunit);
            r.imp = vec![imp.clone()];
            let body = match crate::go_json::to_vec(&r) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::other(e.to_string()));
                    continue;
                }
            };
            let uri = match self.bid_uri(&ext) {
                Ok(u) => u,
                Err(e) => {
                    errs.push(BidderError::bad_input(format!("Unable to build bid URI: {e}")));
                    continue;
                }
            };
            datas.push(RequestData {
                method: "POST".into(),
                uri,
                body,
                headers: headers.clone(),
                imp_ids: r.imp.iter().map(|i| i.id.clone()).collect(),
            });
        }
        (datas, errs)
    }

    fn make_bids(
        &self,
        int_req: &BidRequest,
        _ext_req: &RequestData,
        resp: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if resp.status_code == 204 {
            return (None, vec![]);
        }
        if resp.status_code == 400 {
            return (None, vec![BidderError::bad_input("bad request")]);
        }
        if resp.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!("unexpected status: {}", resp.status_code))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&resp.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![BidderError::bad_server_response(format!("invalid body: {e}"))]),
        };

        let mut imp_types: std::collections::HashMap<&str, BidType> = std::collections::HashMap::new();
        for imp in &int_req.imp {
            if imp_types.contains_key(imp.id.as_str()) {
                return (None, vec![BidderError::bad_input(format!("duplicate $.imp.id {}", imp.id))]);
            }
            let t = if imp.banner.is_some() {
                BidType::Banner
            } else if imp.video.is_some() {
                BidType::Video
            } else if imp.audio.is_some() {
                BidType::Audio
            } else if imp.native.is_some() {
                BidType::Native
            } else {
                return (
                    None,
                    vec![BidderError::bad_input(
                        "one of $.imp.banner, $.imp.video, $.imp.audio and $.imp.native field required",
                    )],
                );
            };
            imp_types.insert(imp.id.as_str(), t);
        }

        let mut bids = Vec::new();
        for seat_bid in bid_resp.seatbid {
            for bid in seat_bid.bid {
                let Some(&tp) = imp_types.get(bid.impid.as_str()) else {
                    return (None, vec![BidderError::bad_server_response(format!("unknown impid: {}", bid.impid))]);
                };
                let mut bid_video = None;
                if tp == BidType::Video {
                    let ads_ext: Option<AdsImpExt> =
                        match unmarshal_raw::<BidExtAds>(&ext_bytes(&bid.ext)) {
                            Ok(e) => e.ads,
                            Err(e) => return (None, vec![BidderError::bad_server_response(e.to_string())]),
                        };
                    let Some(video) = ads_ext.and_then(|a| a.video) else {
                        return (
                            None,
                            vec![BidderError::bad_server_response("$.seatbid.bid.ext.ads.video required")],
                        );
                    };
                    bid_video = Some(ExtBidPrebidVideo {
                        duration: video.duration as i32,
                        primary_category: bid.cat.first().cloned().unwrap_or_default(),
                    });
                }
                let mut typed = TypedBid::new(bid, tp);
                typed.bid_video = bid_video;
                bids.push(typed);
            }
        }
        let mut out = BidderResponse::with_bids_capacity(bids.len());
        out.bids = bids;
        (Some(out), vec![])
    }
}
