//! Go `adapters/insticator/insticator.go`.

use serde::{Deserialize, Serialize};
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp, MarkupType, Publisher, Video};
use crate::ortb::Ext;

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

#[derive(Serialize)]
struct ImpExt {
    insticator: ImpInsticatorExt,
}

#[derive(Serialize)]
struct ImpInsticatorExt {
    #[serde(rename = "adUnitId")]
    ad_unit_id: String,
    #[serde(rename = "publisherId")]
    publisher_id: String,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ReqExt {
    #[serde(skip_serializing_if = "Option::is_none")]
    insticator: Option<ReqInsticatorExt>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ReqInsticatorExt {
    #[serde(skip_serializing_if = "Option::is_none")]
    caller: Option<Vec<InsticatorCaller>>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
struct InsticatorCaller {
    #[serde(skip_serializing_if = "String::is_empty")]
    name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    version: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpInsticator {
    #[serde(rename = "adUnitId")]
    ad_unit_id: String,
    #[serde(rename = "publisherId")]
    publisher_id: String,
}

/// Go `jsonutil.Unmarshal(raw, &v)` on a `json.RawMessage` that may be absent or not an object.
fn unmarshal_ext<T: serde::de::DeserializeOwned + Default>(ext: Option<&Ext>) -> Result<T, BidderError> {
    let Some(ext) = ext else {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into()));
    };
    if ext.0.is_null() {
        return Ok(T::default());
    }
    if !ext.0.is_object() {
        let text = ext.to_json();
        let first = text.chars().next().unwrap_or('\0');
        return Err(BidderError::FailedToUnmarshal(format!("expect {{ or n, but found {first}")));
    }
    ext.decode().map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))
}

/// Go `getMediaTypeForBid`: unknown or missing `mtype` is banner.
fn get_media_type_for_bid(bid: &Bid) -> BidType {
    match bid.mtype {
        MarkupType::BANNER => BidType::Banner,
        MarkupType::VIDEO => BidType::Video,
        _ => BidType::Banner,
    }
}

fn make_req_ext(request: &BidRequest) -> Result<Ext, BidderError> {
    let mut req_ext = ReqExt::default();
    if let Some(ext) = &request.ext {
        // Go struct-path message for the one nested shape the fixtures exercise.
        if ext.0.is_object() {
            if let Some(ins) = ext.0.get("insticator") {
                if !ins.is_null() && !ins.is_object() {
                    let text = ins.to_string();
                    let first = text.chars().next().unwrap_or('\0');
                    return Err(BidderError::FailedToUnmarshal(format!(
                        "cannot unmarshal insticator.reqExt.Insticator: expect {{ or n, but found {first}"
                    )));
                }
            }
        }
        req_ext = unmarshal_ext(Some(ext))?;
    }
    let insticator = req_ext.insticator.get_or_insert_with(ReqInsticatorExt::default);
    let callers = insticator.caller.get_or_insert_with(Vec::new);
    callers.push(InsticatorCaller { name: "Prebid-Server".into(), version: "n/a".into() });
    // `omitempty` on a non-empty slice never drops it.
    Ext::from_serialize(&req_ext).map_err(|e| BidderError::FailedToMarshal(e.to_string()))
}

fn validate_video_params(video: &Video) -> Result<(), BidderError> {
    // Go also rejects a nil `mimes`; Rust cannot tell `[]` from absent, so both are rejected.
    if video.w.unwrap_or_default() == 0 || video.h.unwrap_or_default() == 0 || video.mimes.is_none() {
        return Err(BidderError::bad_input("One or more invalid or missing video field(s) w, h, mimes"));
    }
    Ok(())
}

struct MadeImp {
    imp: Imp,
    key: String,
    publisher_id: String,
}

fn make_imps(imp: &Imp) -> Result<MadeImp, (BidderError, Option<(String, String)>)> {
    let bad = |e: BidderError| (BidderError::bad_input(e.to_string()), None);
    let bidder_ext: ExtImpBidder = unmarshal_ext(imp.ext.as_ref()).map_err(bad)?;
    let insticator_ext: ExtImpInsticator = unmarshal_ext(bidder_ext.bidder.as_ref()).map_err(bad)?;
    let mut imp = imp.clone();
    let imp_ext = ImpExt {
        insticator: ImpInsticatorExt {
            ad_unit_id: insticator_ext.ad_unit_id.clone(),
            publisher_id: insticator_ext.publisher_id.clone(),
        },
    };
    imp.ext = Some(Ext::from_serialize(&imp_ext).map_err(|e| bad(BidderError::other(e.to_string())))?);
    if let Some(video) = &imp.video {
        validate_video_params(video).map_err(|e| {
            (
                BidderError::bad_input(e.to_string()),
                Some((insticator_ext.ad_unit_id.clone(), insticator_ext.publisher_id.clone())),
            )
        })?;
    }
    Ok(MadeImp { imp, key: insticator_ext.ad_unit_id, publisher_id: insticator_ext.publisher_id })
}

fn resolve_bid_floor(
    bid_floor: f64,
    cur: &str,
    req_info: &ExtraRequestInfo,
) -> Result<f64, BidderError> {
    if bid_floor > 0.0 && !cur.is_empty() && cur.to_uppercase() != "USD" {
        let floor = req_info.convert_currency(bid_floor, cur, "USD")?;
        return Ok((floor * 10000.0).round() / 10000.0);
    }
    Ok(bid_floor)
}

fn populate_publisher_id(publisher_id: &str, request: &mut BidRequest) {
    if let Some(site) = &mut request.site {
        let mut p: Publisher = site.publisher.clone().unwrap_or_default();
        p.id = publisher_id.to_string();
        site.publisher = Some(p);
    }
    if let Some(app) = &mut request.app {
        let mut p: Publisher = app.publisher.clone().unwrap_or_default();
        p.id = publisher_id.to_string();
        app.publisher = Some(p);
    }
}

impl Adapter {
    fn make_request(&self, request: &mut BidRequest, imp_list: Vec<Imp>) -> Result<RequestData, BidderError> {
        request.imp = imp_list;
        let body = crate::go_json::to_vec(&*request)
            .map_err(|e| BidderError::FailedToMarshal(e.to_string()))?;
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        if let Some(device) = &request.device {
            if !device.ua.is_empty() {
                headers.add("User-Agent", device.ua.clone());
            }
            if !device.ipv6.is_empty() {
                headers.set("X-Forwarded-For", device.ipv6.clone());
            }
            if !device.ip.is_empty() {
                headers.set("X-Forwarded-For", device.ip.clone());
                headers.add("IP", device.ip.clone());
            }
        }
        Ok(RequestData {
            method: "POST".into(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
        })
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        request_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut adapter_requests = Vec::new();
        // Insertion-ordered stand-in for Go's `map[string][]openrtb2.Imp`.
        let mut grouped: Vec<(String, Vec<Imp>)> = Vec::new();

        let mut request_copy = request.clone();
        match make_req_ext(request) {
            Ok(e) => request_copy.ext = Some(e),
            Err(e) => {
                // Go still assigns the nil result to `request.Ext`.
                errs.push(e);
                request_copy.ext = None;
            }
        }

        let mut is_publisher_id_populated = false;
        for imp in &request.imp {
            let made = match make_imps(imp) {
                Ok(m) => m,
                Err((e, _)) => {
                    errs.push(e);
                    continue;
                }
            };
            let MadeImp { mut imp, key, publisher_id } = made;
            if !is_publisher_id_populated {
                populate_publisher_id(&publisher_id, &mut request_copy);
                is_publisher_id_populated = true;
            }
            let resolved = match resolve_bid_floor(imp.bidfloor, &imp.bidfloorcur, request_info) {
                Ok(v) => v,
                Err(_) => {
                    errs.push(BidderError::bad_input(format!(
                        "Error in converting the provided bid floor currency from {} to USD",
                        imp.bidfloorcur
                    )));
                    continue;
                }
            };
            if resolved > 0.0 {
                imp.bidfloor = resolved;
                imp.bidfloorcur = "USD".into();
            }
            match grouped.iter_mut().find(|(k, _)| *k == key) {
                Some((_, list)) => list.push(imp),
                None => grouped.push((key, vec![imp])),
            }
        }
        for (_, imp_list) in grouped {
            match self.make_request(&mut request_copy, imp_list) {
                Ok(r) => adapter_requests.push(r),
                Err(e) => errs.push(e),
            }
        }
        (adapter_requests, errs)
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if crate::bidder::is_response_status_code_no_content(response_data) {
            return (None, vec![]);
        }
        if let Some(err) = crate::bidder::check_response_status_code_for_errors(response_data) {
            return (None, vec![err]);
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut out = BidderResponse::with_bids_capacity(request.imp.len());
        if !response.cur.is_empty() {
            out.currency = response.cur.clone();
        }
        for sb in response.seatbid {
            for bid in sb.bid {
                let t = get_media_type_for_bid(&bid);
                out.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(out), vec![])
    }
}
