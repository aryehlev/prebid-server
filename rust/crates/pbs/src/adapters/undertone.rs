//! Go `adapters/undertone/undertone.go`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp, Publisher};
use crate::ortb::Ext;

const ADAPTER_ID: i64 = 4;
const ADAPTER_VERSION: &str = "1.0.0";

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
struct UndertoneParams {
    id: i64,
    version: &'static str,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpUndertone {
    #[serde(rename = "publisherId")]
    publisher_id: i64,
    #[serde(rename = "placementId")]
    placement_id: i64,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ImpExtIn {
    bidder: Option<ExtImpUndertone>,
    gpid: String,
}

/// Go `impExt` when marshalled: `bidder` is nil (omitted) once the gpid case clears it.
#[derive(Serialize)]
struct ImpExtOut<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    bidder: Option<ExtImpUndertone2>,
    #[serde(skip_serializing_if = "str::is_empty")]
    gpid: &'a str,
}

#[derive(Serialize)]
struct ExtImpUndertone2 {
    #[serde(rename = "publisherId")]
    publisher_id: i64,
    #[serde(rename = "placementId")]
    placement_id: i64,
}

fn invalid_imp_err(imp_id: &str, err: impl std::fmt::Display) -> BidderError {
    BidderError::bad_input(format!("Invalid impid={imp_id}: {err}"))
}

fn get_imps_and_publisher_id(request: &BidRequest) -> (Vec<Imp>, i64, Vec<BidderError>) {
    let mut errs = Vec::new();
    let mut publisher_id = 0i64;
    let mut valid = Vec::new();
    for imp in &request.imp {
        let mut imp = imp.clone();
        let ext: ImpExtIn = match &imp.ext {
            None => {
                errs.push(invalid_imp_err(&imp.id, "expect { or n, but found \u{0}"));
                continue;
            }
            Some(e) if e.0.is_null_value() => ImpExtIn::default(),
            Some(e) => match e.decode() {
                Ok(v) => v,
                Err(err) => {
                    errs.push(invalid_imp_err(&imp.id, jsonutil_msg(e, err)));
                    continue;
                }
            },
        };
        // Go dereferences `ext.Bidder` (nil when absent) and would panic; treat as zero values.
        let bidder = ext.bidder.unwrap_or_default();
        if publisher_id == 0 {
            publisher_id = bidder.publisher_id;
        }
        imp.tagid = bidder.placement_id.to_string();
        imp.ext = None;
        if !ext.gpid.is_empty() {
            let out = ImpExtOut { bidder: None, gpid: &ext.gpid };
            match Ext::from_serialize(&out) {
                Ok(e) => imp.ext = Some(e),
                Err(err) => {
                    errs.push(invalid_imp_err(&imp.id, err));
                }
            }
        }
        valid.push(imp);
    }
    (valid, publisher_id, errs)
}

/// Go's message for a non-object `imp.ext` (`expect { or n, but found X`).
fn jsonutil_msg(ext: &Ext, err: sonic_rs::Error) -> String {
    use sonic_rs::JsonValueTrait;
    if !ext.0.is_object() {
        let text = ext.to_json();
        let first = text.chars().next().unwrap_or('\0');
        return format!("expect {{ or n, but found {first}");
    }
    err.to_string()
}

trait NullCheck {
    fn is_null_value(&self) -> bool;
}
impl NullCheck for sonic_rs::Value {
    fn is_null_value(&self) -> bool {
        use sonic_rs::JsonValueTrait;
        self.is_null()
    }
}

fn populate_site_app(req: &mut BidRequest, publisher_id: i64) {
    let pub_id = publisher_id.to_string();
    if let Some(site) = &mut req.site {
        let mut publisher: Publisher = site.publisher.clone().unwrap_or_default();
        publisher.id = pub_id;
        site.publisher = Some(publisher);
    } else if let Some(app) = &mut req.app {
        let mut publisher: Publisher = app.publisher.clone().unwrap_or_default();
        publisher.id = pub_id;
        app.publisher = Some(publisher);
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let (imps, publisher_id, mut errs) = get_imps_and_publisher_id(request);
        if imps.is_empty() {
            return (vec![], errs);
        }
        let mut copy = request.clone();
        copy.imp = imps;
        populate_site_app(&mut copy, publisher_id);
        match Ext::from_serialize(&UndertoneParams { id: ADAPTER_ID, version: ADAPTER_VERSION }) {
            Ok(e) => copy.ext = Some(e),
            Err(e) => errs.push(BidderError::other(e.to_string())),
        }
        let body = match crate::go_json::to_vec(&copy) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::other(e.to_string()));
                return (vec![], errs);
            }
        };
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers: Header::new(),
                imp_ids: copy.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            errs,
        )
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        match response_data.status_code {
            204 => return (None, vec![]),
            400 => {
                return (
                    None,
                    vec![BidderError::bad_input(
                        "Unexpected status code: 400. Bad request from publisher. Run with request.debug = 1 for more info.",
                    )],
                )
            }
            200 => {}
            code => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "Unexpected status code: {code}. Run with request.debug = 1 for more info."
                    ))],
                )
            }
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut types: HashMap<&str, BidType> = HashMap::new();
        for imp in &request.imp {
            if imp.banner.is_some() {
                types.insert(&imp.id, BidType::Banner);
            } else if imp.video.is_some() {
                types.insert(&imp.id, BidType::Video);
            }
        }
        let mut out = BidderResponse::with_bids_capacity(request.imp.len());
        out.currency = response.cur.clone();
        for sb in response.seatbid {
            for bid in sb.bid {
                let Some(&t) = types.get(bid.impid.as_str()) else { continue };
                out.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(out), vec![])
    }
}
