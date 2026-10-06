//! Go `adapters/adhese/adhese.go`.

use std::collections::BTreeMap;

use serde::Deserialize;
use sonic_rs::JsonValueTrait;

use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::bid_types::BidType;
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

/// Go `openrtb_ext.ExtImpAdhese`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtImpAdhese {
    account: String,
    location: String,
    format: String,
    #[serde(deserialize_with = "crate::ortb::de::null_default")]
    targets: BTreeMap<String, Vec<String>>,
}

pub struct Adapter {
    endpoint_template: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: &str) -> Result<Self, BidderError> {
        let t = EndpointTemplate::parse(endpoint)
            .map_err(|e| BidderError::other(format!("unable to parse endpoint url template: {e}")))?;
        Ok(Self { endpoint_template: t })
    }
}

/// Go: `Unmarshal(imp.Ext, &ExtImpBidder)` then `Unmarshal(bidderExt.Bidder, &T)`.
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

fn make_slot(p: &ExtImpAdhese) -> String {
    format!("{}-{}", p.location, p.format)
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        // Go indexes `request.Imp[0]` and panics on an empty imp list.
        let Some(imp) = request.imp.first() else {
            return (vec![], vec![BidderError::bad_input("no imps in the request")]);
        };

        let ext = match imp.ext.as_ref() {
            None => None,
            Some(e) => Some(e),
        };
        // Step 1 (ExtImpBidder) and step 2 (bidder params) have different error prefixes.
        let step1 = match ext {
            None => Err("expect { or n, but found \u{0}".to_string()),
            Some(e) if !e.0.is_object() && !e.0.is_null() => Err(format!(
                "expect {{ or n, but found {}",
                e.to_json().chars().next().unwrap_or('\u{0}')
            )),
            Some(_) => Ok(()),
        };
        if let Err(m) = step1 {
            return (vec![], vec![BidderError::bad_input(format!("Error unmarshalling imp.ext: {m}"))]);
        }
        let params: ExtImpAdhese = match imp_bidder_params(imp.ext.as_ref()) {
            Ok(p) => p,
            Err(m) => {
                return (vec![], vec![BidderError::bad_input(format!("Error unmarshalling bidder ext: {m}"))])
            }
        };

        let mut targets: BTreeMap<String, Vec<String>> = BTreeMap::new();
        targets.insert("SL".to_string(), vec![make_slot(&params)]);
        for (k, v) in &params.targets {
            targets.insert(k.clone(), v.clone());
        }

        let mut wrapper: BTreeMap<&str, &BTreeMap<String, Vec<String>>> = BTreeMap::new();
        wrapper.insert("adhese", &targets);
        let modified_ext = match crate::go_json::to_vec(&wrapper)
            .map_err(|e| e.to_string())
            .and_then(|b| Ext::from_slice(&b).map_err(|e| e.to_string()))
        {
            Ok(e) => e,
            Err(e) => return (vec![], vec![BidderError::bad_input(format!("Error marshalling modified ext: {e}"))]),
        };

        // Go assigns through the shared `Imp` slice; mutate a clone instead.
        let mut modified = request.clone();
        modified.imp[0].ext = Some(modified_ext);

        let p = EndpointTemplateParams { account_id: params.account.clone(), ..Default::default() };
        let endpoint = match self.endpoint_template.resolve(&p) {
            Ok(e) => e,
            Err(e) => {
                return (vec![], vec![BidderError::bad_server_response(format!("Error resolving macros: {e}"))])
            }
        };

        let body = match crate::go_json::to_vec(&modified) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::FailedToMarshal(e.to_string())]),
        };
        (
            vec![RequestData {
                method: "POST".into(),
                uri: endpoint,
                body,
                headers: Header::new(),
                imp_ids: vec![request.imp[0].id.clone()],
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if let Some(err) = check_response_status_code_for_errors(response_data) {
            return (None, vec![err]);
        }
        if is_response_status_code_no_content(response_data) {
            return (None, vec![]);
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(_) => return (None, vec![BidderError::bad_server_response("Empty body")]),
        };
        if response.seatbid.is_empty() {
            return (None, vec![BidderError::bad_server_response("Empty SeatBid")]);
        }
        let bids = &response.seatbid[0].bid;
        if bids.is_empty() {
            return (None, vec![BidderError::bad_server_response("Empty SeatBid.Bid")]);
        }
        let mut bid_response = BidderResponse::with_bids_capacity(bids.len());
        if !response.cur.is_empty() {
            bid_response.currency = response.cur.clone();
        }
        let mut bid = bids[0].clone();

        // Go: `jsonutil.Unmarshal(bid.Ext, &map[string]map[string]string)`.
        let wrapped: BTreeMap<String, Option<BTreeMap<String, String>>> = match bid.ext.as_ref() {
            None => {
                return (
                    None,
                    vec![BidderError::bad_server_response(
                        "BidExt parsing error. expect { or n, but found \u{0}",
                    )],
                )
            }
            Some(e) => {
                if !e.0.is_object() && !e.0.is_null() {
                    return (
                        None,
                        vec![BidderError::bad_server_response(format!(
                            "BidExt parsing error. expect {{ or n, but found {}",
                            e.to_json().chars().next().unwrap_or('\u{0}')
                        ))],
                    );
                }
                match e.decode::<Option<BTreeMap<String, Option<BTreeMap<String, String>>>>>() {
                    Ok(m) => m.unwrap_or_default(),
                    Err(er) => {
                        return (
                            None,
                            vec![BidderError::bad_server_response(format!("BidExt parsing error. {er}"))],
                        )
                    }
                }
            }
        };

        // Go indexes `request.Imp[0]` and panics on an empty imp list.
        let Some(imp0) = request.imp.first() else {
            return (None, vec![BidderError::bad_server_response("BidType error: no imps in the request")]);
        };
        let bid_type = match infer_bid_type_from_imp(imp0) {
            Ok(t) => t,
            Err(e) => {
                return (None, vec![BidderError::bad_server_response(format!("BidType error: {e}"))])
            }
        };

        let adhese = wrapped.get("adhese").cloned().flatten();
        let marshalled = match adhese {
            Some(m) => crate::go_json::to_vec(&m),
            None => Ok(b"null".to_vec()),
        };
        match marshalled.map_err(|e| e.to_string()).and_then(|b| Ext::from_slice(&b).map_err(|e| e.to_string())) {
            Ok(e) => bid.ext = Some(e),
            Err(e) => return (None, vec![BidderError::other(e)]),
        }

        bid_response.bids.push(TypedBid::new(bid, bid_type));
        (Some(bid_response), vec![])
    }
}

fn infer_bid_type_from_imp(i: &Imp) -> Result<BidType, BidderError> {
    let mut types = Vec::new();
    if i.banner.is_some() {
        types.push(BidType::Banner);
    }
    if i.video.is_some() {
        types.push(BidType::Video);
    }
    if i.native.is_some() {
        types.push(BidType::Native);
    }
    if i.audio.is_some() {
        types.push(BidType::Audio);
    }
    match types.len() {
        1 => Ok(types[0]),
        n if n > 1 => Err(BidderError::bad_server_response("Multiple media types detected, cannot infer")),
        _ => Err(BidderError::bad_server_response("Could not infer bid type from imp")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ortb::openrtb2::{Audio, Banner, Native, Pmp, Video};

    /// Go `TestInferBidTypeFromImp`.
    #[test]
    fn infer_bid_type() {
        let e = infer_bid_type_from_imp(&Imp::default()).unwrap_err();
        assert_eq!(e.to_string(), "Could not infer bid type from imp");
        let imp = |f: &dyn Fn(&mut Imp)| {
            let mut i = Imp::default();
            f(&mut i);
            i
        };
        assert_eq!(infer_bid_type_from_imp(&imp(&|i| i.banner = Some(Banner::default()))).unwrap(), BidType::Banner);
        assert_eq!(infer_bid_type_from_imp(&imp(&|i| i.native = Some(Native::default()))).unwrap(), BidType::Native);
        assert_eq!(infer_bid_type_from_imp(&imp(&|i| i.video = Some(Video::default()))).unwrap(), BidType::Video);
        assert_eq!(infer_bid_type_from_imp(&imp(&|i| i.audio = Some(Audio::default()))).unwrap(), BidType::Audio);
        let e = infer_bid_type_from_imp(&imp(&|i| i.pmp = Some(Pmp::default()))).unwrap_err();
        assert_eq!(e.to_string(), "Could not infer bid type from imp");
        let e = infer_bid_type_from_imp(&imp(&|i| {
            i.banner = Some(Banner::default());
            i.video = Some(Video::default());
        }))
        .unwrap_err();
        assert_eq!(e.to_string(), "Multiple media types detected, cannot infer");
    }
}
