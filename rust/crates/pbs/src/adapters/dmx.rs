//! Go `adapters/dmx/dmx.go`.

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
use crate::ortb::adcom1::MediaCreativeSubtype;
use crate::ortb::openrtb2::{Banner, Bid, BidRequest, BidResponse, Eid, Imp, Video};
use crate::ortb::Ext;

/// Go `dmxParams`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct DmxParams {
    tagid: String,
    dmxid: String,
    memberid: String,
    publisher_id: String,
    seller_id: String,
    #[serde(deserialize_with = "crate::ortb::de::float")]
    bidfloor: f64,
}

/// Go `dmxExt`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct DmxExt {
    bidder: DmxParams,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct UserExtEids {
    eids: Vec<Eid>,
}

const PROTOCOLS: [i8; 6] = [2, 3, 5, 6, 7, 8];

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

/// Go `jsonutil.Unmarshal(ext, &dmxExt)`.
fn unmarshal_dmx_ext(ext: Option<&Ext>) -> Result<DmxExt, BidderError> {
    let Some(ext) = ext else {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into()));
    };
    if !ext.0.is_object() && !ext.0.is_null() {
        return Err(BidderError::FailedToUnmarshal(format!(
            "expect {{ or n, but found {}",
            ext.to_json().chars().next().unwrap_or('\u{0}')
        )));
    }
    ext.decode::<DmxExt>().map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))
}

fn user_seller_or_pub_id(a: &str, b: &str) -> String {
    if !a.is_empty() { a.to_string() } else { b.to_string() }
}

/// Go `url.QueryEscape`.
fn query_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn pub_ext(id: &str) -> Result<Ext, BidderError> {
    // Go `dmxPubExt{Dmx: dmxPubExtId{Id}}` (`omitempty` on a struct never omits it).
    let json = if id.is_empty() {
        "{\"dmx\":{}}".to_string()
    } else {
        format!("{{\"dmx\":{{\"id\":{}}}}}", serde_json::to_string(id).unwrap_or_default())
    };
    Ext::from_slice(json.as_bytes()).map_err(|e| BidderError::other(format!("unable to marshal ext, {e}")))
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs: Vec<BidderError> = Vec::new();
        let mut imps: Vec<Imp> = Vec::new();
        let mut publisher_id = String::new();
        let mut seller_id = String::new();
        let mut root = DmxExt::default();

        if request.user.is_none() && request.app.is_none() {
            return (vec![], vec![BidderError::other("No user id or app id found. Could not send request to DMX.")]);
        }

        if !request.imp.is_empty() {
            match unmarshal_dmx_ext(request.imp[0].ext.as_ref()) {
                Err(e) => errs.push(e),
                Ok(r) => {
                    publisher_id = user_seller_or_pub_id(&r.bidder.publisher_id, &r.bidder.memberid);
                    seller_id = r.bidder.seller_id.clone();
                    root = r;
                }
            }
        }

        let mut dmx_req = request.clone();
        let mut has_no_id = true;
        let pub_id_for_ext = user_seller_or_pub_id(&root.bidder.publisher_id, &root.bidder.memberid);

        if let Some(app) = dmx_req.app.as_mut() {
            // Go dereferences `*request.App.Publisher` without a nil check and panics.
            let Some(publisher) = app.publisher.as_mut() else {
                return (vec![], vec![BidderError::other("app.publisher is required")]);
            };
            if publisher.id.is_empty() {
                publisher.id = publisher_id.clone();
            }
            match pub_ext(&pub_id_for_ext) {
                Ok(e) => publisher.ext = Some(e),
                Err(e) => {
                    errs.push(e);
                    return (vec![], errs);
                }
            }
            if !app.id.is_empty() {
                has_no_id = false;
            }
            if has_no_id {
                if let Some(device) = &request.device {
                    if !device.ifa.is_empty() {
                        app.id = device.ifa.clone();
                        has_no_id = false;
                    }
                }
            }
        }

        if let Some(site) = dmx_req.site.as_mut() {
            // Go dereferences `*request.Site.Publisher` without a nil check and panics.
            let Some(publisher) = site.publisher.as_mut() else {
                return (vec![], vec![BidderError::other("site.publisher is required")]);
            };
            if publisher.id.is_empty() {
                publisher.id = publisher_id.clone();
            }
            match pub_ext(&pub_id_for_ext) {
                Ok(e) => publisher.ext = Some(e),
                Err(e) => {
                    errs.push(e);
                    return (vec![], errs);
                }
            }
        }

        if let Some(user) = &dmx_req.user {
            if !user.id.is_empty() {
                has_no_id = false;
            }
            if let Some(ext) = &user.ext {
                if let Ok(u) = ext.decode::<UserExtEids>() {
                    if !u.eids.is_empty() {
                        has_no_id = false;
                    }
                }
            }
        }

        for inst in &dmx_req.imp {
            let params = match unmarshal_dmx_ext(inst.ext.as_ref()) {
                Ok(p) => p,
                Err(e) => {
                    errs.push(e);
                    DmxExt::default()
                }
            };
            // Go's `isDmxParams(params.Bidder)` is always true (the static type is `dmxParams`).
            let has_pub = !params.bidder.publisher_id.is_empty() || !params.bidder.memberid.is_empty();
            if let Some(banner) = &inst.banner {
                if !banner.format.is_empty() {
                    if has_pub {
                        fetch_params(&params, inst, &mut imps, Some(banner.clone()), None);
                    } else {
                        return (vec![], vec![BidderError::other("Missing Params for auction to be send")]);
                    }
                }
            }
            if let Some(video) = &inst.video {
                if has_pub {
                    fetch_params(&params, inst, &mut imps, None, Some(video.clone()));
                } else {
                    return (vec![], vec![BidderError::other("Missing Params for auction to be send")]);
                }
            }
        }

        dmx_req.imp = imps;

        if has_no_id {
            return (vec![], vec![BidderError::other("This request contained no identifier")]);
        }

        let body = match crate::go_json::to_vec(&dmx_req) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::other(e.to_string()));
                return (vec![], errs);
            }
        };

        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: format!("{}{}", self.endpoint, add_params(&seller_id)),
                body,
                headers,
                imp_ids: dmx_req.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            errs,
        )
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        let mut errs = Vec::new();
        if is_response_status_code_no_content(response) {
            return (None, vec![]);
        }
        if response.status_code == 400 {
            return (None, vec![BidderError::bad_input("Unexpected status code 400")]);
        }
        if response.status_code != 200 {
            return (None, vec![BidderError::bad_input("Unexpected response no status code")]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut out = BidderResponse::with_bids_capacity(5);
        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                match get_media_type_for_imp(&bid.impid, &request.imp) {
                    Err(e) => errs.push(e),
                    Ok(t) => {
                        if t == BidType::Video {
                            bid.adm = video_imp_insertion(&bid);
                        }
                        out.bids.push(TypedBid::new(bid, t));
                    }
                }
            }
        }
        (Some(out), errs)
    }
}

fn fetch_params(
    params: &DmxExt,
    inst: &Imp,
    imps: &mut Vec<Imp>,
    banner: Option<Banner>,
    video: Option<Video>,
) {
    let mut temp = inst.clone();
    if params.bidder.bidfloor != 0.0 {
        temp.bidfloor = params.bidder.bidfloor;
    }
    if !params.bidder.tagid.is_empty() {
        temp.tagid = params.bidder.tagid.clone();
        temp.secure = Some(1);
    }
    if !params.bidder.dmxid.is_empty() {
        temp.tagid = params.bidder.dmxid.clone();
        temp.secure = Some(1);
    }
    if let Some(mut banner) = banner {
        if banner.h.is_none() || banner.w.is_none() {
            // The caller guarantees a non-empty format list.
            banner.h = Some(banner.format[0].h);
            banner.w = Some(banner.format[0].w);
        }
        temp.banner = Some(banner);
    }
    if let Some(mut video) = video {
        if video.protocols.is_empty() {
            video.protocols = PROTOCOLS.iter().map(|p| MediaCreativeSubtype(*p)).collect();
        }
        temp.video = Some(video);
    }
    if temp.tagid.is_empty() {
        return;
    }
    imps.push(temp);
}

fn add_params(s: &str) -> String {
    if s.is_empty() { String::new() } else { format!("?sellerid={}", query_escape(s)) }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> Result<BidType, BidderError> {
    let mut media_type = BidType::Banner;
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_none() && imp.video.is_some() {
                media_type = BidType::Video;
            }
            return Ok(media_type);
        }
    }
    Err(BidderError::bad_input(format!("Failed to find impression \"{imp_id}\" ")))
}

fn video_imp_insertion(bid: &Bid) -> String {
    let wrapped = format!("</Impression><Impression><![CDATA[{}]]></Impression>", bid.nurl);
    bid.adm.replacen("</Impression>", &wrapped, 1)
}
