//! Go `adapters/taboola/taboola.go`.
#![allow(dead_code, unused_imports)]

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp};
use crate::ortb::Ext;
use sonic_rs::{JsonContainerTrait, JsonValueTrait};
use crate::ortb::adcom1::PlacementPosition;
use crate::ortb::openrtb2::{Publisher, Site};

pub struct Adapter {
    endpoint: EndpointTemplate,
    gvl_id: String,
}

impl Adapter {
    /// Go `Builder`: `gvl_id` is `server.GvlID` (`0` or less leaves it empty).
    pub fn new(endpoint: &str, gvl_id: i32) -> Result<Self, String> {
        Ok(Self {
            endpoint: parse_template(endpoint)?,
            gvl_id: if gvl_id > 0 { gvl_id.to_string() } else { String::new() },
        })
    }

    fn build_request(&self, request: &BidRequest) -> Result<RequestData, BidderError> {
        let body = crate::go_json::to_vec(request).map_err(|e| BidderError::other(e.to_string()))?;

        let imp0 = &request.imp[0];
        let media_type = if imp0.banner.is_some() {
            "display"
        } else if imp0.native.is_some() {
            "native"
        } else {
            return Err(BidderError::other(format!("unsupported media type for imp: {:?}", imp0)));
        };

        let mut publisher_id = String::new();
        if let Some(site) = request.site.as_ref().filter(|s| !s.id.is_empty()) {
            publisher_id = site.id.clone();
        } else if let Some(app) = request.app.as_ref().filter(|a| !a.id.is_empty()) {
            publisher_id = app.id.clone();
        }

        let uri = self
            .endpoint
            .resolve(&EndpointTemplateParams {
                publisher_id,
                media_type: media_type.to_string(),
                gvl_id: self.gvl_id.clone(),
                ..Default::default()
            })
            .map_err(BidderError::other)?;

        Ok(RequestData {
            method: "POST".into(),
            uri,
            body,
            headers: Header::new(),
            imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
        })
    }
}

/// `ImpExtTaboola`: the last parsed imp's values are used for the request-level fields (Go keeps
/// one variable across the loop).
#[derive(Default, Clone)]
struct ImpExtTaboola {
    publisher_id: String,
    publisher_domain: String,
    bid_floor: f64,
    tag_id_lower: String,
    tag_id_upper: String,
    bcat: Option<Vec<String>>,
    badv: Option<Vec<String>>,
    page_type: String,
    position: Option<i64>,
}

fn string_list(obj: Option<&Val>, name: &str, path: &str) -> Result<Option<Vec<String>>, BidderError> {
    let Some(v) = field(obj, name) else { return Ok(None) };
    if v.is_null() {
        return Ok(None);
    }
    let bad = || {
        let c = sonic_rs::to_string(v).ok().and_then(|s| s.chars().next()).unwrap_or('\0');
        BidderError::FailedToUnmarshal(format!("cannot unmarshal {path}: expects \" or n, but found {c}"))
    };
    let arr = v.as_array().ok_or_else(bad)?;
    arr.iter()
        .map(|x| if x.is_null() { Ok(String::new()) } else { x.as_str().map(str::to_string).ok_or_else(bad) })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn parse_taboola_ext(params: Option<&Val>) -> Result<ImpExtTaboola, BidderError> {
    let mut ext = ImpExtTaboola {
        publisher_id: str_field(params, "publisherId", "openrtb_ext.ImpExtTaboola.PublisherId")?,
        publisher_domain: str_field(params, "publisherDomain", "openrtb_ext.ImpExtTaboola.PublisherDomain")?,
        tag_id_lower: str_field(params, "tagid", "openrtb_ext.ImpExtTaboola.TagId")?,
        tag_id_upper: str_field(params, "tagId", "openrtb_ext.ImpExtTaboola.TagID")?,
        bcat: string_list(params, "bcat", "openrtb_ext.ImpExtTaboola.BCat")?,
        badv: string_list(params, "badv", "openrtb_ext.ImpExtTaboola.BAdv")?,
        page_type: str_field(params, "pageType", "openrtb_ext.ImpExtTaboola.PageType")?,
        ..Default::default()
    };
    ext.bid_floor = match field(params, "bidfloor") {
        None => 0.0,
        Some(v) if v.is_null() => 0.0,
        Some(v) => v.as_f64().ok_or_else(|| {
            BidderError::FailedToUnmarshal("cannot unmarshal openrtb_ext.ImpExtTaboola.BidFloor".into())
        })?,
    };
    ext.position = match field(params, "position") {
        None => None,
        Some(v) if v.is_null() => None,
        Some(v) => Some(v.as_i64().ok_or_else(|| {
            BidderError::FailedToUnmarshal("cannot unmarshal openrtb_ext.ImpExtTaboola.Position".into())
        })?),
    };
    Ok(ext)
}

/// Go `createTaboolaRequests`.
fn create_taboola_requests(request: &BidRequest) -> (Vec<BidRequest>, Vec<BidderError>) {
    let mut modified = request.clone();
    let mut native_imp = Vec::new();
    let mut banner_imp = Vec::new();
    let mut errs = Vec::new();

    let mut taboola_ext = ImpExtTaboola::default();
    for i in 0..modified.imp.len() {
        let params = match imp_bidder_params(&modified.imp[i]) {
            Ok(p) => p,
            Err((_, e)) => {
                errs.push(e);
                continue;
            }
        };
        // Go decodes into the same struct each time: only keys present overwrite earlier values.
        // A failed decode leaves already-set fields in place, but the imp is skipped.
        match parse_taboola_ext(params) {
            Ok(e) => taboola_ext = merge_ext(taboola_ext, e, params),
            Err(e) => {
                errs.push(e);
                continue;
            }
        }

        let imp = &mut modified.imp[i];
        let tag_id = if taboola_ext.tag_id_upper.is_empty() {
            taboola_ext.tag_id_lower.clone()
        } else {
            taboola_ext.tag_id_upper.clone()
        };
        imp.tagid = tag_id;
        if taboola_ext.bid_floor != 0.0 {
            imp.bidfloor = taboola_ext.bid_floor;
        }
        if let Some(banner) = imp.banner.as_mut() {
            if let Some(pos) = taboola_ext.position {
                banner.pos = Some(PlacementPosition(pos as _));
            }
            banner_imp.push(imp.clone());
        } else if imp.native.is_some() {
            native_imp.push(imp.clone());
        }
    }

    let publisher = Publisher { id: taboola_ext.publisher_id.clone(), ..Default::default() };

    if let Some(site) = modified.site.as_mut() {
        let domain = if !taboola_ext.publisher_domain.is_empty() {
            taboola_ext.publisher_domain.clone()
        } else {
            request.site.as_ref().map(|s| s.domain.clone()).unwrap_or_default()
        };
        site.id = taboola_ext.publisher_id.clone();
        site.name = taboola_ext.publisher_id.clone();
        site.domain = domain;
        site.publisher = Some(publisher.clone());
    }
    if let Some(app) = modified.app.as_mut() {
        app.id = taboola_ext.publisher_id.clone();
        app.publisher = Some(publisher);
    }
    if let Some(bcat) = &taboola_ext.bcat {
        modified.bcat = bcat.clone();
    }
    if let Some(badv) = &taboola_ext.badv {
        modified.badv = badv.clone().into();
    }
    if !taboola_ext.page_type.is_empty() {
        #[derive(serde::Serialize)]
        struct RequestExt<'a> {
            #[serde(rename = "pageType", skip_serializing_if = "str::is_empty")]
            page_type: &'a str,
        }
        match Ext::from_serialize(&RequestExt { page_type: &taboola_ext.page_type }) {
            Ok(e) => modified.ext = Some(e),
            Err(e) => errs.push(BidderError::other(format!("could not marshal, err: {e}"))),
        }
    }

    let mut native_req = modified.clone();
    native_req.imp = native_imp;
    let mut banner_req = modified;
    banner_req.imp = banner_imp;
    (vec![native_req, banner_req], errs)
}

/// Keys absent from this imp's `bidder` object keep the value from earlier imps.
fn merge_ext(mut old: ImpExtTaboola, new: ImpExtTaboola, params: Option<&Val>) -> ImpExtTaboola {
    let has = |k: &str| field(params, k).is_some();
    if has("publisherId") {
        old.publisher_id = new.publisher_id;
    }
    if has("publisherDomain") {
        old.publisher_domain = new.publisher_domain;
    }
    if has("bidfloor") {
        old.bid_floor = new.bid_floor;
    }
    if has("tagid") {
        old.tag_id_lower = new.tag_id_lower;
    }
    if has("tagId") {
        old.tag_id_upper = new.tag_id_upper;
    }
    if has("bcat") && new.bcat.is_some() {
        old.bcat = new.bcat;
    }
    if has("badv") && new.badv.is_some() {
        old.badv = new.badv;
    }
    if has("pageType") {
        old.page_type = new.page_type;
    }
    if has("position") && new.position.is_some() {
        old.position = new.position;
    }
    old
}

fn get_media_type(imp_id: &str, imps: &[Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return Ok(BidType::Banner);
            } else if imp.native.is_some() {
                return Ok(BidType::Native);
            }
        }
    }
    Err(BidderError::bad_input(format!("Failed to find banner/native impression \"{imp_id}\" ")))
}

/// Go `resolveMacros`.
fn resolve_macros(bid: &mut Bid) {
    let price = format!("{}", bid.price);
    bid.nurl = bid.nurl.replace("${AUCTION_PRICE}", &price);
    bid.adm = bid.adm.replace("${AUCTION_PRICE}", &price);
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let (taboola_requests, errs) = create_taboola_requests(request);
        if !errs.is_empty() {
            return (vec![], errs);
        }
        let mut requests = Vec::new();
        for r in &taboola_requests {
            if !r.imp.is_empty() {
                match self.build_request(r) {
                    Ok(d) => requests.push(d),
                    Err(e) => {
                        return (vec![], vec![BidderError::other(format!("unable to build request {e}"))])
                    }
                }
            }
        }
        (requests, errs)
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
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };

        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        bid_response.currency = response.cur;
        let mut errs = Vec::new();
        for seat_bid in response.seatbid {
            for mut bid in seat_bid.bid {
                let bid_type = get_media_type(&bid.impid, &request.imp);
                resolve_macros(&mut bid);
                match bid_type {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        (Some(bid_response), errs)
    }
}

// ---- local helpers (Go `jsonutil.Unmarshal` into `json.RawMessage`-backed params) ----------
type Val = sonic_rs::Value;

fn unmarshal_err(first: char) -> BidderError {
    BidderError::FailedToUnmarshal(format!("expect {{ or n, but found {first}"))
}

/// Go `jsonutil.Unmarshal(raw, &struct)` first-byte check: absent input fails on the NUL byte,
/// anything but an object or `null` fails on its first character. `Ok(None)` is `null`.
fn obj_of(v: Option<&Val>) -> Result<Option<&Val>, BidderError> {
    let Some(v) = v else {
        return Err(unmarshal_err('\0'));
    };
    if v.is_object() {
        Ok(Some(v))
    } else if v.is_null() {
        Ok(None)
    } else {
        Err(unmarshal_err(sonic_rs::to_string(v).ok().and_then(|s| s.chars().next()).unwrap_or('\0')))
    }
}

/// jsoniter matches struct keys case-insensitively.
fn field<'a>(obj: Option<&'a Val>, name: &str) -> Option<&'a Val> {
    let obj = obj?.as_object()?;
    obj.iter().find(|(k, _)| *k == name).or_else(|| obj.iter().find(|(k, _)| k.eq_ignore_ascii_case(name))).map(|(_, v)| v)
}

/// A Go `string` field: absent or `null` is `""`, other types fail like jsoniter.
fn str_field(obj: Option<&Val>, name: &str, path: &str) -> Result<String, BidderError> {
    match field(obj, name) {
        None => Ok(String::new()),
        Some(v) if v.is_null() => Ok(String::new()),
        Some(v) => match v.as_str() {
            Some(s) => Ok(s.to_string()),
            None => {
                let c = sonic_rs::to_string(v).ok().and_then(|s| s.chars().next()).unwrap_or('\0');
                Err(BidderError::FailedToUnmarshal(format!(
                    "cannot unmarshal {path}: expects \" or n, but found {c}"
                )))
            }
        },
    }
}

/// `imp.ext` -> `{"bidder": ...}` -> the bidder params object (Go `ExtImpBidder`).
/// The error is `(failed_at_bidder_step, error)`.
fn imp_bidder_params(imp: &Imp) -> Result<Option<&Val>, (bool, BidderError)> {
    let outer = obj_of(imp.ext.as_ref().map(|e| &e.0)).map_err(|e| (false, e))?;
    obj_of(field(outer, "bidder")).map_err(|e| (true, e))
}

/// Go `template.Parse` rejects an action that is not a field (`{{Malformed}}`).
fn parse_template(endpoint: &str) -> Result<EndpointTemplate, String> {
    let mut rest = endpoint;
    while let Some(open) = rest.find("{{") {
        let after = &rest[open + 2..];
        if let Some(close) = after.find("}}") {
            let action = after[..close].trim();
            if !action.starts_with('.') {
                return Err(format!(
                    "unable to parse endpoint url template: template: endpointTemplate:1: function \"{action}\" not defined"
                ));
            }
            rest = &after[close + 2..];
        } else {
            break;
        }
    }
    EndpointTemplate::parse(endpoint)
        .map_err(|e| format!("unable to parse endpoint url template: {e}"))
}
// ---------------------------------------------------------------------------------------------

/// A Go `int`/`int64` field: absent or `null` is 0. `Err(())` for a non-integer value.
fn int_field(obj: Option<&Val>, name: &str) -> Result<i64, ()> {
    match field(obj, name) {
        None => Ok(0),
        Some(v) if v.is_null() => Ok(0),
        Some(v) => v.as_i64().ok_or(()),
    }
}

/// Go `jsonutil.Unmarshal(bid.Ext, &openrtb_ext.ExtBid)` then `bidExt.Prebid.Type`:
/// `None` when the ext is absent, fails to parse or has no `prebid` object; otherwise the type
/// text (`""` when `prebid.type` is missing).
fn prebid_type(ext: Option<&Ext>) -> Option<String> {
    let ext = ext?;
    let obj = obj_of(Some(&ext.0)).ok()??;
    let prebid = field(Some(obj), "prebid")?;
    if prebid.is_null() {
        return None;
    }
    if !prebid.is_object() {
        return None;
    }
    match field(Some(prebid), "type") {
        None => Some(String::new()),
        Some(v) if v.is_null() => Some(String::new()),
        Some(v) => v.as_str().map(str::to_string),
    }
}

/// Go `openrtb_ext.ParseBidType`.
fn parse_bid_type(s: &str) -> Result<BidType, BidderError> {
    BidType::parse(s).map_err(BidderError::other)
}
