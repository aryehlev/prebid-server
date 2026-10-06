//! Go `adapters/richaudience/richaudience.go`.

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
use crate::ortb::openrtb2::{BidRequest, BidResponse, Device, Imp};
use crate::ortb::Ext;

/// Go `openrtb_ext.ExtImpRichaudience`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtImpRichaudience {
    pid: String,
    #[serde(rename = "bidfloorcur")]
    bid_floor_cur: String,
    test: bool,
}

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
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

fn set_headers(h: &mut Header) {
    h.set("Content-Type", "application/json;charset=utf-8");
    h.set("Accept", "application/json");
    h.add("X-Openrtb-Version", "2.5");
}

/// Go `getIsUrlSecure`: also fills `site.domain` from the page host when empty.
fn get_is_url_secure(request: &mut BidRequest) -> bool {
    let mut secure = false;
    if let Some(site) = request.site.as_mut() {
        if !site.page.is_empty() {
            if let Ok(u) = url::Url::parse(&site.page) {
                if site.domain.is_empty() {
                    // Go `URL.Host` keeps the port.
                    site.domain = match (u.host_str(), u.port()) {
                        (Some(h), Some(p)) => format!("{h}:{p}"),
                        (Some(h), None) => h.to_string(),
                        _ => String::new(),
                    };
                }
                secure = u.scheme() == "https";
            }
        }
    }
    secure
}

fn parse_imp_ext(imp: &Imp) -> Result<ExtImpRichaudience, BidderError> {
    imp_bidder_params::<ExtImpRichaudience>(imp.ext.as_ref()).map_err(|_| {
        // Go reports "not found parameters" when the outer ext fails and "invalid parameters" when
        // the bidder part fails.
        let outer_ok = imp.ext.as_ref().is_some_and(|e| e.0.is_object() || e.0.is_null());
        if outer_ok && imp.ext.as_ref().is_some_and(|e| e.0.get("bidder").is_some()) {
            BidderError::bad_input(format!("invalid parameters ext in ImpID: {}", imp.id))
        } else if outer_ok {
            // `bidder` absent: unmarshalling an empty RawMessage fails in the second step.
            BidderError::bad_input(format!("invalid parameters ext in ImpID: {}", imp.id))
        } else {
            BidderError::bad_input(format!("not found parameters ext in ImpID : {}", imp.id))
        }
    })
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut out = Vec::new();
        let mut errs = Vec::with_capacity(request.imp.len());
        let mut rai_headers = Header::new();
        set_headers(&mut rai_headers);

        // Go mutates `request` in place across the loop (site/app keywords, device, test), so the
        // changes carry over to later imps; keep that on a private copy.
        let mut request = request.clone();
        let is_url_secure = get_is_url_secure(&mut request);

        if let Some(d) = &request.device {
            if d.ip.is_empty() && d.ipv6.is_empty() {
                errs.push(BidderError::bad_input("request.Device.IP is required"));
                return (vec![], errs);
            }
        }

        let imps = request.imp.clone();
        for mut imp in imps {
            let rai_ext = match parse_imp_ext(&imp) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };

            if let Some(app) = request.app.as_mut() {
                app.keywords = format!("tagid={}", imp.tagid);
            }
            if let Some(site) = request.site.as_mut() {
                site.keywords = format!("tagid={}", imp.tagid);
            }

            if !rai_ext.pid.is_empty() {
                imp.tagid = rai_ext.pid.clone();
            }
            if rai_ext.test {
                let device = request.device.get_or_insert_with(Device::default);
                device.ip = "11.222.33.44".to_string();
                request.test = 1;
            }
            if !rai_ext.bid_floor_cur.is_empty() {
                imp.bidfloorcur = rai_ext.bid_floor_cur.clone();
            } else if imp.bidfloorcur.is_empty() {
                imp.bidfloorcur = "USD".to_string();
            }

            imp.secure = Some(if is_url_secure { 1 } else { 0 });

            if let Some(banner) = &imp.banner {
                if banner.w.is_none() && banner.h.is_none() && banner.format.is_empty() {
                    errs.push(BidderError::bad_input("request.Banner.Format is required"));
                    continue;
                }
            }
            if let Some(video) = &imp.video {
                if video.w.unwrap_or_default() == 0 || video.h.unwrap_or_default() == 0 {
                    errs.push(BidderError::bad_input("request.Video.Sizes is required"));
                    continue;
                }
            }

            request.imp = vec![imp];
            let body = match crate::go_json::to_vec(&request) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::bad_input(e.to_string()));
                    continue;
                }
            };
            out.push(RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers: rai_headers.clone(),
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
            });
        }
        (out, errs)
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response_data) {
            return (None, vec![]);
        }
        if response_data.status_code == 400 {
            return (
                None,
                vec![BidderError::bad_input(
                    "Unexpected status code: 400. Bad request from publisher. Run with request.debug = 1 for more info.",
                )],
            );
        }
        if response_data.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {}. Run with request.debug = 1 for more info.",
                    response_data.status_code
                ))],
            );
        }

        // Go re-parses the outgoing request body here.
        let bid_req: BidRequest = match jsonutil::unmarshal(&request_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![BidderError::bad_server_response(e.to_string())]),
        };
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![BidderError::bad_server_response(e.to_string())]),
        };

        let mut out = BidderResponse::with_bids_capacity(bid_req.imp.len());
        out.currency = bid_resp.cur.clone();

        for req_bid in &bid_req.imp {
            for seat_bid in &bid_resp.seatbid {
                for b in &seat_bid.bid {
                    let mut bid = b.clone();
                    // Go yields the string "no bidtype assigned" for a bid on another imp; `BidType`
                    // cannot hold that, so such a bid is typed as banner here.
                    let bid_type = get_media_type(&bid.impid, req_bid);
                    if bid_type == Some(BidType::Video) {
                        // Go dereferences `*reqBid.Video.W` / `.H` (set, as make_requests requires them).
                        if let Some(v) = &req_bid.video {
                            bid.w = v.w.unwrap_or_default();
                            bid.h = v.h.unwrap_or_default();
                        }
                    }
                    out.bids.push(TypedBid::new(bid, bid_type.unwrap_or(BidType::NoBidTypeAssigned)));
                }
            }
        }
        (Some(out), vec![])
    }
}

fn get_media_type(imp_id: &str, imp: &Imp) -> Option<BidType> {
    if imp.id == imp_id {
        if imp.video.is_some() {
            return Some(BidType::Video);
        }
        return Some(BidType::Banner);
    }
    None
}
