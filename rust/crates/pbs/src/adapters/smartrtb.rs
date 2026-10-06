//! Go `adapters/smartrtb/smartrtb.go`.

use serde::{Deserialize, Serialize};

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

const CREATIVE_TYPE_BANNER: &str = "BANNER";
const CREATIVE_TYPE_VIDEO: &str = "VIDEO";

pub struct Adapter {
    endpoint_template: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: &str) -> Result<Self, String> {
        let endpoint_template = EndpointTemplate::parse(endpoint)
            .map_err(|e| format!("unable to parse endpoint url template: {e}"))?;
        Ok(Self { endpoint_template })
    }
}

/// Bid request extension appended to the downstream request.
#[derive(Serialize, Default)]
struct BidRequestExt {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    zone_id: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    force_bid: bool,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidExt {
    format: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpSmartRtb {
    pub_id: String,
    med_id: String,
    zone_id: String,
    force_bid: bool,
}

/// `jsonutil::unmarshal` with json-iterator's wording for empty input (nil ext).
fn unmarshal_obj<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T, BidderError> {
    if data.iter().all(|b| b" \t\r\n".contains(b)) {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()));
    }
    jsonutil::unmarshal(data)
}

fn ext_text(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

fn parse_ext_imp(dst: &mut BidRequestExt, imp: &mut Imp) -> Result<(), BidderError> {
    let ext: ExtImpBidder =
        unmarshal_obj(&ext_text(&imp.ext)).map_err(|e| BidderError::bad_input(e.to_string()))?;
    let bytes = ext_text(&ext.bidder);
    let src: ExtImpSmartRtb = unmarshal_obj(&bytes).map_err(|e| {
        // json-iterator names the offending Go field; serde's wording is replaced for strings.
        let msg = match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(v) => ["pub_id", "med_id", "zone_id"]
                .iter()
                .zip(["PubID", "MedID", "ZoneID"])
                .find_map(|(k, f)| match v.get(*k) {
                    Some(x) if !x.is_string() && !x.is_null() => Some(format!(
                        "cannot unmarshal {f}: expects \" or n, but found {}",
                        x.to_string().chars().next().unwrap_or(' ')
                    )),
                    _ => None,
                })
                .unwrap_or_else(|| e.to_string()),
            Err(_) => e.to_string(),
        };
        BidderError::bad_input(msg)
    })?;
    if dst.pub_id.is_empty() {
        dst.pub_id = src.pub_id;
    }
    if !src.zone_id.is_empty() {
        imp.tagid = src.zone_id;
    }
    let _ = (src.med_id, src.force_bid);
    Ok(())
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        brq: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut imps: Vec<Imp> = vec![];
        let mut ext = BidRequestExt::default();
        let mut errs = vec![];
        for imp in &brq.imp {
            if imp.banner.is_none() && imp.video.is_none() {
                continue;
            }
            let mut imp = imp.clone();
            if let Err(e) = parse_ext_imp(&mut ext, &mut imp) {
                errs.push(e);
                continue;
            }
            imps.push(imp);
        }
        if imps.is_empty() {
            return (vec![], errs);
        }
        if ext.pub_id.is_empty() {
            errs.push(BidderError::bad_input("Cannot infer publisher ID from bid ext"));
            return (vec![], errs);
        }
        let mut req = brq.clone();
        req.ext = match Ext::from_serialize(&ext) {
            Ok(e) => Some(e),
            Err(e) => {
                errs.push(BidderError::other(e.to_string()));
                return (vec![], errs);
            }
        };
        req.imp = imps;
        let rq = match crate::go_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::other(e.to_string()));
                return (vec![], errs);
            }
        };
        let params = EndpointTemplateParams { publisher_id: ext.pub_id.clone(), ..Default::default() };
        let url = match self.endpoint_template.resolve(&params) {
            Ok(u) => u,
            Err(e) => {
                errs.push(BidderError::other(e));
                return (vec![], errs);
            }
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        headers.add("x-openrtb-version", "2.5");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: url,
                body: rq,
                headers,
                imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            errs,
        )
    }

    fn make_bids(
        &self,
        _brq: &BidRequest,
        _drq: &RequestData,
        rs: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if rs.status_code == 204 {
            return (None, vec![]);
        } else if rs.status_code == 400 {
            return (None, vec![BidderError::bad_input("Invalid request.")]);
        } else if rs.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!("Unexpected HTTP status {}.", rs.status_code))],
            );
        }
        let brs: BidResponse = match jsonutil::unmarshal(&rs.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut rv = BidderResponse::with_bids_capacity(5);
        for seat in brs.seatbid {
            for mut bid in seat.bid {
                let ext: BidExt = match jsonutil::unmarshal(&ext_text(&bid.ext)) {
                    Ok(e) => e,
                    Err(_) => {
                        return (
                            None,
                            vec![BidderError::bad_server_response("Invalid bid extension from endpoint.")],
                        )
                    }
                };
                let btype = match ext.format.as_str() {
                    CREATIVE_TYPE_BANNER => BidType::Banner,
                    CREATIVE_TYPE_VIDEO => BidType::Video,
                    other => {
                        return (
                            None,
                            vec![BidderError::bad_server_response(format!(
                                "Unsupported creative type {other}."
                            ))],
                        )
                    }
                };
                bid.ext = None;
                rv.bids.push(TypedBid::new(bid, btype));
            }
        }
        (Some(rv), vec![])
    }
}
