//! Go `adapters/adkernelAdn/adkernelAdn.go`.

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use serde::Deserialize;
use sonic_rs::JsonValueTrait;

/// Go `openrtb_ext.ExtImpAdkernelAdn`.
#[derive(Debug, Default, Deserialize, Clone, PartialEq, Eq)]
struct ExtImpAdkernelAdn {
    #[serde(rename = "pubId", default)]
    publisher_id: i64,
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

    fn build_adapter_request(
        &self,
        prebid_bid_request: &BidRequest,
        params: &ExtImpAdkernelAdn,
        imps: Vec<Imp>,
    ) -> Result<RequestData, BidderError> {
        let new_bid_request = create_bid_request(prebid_bid_request, imps);
        let req_json = crate::go_json::to_vec(&new_bid_request).map_err(|e| BidderError::other(e.to_string()))?;
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        headers.add("x-openrtb-version", "2.5");
        let url = self
            .endpoint_template
            .resolve(&EndpointTemplateParams { publisher_id: params.publisher_id.to_string(), ..Default::default() })
            .map_err(BidderError::other)?;
        Ok(RequestData {
            method: "POST".into(),
            uri: url,
            body: req_json,
            headers,
            imp_ids: new_bid_request.imp.iter().map(|i| i.id.clone()).collect(),
        })
    }
}

fn create_bid_request(prebid: &BidRequest, imps: Vec<Imp>) -> BidRequest {
    let mut r = prebid.clone();
    r.imp = imps;
    if let Some(site) = r.site.as_mut() {
        site.publisher = None;
        site.domain = String::new();
    }
    if let Some(app) = r.app.as_mut() {
        app.publisher = None;
    }
    r
}

/// `decode_ext` for `ExtImpAdkernelAdn` with json-iterator's wording for a non-integer pubId.
fn get_impression_ext(imp: &Imp) -> Result<ExtImpAdkernelAdn, BidderError> {
    let bidder_ext: ExtImpBidder = decode_ext(imp.ext.as_ref()).map_err(|e| BidderError::bad_input(e.to_string()))?;
    if let Some(v) = bidder_ext
        .bidder
        .as_ref()
        .filter(|b| b.0.is_object())
        .and_then(|b| b.0.get("pubId"))
    {
        if !v.is_number() && !v.is_null() {
            let text = v.to_string();
            let found = text.chars().next().unwrap_or(' ');
            // json-iterator: `ReadInt: unexpected character`; the real text continues with the
            // offending byte and the position, so only this prefix is reproduced.
            return Err(BidderError::bad_input(format!(
                "cannot unmarshal openrtb_ext.ExtImpAdkernelAdn.PublisherID: unexpected character: {found}"
            )));
        }
    }
    decode_ext(bidder_ext.bidder.as_ref()).map_err(|e| BidderError::bad_input(e.to_string()))
}

fn validate_impression(imp: &Imp, imp_ext: &ExtImpAdkernelAdn) -> Result<(), BidderError> {
    if imp_ext.publisher_id < 1 {
        return Err(BidderError::bad_input(format!("Invalid pubId value. Ignoring imp id={}", imp.id)));
    }
    if imp.video.is_none() && imp.banner.is_none() {
        return Err(BidderError::bad_input(format!(
            "Invalid imp with id={}. Expected imp.banner or imp.video",
            imp.id
        )));
    }
    Ok(())
}

/// Go `compatImpression`: alters the impression to comply with adkernel platform requirements.
fn compat_impression(imp: &mut Imp) -> Result<(), BidderError> {
    imp.ext = None; // do not forward ext to adkernel platform
    if let Some(banner) = imp.banner.as_mut() {
        // banner.w/h are required fields for adkernelAdn: take the first format entry
        if banner.w.is_none() && banner.h.is_none() {
            if banner.format.is_empty() {
                return Err(BidderError::bad_input("Expected at least one banner.format entry or explicit w/h"));
            }
            let format = banner.format.remove(0);
            banner.w = Some(format.w);
            banner.h = Some(format.h);
        }
        imp.video = None;
        imp.native = None;
        imp.audio = None;
        return Ok(());
    }
    if imp.video.is_some() {
        imp.banner = None;
        imp.audio = None;
        imp.native = None;
        return Ok(());
    }
    Err(BidderError::bad_input("Unsupported impression has been received"))
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::bad_input("No impression in the bid request")]);
        }
        // getImpressionsInfo
        let mut imps = Vec::new();
        let mut imp_exts = Vec::new();
        let mut info_errs = Vec::new();
        for imp in &request.imp {
            let ext = match get_impression_ext(imp) {
                Ok(e) => e,
                Err(e) => {
                    info_errs.push(e);
                    continue;
                }
            };
            if let Err(e) = validate_impression(imp, &ext) {
                info_errs.push(e);
                continue;
            }
            imps.push(imp.clone());
            imp_exts.push(ext);
        }
        if imps.is_empty() {
            return (vec![], info_errs);
        }
        errs.extend(info_errs);
        // dispatchImpressions: group by params (Go ranges over a map; first-seen order here).
        let mut groups: Vec<(ExtImpAdkernelAdn, Vec<Imp>)> = Vec::new();
        for (mut imp, ext) in imps.into_iter().zip(imp_exts) {
            if let Err(e) = compat_impression(&mut imp) {
                errs.push(e);
                continue;
            }
            match groups.iter_mut().find(|(k, _)| *k == ext) {
                Some((_, v)) => v.push(imp),
                None => groups.push((ext, vec![imp])),
            }
        }
        if groups.is_empty() {
            return (vec![], errs);
        }
        let mut result = Vec::with_capacity(groups.len());
        for (k, imps) in groups {
            match self.build_adapter_request(request, &k, imps) {
                Ok(r) => result.push(r),
                Err(e) => errs.push(e),
            }
        }
        (result, errs)
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        _external_request: &RequestData,
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
                // Go formats the error with `%d`: `&{%!d(string=msg)}`.
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "Bad server response: &{{%!d(string={})}}",
                        e.message()
                    ))],
                );
            }
        };
        if bid_resp.seatbid.len() != 1 {
            return (
                None,
                vec![BidderError::bad_server_response(format!("Invalid SeatBids count: {}", bid_resp.seatbid.len()))],
            );
        }
        let seat_bid = bid_resp.seatbid.into_iter().next().unwrap_or_default();
        let mut bid_response = BidderResponse::with_bids_capacity(seat_bid.bid.len());
        for bid in seat_bid.bid {
            let t = get_media_type_for_imp_id(&bid.impid, &internal_request.imp);
            bid_response.bids.push(TypedBid::new(bid, t));
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type_for_imp_id(imp_id: &str, imps: &[Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id && imp.banner.is_some() {
            return BidType::Banner;
        }
    }
    BidType::Video
}
