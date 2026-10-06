//! Go `adapters/vungle/vungle.go`.

use serde::{Deserialize, Serialize};
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{App, BidRequest, BidResponse};
use crate::ortb::Ext;

const SUPPORTED_CURRENCY: &str = "USD";

/// Go `openrtb_ext.ImpExtVungle`.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct ImpExtVungle {
    bid_token: String,
    #[serde(rename = "app_store_id")]
    pub_app_store_id: String,
    #[serde(rename = "placement_reference_id")]
    placement_ref_id: String,
}

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

/// Go `vungleImpressionExt` marshalled: the embedded `*ExtImpBidder` (`prebid`, `bidder`, `ae`
/// omitted when zero) followed by `vungle`.
fn build_imp_ext(ext: &Ext, vungle: &ImpExtVungle) -> Result<Ext, String> {
    let prebid = ext.0.get("prebid").map(|v| v.to_string()).unwrap_or_else(|| "null".to_string());
    let bidder = ext.0.get("bidder").map(|v| v.to_string()).unwrap_or_else(|| "null".to_string());
    let mut out = format!("{{\"prebid\":{prebid},\"bidder\":{bidder}");
    if let Some(ae) = ext.0.get("ae").and_then(|v| v.as_i64()) {
        if ae != 0 {
            out.push_str(&format!(",\"ae\":{ae}"));
        }
    }
    out.push_str(&format!(",\"vungle\":{}}}", serde_json::to_string(vungle).map_err(|e| e.to_string())?));
    Ext::from_slice(out.as_bytes()).map_err(|e| e.to_string())
}

/// Go: `jsonutil.Unmarshal(imp.Ext, &ExtImpBidder)` then `jsonutil.Unmarshal(bidderExt.Bidder, &T)`.
/// Returns the Go error message text of whichever step fails.
fn imp_bidder_params<T: serde::de::DeserializeOwned>(ext: Option<&Ext>) -> Result<T, String> {
    fn not_obj(v: &sonic_rs::Value) -> Option<String> {
        if v.is_object() || v.is_null() {
            return None;
        }
        let first = sonic_rs::to_string(v).ok()?.chars().next().unwrap_or('\u{0}');
        Some(format!("expect {{ or n, but found {first}"))
    }
    let Some(ext) = ext else {
        return Err("expect { or n, but found \u{0}".to_string());
    };
    if let Some(m) = not_obj(&ext.0) {
        return Err(m);
    }
    match ext.0.get("bidder") {
        None => Err("expect { or n, but found \u{0}".to_string()),
        Some(b) => {
            if let Some(m) = not_obj(b) {
                return Err(m);
            }
            sonic_rs::from_value::<T>(b).map_err(|e| e.to_string())
        }
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        let mut request_copy = request.clone();

        for imp in &request.imp {
            let mut imp = imp.clone();
            if imp.bidfloor > 0.0
                && !imp.bidfloorcur.is_empty()
                && imp.bidfloorcur.to_uppercase() != SUPPORTED_CURRENCY
            {
                match req_info.convert_currency(imp.bidfloor, &imp.bidfloorcur, SUPPORTED_CURRENCY) {
                    Ok(v) => {
                        imp.bidfloorcur = SUPPORTED_CURRENCY.to_string();
                        imp.bidfloor = v;
                    }
                    Err(e) => {
                        errs.push(BidderError::other(format!("failed to convert currency (err){e}")));
                        continue;
                    }
                }
            }

            let Some(ext) = imp.ext.clone() else {
                errs.push(BidderError::other(
                    "failed unmarshalling imp ext (err)expect { or n, but found \u{0}",
                ));
                continue;
            };
            if !ext.0.is_object() && !ext.0.is_null() {
                errs.push(BidderError::other(format!(
                    "failed unmarshalling imp ext (err)expect {{ or n, but found {}",
                    ext.to_json().chars().next().unwrap_or('\u{0}')
                )));
                continue;
            }
            let mut bidder_imp_ext: ImpExtVungle = match ext.0.get("bidder") {
                None => {
                    errs.push(BidderError::other(
                        "failed unmarshalling bidder imp ext (err)expect { or n, but found \u{0}",
                    ));
                    continue;
                }
                // `Ext::decode` matches keys ignoring case and rejects an array for a struct, as Go does.
                Some(b) => match Ext(b.clone()).decode() {
                    Ok(v) => v,
                    Err(e) => {
                        errs.push(BidderError::other(format!("failed unmarshalling bidder imp ext (err){e}")));
                        continue;
                    }
                },
            };

            // Go dereferences `requestCopy.User` and panics when user is nil.
            let Some(user) = request_copy.user.as_ref() else {
                errs.push(BidderError::other("failed constructing app, bid request has no user object"));
                continue;
            };
            bidder_imp_ext.bid_token = user.buyeruid.clone();
            match build_imp_ext(&ext, &bidder_imp_ext) {
                Ok(e) => imp.ext = Some(e),
                Err(_) => {
                    errs.push(BidderError::other("failed re-marshalling imp ext"));
                    continue;
                }
            }

            imp.tagid = bidder_imp_ext.placement_ref_id.clone();
            request_copy.imp = vec![imp];

            let request_app_copy = if let Some(app) = &request.app {
                let mut a = app.clone();
                a.id = bidder_imp_ext.pub_app_store_id.clone();
                a
            } else if request.site.is_some() {
                request_copy.site = None;
                App { id: bidder_imp_ext.pub_app_store_id.clone(), ..Default::default() }
            } else {
                errs.push(BidderError::other(
                    "failed constructing app, must have app or site object in bid request",
                ));
                continue;
            };
            request_copy.app = Some(request_app_copy);

            let body = match crate::go_json::to_vec(&request_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::other(e.to_string()));
                    continue;
                }
            };

            // Go builds this with a composite literal, so the keys are not canonicalised
            // (`X-OpenRTB-Version`); `Header` has no raw insert, so go through its serde form.
            let headers: Header = serde_json::from_value(serde_json::json!({
                "Content-Type": ["application/json"],
                "Accept": ["application/json"],
                "X-OpenRTB-Version": ["2.5"],
            }))
            .unwrap_or_default();
            requests.push(RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: request_copy.imp.iter().map(|i| i.id.clone()).collect(),
            });
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
        if let Some(err) = check_response_status_code_for_errors(response_data) {
            return (None, vec![err]);
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        bid_response.currency = response.cur;
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                let mut t = TypedBid::new(bid, BidType::Video);
                t.seat = seat_bid.seat.clone();
                bid_response.bids.push(t);
            }
        }
        (Some(bid_response), vec![])
    }
}
