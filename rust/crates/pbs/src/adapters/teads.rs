//! Go `adapters/teads/teads.go` and `models.go`.

use crate::bid_types::{BidType, ExtBidPrebidMeta};
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;
use serde::Deserialize;

/// Go `openrtb_ext.ExtImpTeads` (never read; `defaultBidderImpExtension` is used instead).
#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
pub struct ExtImpTeads {
    #[serde(rename = "placementId", default)]
    pub placement_id: String,
}

#[derive(Debug, Default, Deserialize)]
struct DefaultBidderImpExtension {
    #[serde(default)]
    bidder: Bidder_,
}

#[derive(Debug, Default, Deserialize)]
struct Bidder_ {
    #[serde(rename = "placementId", default)]
    placement_id: i64,
}

#[derive(Debug, Default, Deserialize)]
struct TeadsBidExt {
    #[serde(default)]
    prebid: TeadsPrebidExt,
}

#[derive(Debug, Default, Deserialize)]
struct TeadsPrebidExt {
    #[serde(default)]
    meta: TeadsPrebidMeta,
}

#[derive(Debug, Default, Deserialize)]
struct TeadsPrebidMeta {
    #[serde(rename = "rendererName", default)]
    renderer_name: String,
    #[serde(rename = "rendererVersion", default)]
    renderer_version: String,
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
    endpoint_template: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: &str) -> Result<Self, String> {
        let tmpl = EndpointTemplate::parse(endpoint)
            .map_err(|e| format!("unable to parse endpoint url template: {e}"))?;
        Ok(Self { endpoint_template: tmpl })
    }

    /// Go `buildEndpointURL`.
    fn build_endpoint_url(&self) -> Result<String, BidderError> {
        let host = self
            .endpoint_template
            .resolve(&EndpointTemplateParams::default())
            .map_err(|e| BidderError::bad_input(format!("Unable to parse endpoint url template: {e}")))?;
        let uri = url::Url::parse(&host).map_err(|e| BidderError::bad_input(format!("Malformed URL: {e}")))?;
        Ok(uri.to_string())
    }
}

fn update_imp_object(imps: &mut [Imp]) -> Result<(), BidderError> {
    for imp in imps.iter_mut() {
        if let Some(banner) = imp.banner.as_mut() {
            if let Some(first) = banner.format.first() {
                let (h, w) = (first.h, first.w);
                banner.h = Some(h);
                banner.w = Some(w);
            }
        }
        let default_imp_ext: DefaultBidderImpExtension = decode_ext(imp.ext.as_ref())
            .map_err(|_| BidderError::bad_input("Error parsing Imp.Ext object"))?;
        if default_imp_ext.bidder.placement_id == 0 {
            return Err(BidderError::bad_input("placementId should not be 0."));
        }
        imp.tagid = default_imp_ext.bidder.placement_id.to_string();
        let ext = serde_json::json!({"kv": {"placementId": default_imp_ext.bidder.placement_id}});
        imp.ext = Some(
            Ext::from_serialize(&ext).map_err(|_| BidderError::bad_input("Error stringify Imp.Ext object"))?,
        );
    }
    Ok(())
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::bad_input("No impression in the bid request")]);
        }
        let endpoint_url = match self.build_endpoint_url() {
            Ok(u) if !u.is_empty() => u,
            Ok(_) => return (vec![], vec![BidderError::other("")]),
            Err(e) => return (vec![], vec![e]),
        };
        let mut request = request.clone();
        if let Err(e) = update_imp_object(&mut request.imp) {
            return (vec![], vec![e]);
        }
        let body = match crate::go_json::to_vec(&request) {
            Ok(b) => b,
            Err(_) => return (vec![], vec![BidderError::bad_input("Error parsing BidRequest object")]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: endpoint_url,
                body,
                headers,
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response) {
            return (None, vec![]);
        }
        if let Some(err) = check_response_status_code_for_errors(response) {
            return (None, vec![err]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bidder_response = BidderResponse::with_bids_capacity(bid_resp.seatbid.len());
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let ext = match get_teads_renderer_from_bid_ext(bid.ext.as_ref()) {
                    Ok(e) => e,
                    Err(e) => return (None, vec![e]),
                };
                let bid_type = match get_media_type_for_imp(&bid.impid, &internal_request.imp) {
                    Ok(t) => t,
                    Err(e) => return (None, vec![e]),
                };
                let mut typed = TypedBid::new(bid, bid_type);
                typed.bid_meta = Some(ExtBidPrebidMeta {
                    renderer_name: ext.prebid.meta.renderer_name,
                    renderer_version: ext.prebid.meta.renderer_version,
                    ..Default::default()
                });
                bidder_response.bids.push(typed);
            }
        }
        if !bid_resp.cur.is_empty() {
            bidder_response.currency = bid_resp.cur;
        }
        (Some(bidder_response), vec![])
    }
}

fn get_teads_renderer_from_bid_ext(ext: Option<&Ext>) -> Result<TeadsBidExt, BidderError> {
    let bid_ext: TeadsBidExt = decode_ext(ext)?;
    if bid_ext.prebid.meta.renderer_name.is_empty() {
        return Err(BidderError::bad_input("RendererName should not be empty if present"));
    }
    if bid_ext.prebid.meta.renderer_version.is_empty() {
        return Err(BidderError::bad_input("RendererVersion should not be empty if present"));
    }
    Ok(bid_ext)
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.video.is_some() {
                return Ok(BidType::Video);
            }
            return Ok(BidType::Banner);
        }
    }
    Err(BidderError::bad_input("Imp ids were not equals"))
}

#[allow(dead_code)]
type _Unused = BidType;
