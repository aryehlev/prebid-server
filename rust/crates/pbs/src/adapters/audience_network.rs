//! Go `adapters/audienceNetwork/facebook.go`.

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

use crate::bidder::TimeoutBidder;
use crate::config;
use crate::ext_helpers::ext_remove;
use crate::ortb::openrtb2::Publisher;

/// Go `jsonutil.Unmarshal` of an absent `json.RawMessage`: jsoniter reports a NUL byte.
fn unmarshal_raw<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T, BidderError> {
    if data.iter().all(|b| b" \t\r\n".contains(b)) {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()));
    }
    jsonutil::unmarshal(data)
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpFacebook {
    #[serde(rename = "placementId", alias = "placementid", alias = "PlacementId")]
    placement_id: String,
    #[serde(rename = "publisherId", alias = "publisherid", alias = "PublisherId")]
    publisher_id: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct FacebookAdMarkup {
    bid_id: String,
}

#[derive(Serialize)]
struct FacebookReqExt {
    platformid: String,
    authentication_id: String,
}

pub struct Adapter {
    uri: String,
    platform_id: String,
    app_secret: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(cfg: &config::Adapter) -> Result<Self, BidderError> {
        if cfg.platform_id.is_empty() {
            return Err(BidderError::other(
                "PartnerID is not configured. Did you set adapters.facebook.platform_id in the app config?",
            ));
        }
        if cfg.app_secret.is_empty() {
            return Err(BidderError::other(
                "AppSecret is not configured. Did you set adapters.facebook.app_secret in the app config?",
            ));
        }
        Ok(Self { uri: cfg.endpoint.clone(), platform_id: cfg.platform_id.clone(), app_secret: cfg.app_secret.clone() })
    }

    /// The authentication ID is a sha256 hmac hash encoded as a hex string, based on the app
    /// secret and the ID of the bid request.
    fn make_auth_id(&self, req: &BidRequest) -> String {
        hex(&hmac_sha256(self.app_secret.as_bytes(), req.id.as_bytes()))
    }

    fn modify_request(&self, out: &mut BidRequest) -> Result<(), BidderError> {
        // Go panics unless there is exactly one imp; callers here always pass one.
        let (plmt_id, pub_id) = extract_placement_and_publisher(&out.imp[0])?;

        // Every outgoing FAN request has a single impression, so the imp ID is the request ID. It
        // must be set before the auth ID, which hashes the request ID.
        out.id = out.imp[0].id.clone();

        let req_ext = FacebookReqExt { platformid: self.platform_id.clone(), authentication_id: self.make_auth_id(out) };
        let bytes = crate::go_json::to_vec(&req_ext).map_err(|e| BidderError::other(e.to_string()))?;
        out.ext = Some(Ext::from_slice(&bytes).map_err(|e| BidderError::other(e.to_string()))?);

        out.imp[0].tagid = format!("{pub_id}_{plmt_id}");
        out.imp[0].ext = None;

        if let Some(app) = out.app.as_mut() {
            app.publisher = Some(Publisher { id: pub_id, ..Default::default() });
        }

        modify_imp(&mut out.imp[0])
    }
}

fn modify_imp(out: &mut Imp) -> Result<(), BidderError> {
    let imp_type = resolve_imp_type(out);

    if out.instl == 1 && imp_type != BidType::Banner {
        return Err(BidderError::bad_input(format!(
            "imp #{}: interstitial imps are only supported for banner",
            out.id
        )));
    }

    if imp_type == BidType::Banner {
        let id = out.id.clone();
        let instl = out.instl;
        let banner = out.banner.as_mut().expect("banner imp");
        if instl == 1 {
            banner.w = Some(0);
            banner.h = Some(0);
            banner.format = Vec::new();
            return Ok(());
        }
        if banner.h.is_none() {
            for f in &banner.format {
                if f.h == 50 || f.h == 250 {
                    banner.h = Some(f.h);
                    break;
                }
            }
            if banner.h.is_none() {
                return Err(BidderError::bad_input(format!("imp #{id}: banner height required")));
            }
        }
        let h = banner.h.unwrap();
        if h != 50 && h != 250 {
            return Err(BidderError::bad_input(format!("imp #{id}: only banner heights 50 and 250 are supported")));
        }
        banner.w = Some(-1);
        banner.format = Vec::new();
    }
    Ok(())
}

fn extract_placement_and_publisher(imp: &Imp) -> Result<(String, String), BidderError> {
    let outer: ExtImpBidder =
        unmarshal_raw(&ext_bytes(&imp.ext)).map_err(|e| BidderError::bad_input(e.to_string()))?;
    let fb: ExtImpFacebook =
        unmarshal_raw(&ext_bytes(&outer.bidder)).map_err(|e| BidderError::bad_input(e.to_string()))?;
    if fb.placement_id.is_empty() {
        return Err(BidderError::bad_input("Missing placementId param"));
    }
    let mut placement_id = fb.placement_id;
    let mut publisher_id = fb.publisher_id;

    // Support the legacy path where the caller passes an underscore concatenated string with the
    // publisherId and placementId.
    let toks: Vec<&str> = placement_id.split('_').collect();
    if toks.len() == 1 {
        if publisher_id.is_empty() {
            return Err(BidderError::bad_input("Missing publisherId param"));
        }
        return Ok((placement_id, publisher_id));
    } else if toks.len() == 2 {
        publisher_id = toks[0].to_string();
        placement_id = toks[1].to_string();
    } else {
        return Err(BidderError::bad_input(format!(
            "Invalid placementId param '{placement_id}' and publisherId param '{publisher_id}'"
        )));
    }
    Ok((placement_id, publisher_id))
}

/// Go `modifyImpCustom`: video and native imps get non-OpenRTB fields after serialization. The
/// request is re-encoded through a map, so keys come out sorted (serde_json's `Value` sorts too).
fn modify_imp_custom(json_data: Vec<u8>, imp: &Imp) -> Result<Vec<u8>, BidderError> {
    let imp_type = resolve_imp_type(imp);
    if imp_type != BidType::Video && imp_type != BidType::Native {
        return Ok(json_data);
    }
    let mut json_map: serde_json::Value = jsonutil::unmarshal(&json_data)?;
    let imp_map = match json_map.get_mut("imp") {
        Some(serde_json::Value::Array(a)) => match a.first_mut() {
            Some(serde_json::Value::Object(o)) => o,
            Some(_) => return Err(BidderError::other("unexpected type for imp[0] found in json data")),
            None => return Err(BidderError::other("unable to find imp[0] in json data")),
        },
        _ => return Err(BidderError::other("unable to find imp in json data")),
    };
    match imp_type {
        BidType::Video => {
            let Some(serde_json::Value::Object(video)) = imp_map.get_mut("video") else {
                return Err(BidderError::other("unable to find imp[0].video in json data"));
            };
            // the openrtb library omits video.w/h if set to zero, so force them to zero
            video.insert("w".into(), 0.into());
            video.insert("h".into(), 0.into());
        }
        _ => {
            let Some(serde_json::Value::Object(native)) = imp_map.get_mut("native") else {
                return Err(BidderError::other("unable to find imp[0].video in json data"));
            };
            // w/h are -1 for native impressions per the facebook native spec; the FAN adserver
            // does not expect the native request payload.
            native.insert("w".into(), (-1).into());
            native.insert("h".into(), (-1).into());
            native.remove("ver");
            native.remove("request");
        }
    }
    crate::go_json::to_vec(&json_map).map_err(|e| BidderError::other(format!("unable to encode json data ({e})")))
}

fn resolve_imp_type(imp: &Imp) -> BidType {
    if imp.banner.is_some() {
        return BidType::Banner;
    }
    if imp.video.is_some() {
        return BidType::Video;
    }
    if imp.audio.is_some() {
        return BidType::Audio;
    }
    if imp.native.is_some() {
        return BidType::Native;
    }
    BidType::Banner
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::bad_input("No impressions provided")]);
        }
        let buyer_uid = match request.user.as_ref() {
            Some(u) if !u.buyeruid.is_empty() => u.buyeruid.clone(),
            _ => return (vec![], vec![BidderError::bad_input("Missing bidder token in 'user.buyeruid'")]),
        };
        if request.site.is_some() {
            return (vec![], vec![BidderError::bad_input("Site impressions are not supported.")]);
        }

        // Documentation suggests splitting by impression so each request has a single imp.
        let mut reqs = Vec::with_capacity(request.imp.len());
        let mut errs = Vec::new();
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        headers.add("X-Fb-Pool-Routing-Token", buyer_uid);

        for imp in &request.imp {
            let mut fbreq = request.clone();
            fbreq.imp = vec![imp.clone()];

            if let Err(e) = self.modify_request(&mut fbreq) {
                errs.push(e);
                continue;
            }

            // `jsonutil.DropElement(body, "consented_providers_settings")` removes the first such key
            // in the serialized body; in practice that is `user.ext.consented_providers_settings`.
            if let Some(user) = fbreq.user.as_mut() {
                if let Err(e) = ext_remove(&mut user.ext, "consented_providers_settings") {
                    errs.push(BidderError::other(e.to_string()));
                    return (reqs, errs);
                }
            }

            let body = match crate::go_json::to_vec(&fbreq) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::other(e.to_string()));
                    continue;
                }
            };
            let body = match modify_imp_custom(body, &fbreq.imp[0]) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };

            reqs.push(RequestData {
                method: "POST".into(),
                uri: self.uri.clone(),
                body,
                headers: headers.clone(),
                imp_ids: fbreq.imp.iter().map(|i| i.id.clone()).collect(),
            });
        }
        (reqs, errs)
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _adapter_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 204 {
            return (None, vec![]);
        }
        if response.status_code != 200 {
            let msg = response.headers.get("x-fb-an-errors");
            return (
                None,
                vec![BidderError::bad_input(format!(
                    "Unexpected status code {} with error message '{}'",
                    response.status_code, msg
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut out = BidderResponse::with_bids_capacity(4);
        let mut errs = Vec::new();
        for seatbid in bid_resp.seatbid {
            for mut bid in seatbid.bid {
                if bid.adm.is_empty() {
                    errs.push(BidderError::bad_server_response(format!("Bid {} missing 'adm'", bid.id)));
                    continue;
                }
                let obj: FacebookAdMarkup = match jsonutil::unmarshal(bid.adm.as_bytes()) {
                    Ok(o) => o,
                    Err(e) => {
                        errs.push(BidderError::bad_server_response(e.to_string()));
                        continue;
                    }
                };
                if obj.bid_id.is_empty() {
                    errs.push(BidderError::bad_server_response(format!(
                        "bid {} missing 'bid_id' in 'adm'",
                        bid.id
                    )));
                    continue;
                }
                bid.adid = obj.bid_id.clone();
                bid.crid = obj.bid_id;
                // Go panics when the bid's imp ID matches no imp; report it instead.
                let Some(imp) = request.imp.iter().find(|i| i.id == bid.impid) else {
                    return (
                        None,
                        vec![BidderError::other(format!(
                            "Invalid bid imp ID {} does not match any imp IDs from the original bid request",
                            bid.impid
                        ))],
                    );
                };
                let t = resolve_imp_type(imp);
                out.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(out), errs)
    }
}

impl TimeoutBidder for Adapter {
    fn make_timeout_notification(&self, req: &RequestData) -> Result<RequestData, BidderError> {
        // The facebook adserver handles single-imp requests, so split requests carry the imp ID as
        // their ID; the publisher ID is expected in the app object.
        let parsed: serde_json::Value = serde_json::from_slice(&req.body)
            .map_err(|_| BidderError::other("Malformed JSON error"))?;
        let r_id = match parsed.get("id") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => return Err(BidderError::other(format!("Value is not a string: {v}"))),
            None => return Err(BidderError::other("Key path not found")),
        };
        let pub_id = match parsed.get("app").and_then(|a| a.get("publisher")).and_then(|p| p.get("id")) {
            Some(serde_json::Value::String(s)) => s.clone(),
            _ => return Err(BidderError::other("path app.publisher.id not found in the request")),
        };
        let uri = format!(
            "https://www.facebook.com/audiencenetwork/nurl/?partner={}&app={}&auction={}&ortb_loss_code=2",
            self.platform_id, pub_id, r_id
        );
        Ok(RequestData { method: "GET".into(), uri, body: Vec::new(), headers: Header::new(), imp_ids: vec![] })
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 (FIPS 180-4), kept local so the adapter needs no extra crate.
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 32];
    for (i, v) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut k = if key.len() > 64 { sha256(key).to_vec() } else { key.to_vec() };
    k.resize(64, 0);
    let mut inner: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
    inner.extend_from_slice(msg);
    let inner_hash = sha256(&inner);
    let mut outer: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();
    outer.extend_from_slice(&inner_hash);
    sha256(&outer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_rfc4231_case_2() {
        assert_eq!(
            hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn builder_requires_platform_id_and_secret() {
        let mut cfg = config::Adapter::with_endpoint("https://an.facebook.com/placementbid.ortb");
        assert_eq!(
            Adapter::new(&cfg).err().unwrap().to_string(),
            "PartnerID is not configured. Did you set adapters.facebook.platform_id in the app config?"
        );
        cfg.platform_id = "p".into();
        assert_eq!(
            Adapter::new(&cfg).err().unwrap().to_string(),
            "AppSecret is not configured. Did you set adapters.facebook.app_secret in the app config?"
        );
    }

    #[test]
    fn timeout_notification_app() {
        let cfg = config::Adapter {
            endpoint: "https://an.facebook.com/placementbid.ortb".into(),
            platform_id: "test-platform-id".into(),
            app_secret: "s".into(),
            ..Default::default()
        };
        let a = Adapter::new(&cfg).unwrap();
        let req = RequestData {
            body: br#"{"id":"1234","imp":[{"id":"1234"}],"app":{"publisher":{"id":"5678"}}}"#.to_vec(),
            ..Default::default()
        };
        let t = a.make_timeout_notification(&req).unwrap();
        assert_eq!(
            t.uri,
            "https://www.facebook.com/audiencenetwork/nurl/?partner=test-platform-id&app=5678&auction=1234&ortb_loss_code=2"
        );
        let bad = RequestData { body: br#"{"imp":[{{"id":"1234"}}"#.to_vec(), ..Default::default() };
        assert!(a.make_timeout_notification(&bad).is_err());
    }
}
