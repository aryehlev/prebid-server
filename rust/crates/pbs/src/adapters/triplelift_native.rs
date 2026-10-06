//! Go `adapters/triplelift_native/triplelift_native.go`.

use std::collections::HashSet;

use serde::Deserialize;
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp, Publisher};
use crate::ortb::Ext;

pub struct Adapter {
    endpoint: String,
    publisher_whitelist_map: HashSet<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct TripleliftNativeExtInfo {
    publisher_whitelist: Vec<String>,
}

impl Adapter {
    /// Go `Builder`; `extra_adapter_info` is `config.Adapter.ExtraAdapterInfo`.
    pub fn new(endpoint: impl Into<String>, extra_adapter_info: &str) -> Result<Self, BidderError> {
        let extra = get_extra_info(extra_adapter_info)?;
        Ok(Self {
            endpoint: endpoint.into(),
            publisher_whitelist_map: extra.publisher_whitelist.into_iter().collect(),
        })
    }
}

fn get_extra_info(v: &str) -> Result<TripleliftNativeExtInfo, BidderError> {
    if v.is_empty() {
        return Ok(TripleliftNativeExtInfo::default());
    }
    jsonutil::unmarshal(v.as_bytes())
        .map_err(|e| BidderError::other(format!("invalid extra info: {e}")))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpData {
    tag_code: String,
}

/// Go `ExtImp`: embeds `*adapters.ExtImpBidder` and adds `data`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImp {
    bidder: Option<Ext>,
    data: Option<ExtImpData>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpTriplelift {
    #[serde(rename = "inventoryCode")]
    inv_code: String,
    floor: Option<f64>,
}

#[derive(Deserialize, Default)]
struct ExtPublisherPrebid {
    #[serde(rename = "parentAccount", default)]
    parent_account: Option<String>,
}

#[derive(Deserialize, Default)]
struct ExtPublisher {
    #[serde(default)]
    prebid: Option<ExtPublisherPrebid>,
}

/// Go `jsonutil.Unmarshal(raw, &v)` on a `json.RawMessage` that may be absent or not an object:
/// json-iterator reports `expect { or n, but found X` for anything but an object or `null`.
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

fn msn_in_app(request: &BidRequest) -> bool {
    request.app.as_ref().and_then(|a| a.publisher.as_ref()).is_some_and(|p| p.domain == "msn.com")
}

fn msn_in_site(request: &BidRequest) -> bool {
    request.site.as_ref().and_then(|s| s.publisher.as_ref()).is_some_and(|p| p.domain == "msn.com")
}

fn process_imp(imp: &mut Imp, request: &BidRequest) -> Result<(), BidderError> {
    let ext: ExtImp = unmarshal_ext(imp.ext.as_ref())?;
    let tlext: ExtImpTriplelift = unmarshal_ext(ext.bidder.as_ref())?;
    if imp.native.is_none() {
        return Err(BidderError::other("no native object specified"));
    }
    match &ext.data {
        Some(d) if !d.tag_code.is_empty() && (msn_in_site(request) || msn_in_app(request)) => {
            imp.tagid = d.tag_code.clone();
        }
        _ => imp.tagid = tlext.inv_code,
    }
    if let Some(floor) = tlext.floor {
        imp.bidfloor = floor;
    }
    Ok(())
}

fn effective_pub_id(publisher: Option<&Publisher>) -> String {
    if let Some(pub_) = publisher {
        if let Some(ext) = &pub_.ext {
            if let Ok(pub_ext) = unmarshal_ext::<ExtPublisher>(Some(ext)) {
                if let Some(parent) = pub_ext.prebid.and_then(|p| p.parent_account) {
                    if !parent.is_empty() {
                        return parent;
                    }
                }
            }
        }
        if !pub_.id.is_empty() {
            return pub_.id.clone();
        }
    }
    "unknown".into()
}

/// Go `getPublisher`: `request.Site.Publisher` panics on a nil site; treated as no publisher.
fn get_publisher(request: &BidRequest) -> Option<&Publisher> {
    if let Some(app) = &request.app {
        return app.publisher.as_ref();
    }
    request.site.as_ref().and_then(|s| s.publisher.as_ref())
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _extra: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::with_capacity(request.imp.len() + 1);
        let mut tl_request = request.clone();
        let mut valid_imps = Vec::new();
        for imp in &request.imp {
            let mut imp = imp.clone();
            match process_imp(&mut imp, request) {
                Ok(()) => valid_imps.push(imp),
                Err(e) => errs.push(e),
            }
        }
        let publisher_id = effective_pub_id(get_publisher(request));
        if !self.publisher_whitelist_map.contains(&publisher_id) {
            return (vec![], vec![BidderError::other("Unsupported publisher for triplelift_native")]);
        }
        if valid_imps.is_empty() {
            errs.push(BidderError::other("No valid impressions for triplelift"));
            return (vec![], errs);
        }
        tl_request.imp = valid_imps;
        let body = match crate::go_json::to_vec(&tl_request) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::other(e.to_string()));
                return (vec![], errs);
            }
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: tl_request.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            errs,
        )
    }

    fn make_bids(
        &self,
        _internal_request: &BidRequest,
        _external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 204 {
            return (None, vec![]);
        }
        let msg = format!(
            "Unexpected status code: {}. Run with request.debug = 1 for more info",
            response.status_code
        );
        if response.status_code == 400 {
            return (None, vec![BidderError::bad_input(msg)]);
        }
        if response.status_code != 200 {
            return (None, vec![BidderError::bad_server_response(msg)]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let count: usize = bid_resp.seatbid.iter().map(|sb| sb.bid.len()).sum();
        let mut out = BidderResponse::with_bids_capacity(count);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                out.bids.push(TypedBid::new(bid, BidType::Native));
            }
        }
        (Some(out), vec![])
    }
}
