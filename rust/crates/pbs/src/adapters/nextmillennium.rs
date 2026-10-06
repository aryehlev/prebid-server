//! Go `adapters/nextmillennium/nextmillennium.go`.

use serde::{Deserialize, Serialize};

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::config;
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, MarkupType};
use crate::ortb::Ext;

const NM_ADAPTER_VERSION: &str = "v1.0.1";

pub struct Adapter {
    endpoint: String,
    nmm_flags: Vec<String>,
    server: config::Server,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(
        endpoint: impl Into<String>,
        extra_adapter_info: &str,
        server: &config::Server,
    ) -> Result<Self, String> {
        let mut info = NmExtNmm::default();
        if !extra_adapter_info.is_empty() {
            info = jsonutil::unmarshal(extra_adapter_info.as_bytes())
                .map_err(|e| format!("invalid extra info: {e}"))?;
        }
        Ok(Self { endpoint: endpoint.into(), nmm_flags: info.nmm_flags, server: server.clone() })
    }
}

#[derive(Serialize)]
struct NmExtPrebidStoredRequest {
    id: String,
}

#[derive(Serialize)]
struct Server {
    externalurl: String,
    gvlid: i32,
    datacenter: String,
}

#[derive(Serialize)]
struct NmExtPrebid {
    storedrequest: NmExtPrebidStoredRequest,
    #[serde(skip_serializing_if = "Option::is_none")]
    server: Option<Server>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct NmExtNmm {
    #[serde(rename = "nmmFlags", skip_serializing_if = "Vec::is_empty")]
    nmm_flags: Vec<String>,
    #[serde(rename = "adSlots", skip_serializing_if = "Vec::is_empty")]
    ad_slots: Vec<String>,
    #[serde(rename = "allowedAds", skip_serializing_if = "Vec::is_empty")]
    allowed_ads: Vec<String>,
    // `version.Ver` is unset unless injected by ldflags, so it is empty (and omitted) here.
    #[serde(skip_serializing_if = "String::is_empty")]
    server_version: String,
    #[serde(rename = "nm_version", skip_serializing_if = "String::is_empty")]
    adapter_version: String,
}

#[derive(Serialize)]
struct NextMillJsonExt {
    prebid: NmExtPrebid,
    #[serde(rename = "nextMillennium")]
    next_millennium: NmExtNmm,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ImpExtNextMillennium {
    group_id: String,
    placement_id: String,
    #[serde(rename = "adSlots")]
    ad_slots: Vec<String>,
    #[serde(rename = "allowedAds")]
    allowed_ads: Vec<String>,
}

fn ext_text(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

/// `jsonutil::unmarshal` with json-iterator's wording for empty input (nil `imp.ext`).
fn unmarshal_obj<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T, BidderError> {
    if data.iter().all(|b| b" \t\r\n".contains(b)) {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()));
    }
    jsonutil::unmarshal(data)
}

fn get_impression_ext(imp: &crate::ortb::openrtb2::Imp) -> Result<ImpExtNextMillennium, BidderError> {
    let bidder_ext: ExtImpBidder =
        unmarshal_obj(&ext_text(&imp.ext)).map_err(|e| BidderError::bad_input(e.to_string()))?;
    unmarshal_obj(&ext_text(&bidder_ext.bidder)).map_err(|e| BidderError::bad_input(e.to_string()))
}

fn create_bid_request(
    prebid: &BidRequest,
    params: &ImpExtNextMillennium,
    flags: &[String],
    server_params: &config::Server,
) -> Result<BidRequest, BidderError> {
    let mut placement_id = params.placement_id.clone();
    if !params.group_id.is_empty() {
        let mut domain = String::new();
        let mut size = String::new();
        if let Some(site) = &prebid.site {
            domain = site.domain.clone();
        }
        if let Some(app) = &prebid.app {
            domain = app.domain.clone();
        }
        // Go indexes `Imp[0]`, which exists because this runs once per parsed imp.
        if let Some(banner) = prebid.imp.first().and_then(|i| i.banner.as_ref()) {
            if let Some(f) = banner.format.first() {
                size = format!("{}x{}", f.w, f.h);
            } else if let (Some(w), Some(h)) = (banner.w, banner.h) {
                size = format!("{w}x{h}");
            }
        }
        placement_id = format!("g{};{};{}", params.group_id, size, domain);
    }

    let mut ext = NextMillJsonExt {
        prebid: NmExtPrebid { storedrequest: NmExtPrebidStoredRequest { id: placement_id }, server: None },
        next_millennium: NmExtNmm { nmm_flags: flags.to_vec(), ..Default::default() },
    };
    let mut bid_request = prebid.clone();
    let common = Ext::from_serialize(&ext).map_err(|e| BidderError::other(e.to_string()))?;
    let Some(imp0) = bid_request.imp.first_mut() else {
        return Err(BidderError::other("no impressions in the request"));
    };
    imp0.ext = Some(common);

    ext.prebid.server = Some(Server {
        gvlid: server_params.gvl_id,
        datacenter: server_params.data_center.clone(),
        externalurl: server_params.external_url.clone(),
    });
    ext.next_millennium.adapter_version = NM_ADAPTER_VERSION.to_string();
    ext.next_millennium.allowed_ads = params.allowed_ads.clone();
    ext.next_millennium.ad_slots = params.ad_slots.clone();
    bid_request.ext = Some(Ext::from_serialize(&ext).map_err(|e| BidderError::other(e.to_string()))?);
    Ok(bid_request)
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut res_imps = vec![];
        let mut errors = vec![];
        for imp in &request.imp {
            match get_impression_ext(imp) {
                Ok(e) => res_imps.push(e),
                Err(e) => errors.push(e),
            }
        }
        if !errors.is_empty() {
            return (vec![], errors);
        }
        let mut result = Vec::with_capacity(res_imps.len());
        for params in &res_imps {
            let new_request = match create_bid_request(request, params, &self.nmm_flags, &self.server) {
                Ok(r) => r,
                Err(e) => return (vec![], vec![e]),
            };
            let body = match crate::go_json::to_vec(&new_request) {
                Ok(b) => b,
                Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
            };
            let mut headers = Header::new();
            headers.add("Content-Type", "application/json;charset=utf-8");
            headers.add("Accept", "application/json");
            headers.add("x-openrtb-version", "2.5");
            result.push(RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: new_request.imp.iter().map(|i| i.id.clone()).collect(),
            });
        }
        (result, vec![])
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 204 {
            return (None, vec![]);
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected http status code: {}",
                    response.status_code
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => {
                // Go formats the error pointer with `%d`, which prints the struct fields.
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "Bad server response: &{{%!d(string={})}}",
                        e.message()
                    ))],
                );
            }
        };
        if bid_resp.seatbid.is_empty() {
            return (None, vec![]);
        }
        let mut bid_response = BidderResponse::with_bids_capacity(1);
        let mut errors = vec![];
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_bid_type(bid.mtype) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errors.push(e),
                }
            }
        }
        (Some(bid_response), errors)
    }
}

fn get_bid_type(m_type: MarkupType) -> Result<BidType, BidderError> {
    match m_type {
        MarkupType::BANNER => Ok(BidType::Banner),
        MarkupType::VIDEO => Ok(BidType::Video),
        other => Err(BidderError::bad_server_response(format!("Unsupported return mType: {}", other.0))),
    }
}
