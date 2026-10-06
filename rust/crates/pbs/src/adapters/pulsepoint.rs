//! Go `adapters/pulsepoint/pulsepoint.go`.

use std::collections::HashMap;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp, Publisher};
use crate::ortb::Ext;

pub struct Adapter {
    uri: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Result<Self, BidderError> {
        Ok(Self { uri: endpoint.into() })
    }
}

/// Go `jsonutil.StringInt` for one field of `openrtb_ext.ExtImpPulsePoint`.
fn string_int(map: &serde_json::Map<String, serde_json::Value>, key: &str, field: &str) -> Result<i64, BidderError> {
    let bad = || {
        BidderError::FailedToUnmarshal(format!(
            "cannot unmarshal openrtb_ext.ExtImpPulsePoint.{field}: Value looks like Number/Boolean/None, but can't find its end: ',' or '}}' symbol"
        ))
    };
    match map.get(key) {
        None | Some(serde_json::Value::Null) => Ok(0),
        Some(serde_json::Value::String(s)) => {
            // Go strips the first and last byte of the raw text (the quotes).
            if s.is_empty() {
                Ok(0)
            } else {
                s.parse::<i64>().map_err(|_| bad())
            }
        }
        Some(serde_json::Value::Number(n)) => n.to_string().parse::<i64>().map_err(|_| bad()),
        Some(_) => Err(bad()),
    }
}

fn parse_param(name: &str, value: i64) -> Result<String, BidderError> {
    if value == 0 {
        return Err(BidderError::bad_input(format!("param not found - {name}")));
    }
    Ok(value.to_string())
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut pub_id = String::new();
        let mut imps: Vec<Imp> = Vec::with_capacity(request.imp.len());

        for imp in &request.imp {
            let mut imp = imp.clone();
            let bidder_ext: ExtImpBidder = match unmarshal_ext(imp.ext.as_ref()) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::bad_input(e.to_string()));
                    continue;
                }
            };
            // Go unmarshals into ExtImpPulsePoint; the StringInt fields are parsed by hand to keep
            // Go's error text.
            let pp: serde_json::Map<String, serde_json::Value> = match &bidder_ext.bidder {
                None => {
                    errs.push(BidderError::bad_input("expect { or n, but found \u{0}"));
                    continue;
                }
                Some(b) => match unmarshal_ext::<serde_json::Map<String, serde_json::Value>>(Some(b)) {
                    Ok(m) => m,
                    Err(e) => {
                        errs.push(BidderError::bad_input(e.to_string()));
                        continue;
                    }
                },
            };
            let cp = string_int(&pp, "cp", "PubID");
            let ct = string_int(&pp, "ct", "TagID");
            let (cp, ct) = match (cp, ct) {
                (Err(e), _) | (_, Err(e)) => {
                    errs.push(BidderError::bad_input(e.to_string()));
                    continue;
                }
                (Ok(a), Ok(b)) => (a, b),
            };
            if pub_id.is_empty() {
                match parse_param("pubID", cp) {
                    Ok(p) => pub_id = p,
                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                }
            }
            match parse_param("tagID", ct) {
                Ok(t) => imp.tagid = t,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            }
            imps.push(imp);
        }

        if imps.is_empty() {
            return (vec![], errs);
        }

        let mut request = request.clone();
        if let Some(site) = request.site.as_mut() {
            match site.publisher.as_mut() {
                Some(p) => p.id = pub_id.clone(),
                None => site.publisher = Some(Publisher { id: pub_id.clone(), ..Default::default() }),
            }
        } else if let Some(app) = request.app.as_mut() {
            match app.publisher.as_mut() {
                Some(p) => p.id = pub_id.clone(),
                None => app.publisher = Some(Publisher { id: pub_id.clone(), ..Default::default() }),
            }
        }
        request.imp = imps;
        let body = match crate::go_json::to_vec(&request) {
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
                uri: self.uri.clone(),
                body,
                headers,
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            errs,
        )
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 204 {
            return (None, vec![]);
        }
        if response.status_code == 400 {
            return (
                None,
                vec![BidderError::bad_input(format!("Bad user input: HTTP status {}", response.status_code))],
            );
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Bad server response: HTTP status {}",
                    response.status_code
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(5);
        // Go map assignment: the last imp with a given id wins.
        let mut imps_by_id: HashMap<&str, &Imp> = HashMap::new();
        for imp in &internal_request.imp {
            imps_by_id.insert(imp.id.as_str(), imp);
        }
        let default_imp = Imp::default();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let imp = imps_by_id.get(bid.impid.as_str()).copied().unwrap_or(&default_imp);
                if let Some(t) = get_bid_type(imp) {
                    bid_response.bids.push(TypedBid::new(bid, t));
                }
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_bid_type(imp: &Imp) -> Option<BidType> {
    if imp.banner.is_some() {
        Some(BidType::Banner)
    } else if imp.video.is_some() {
        Some(BidType::Video)
    } else if imp.audio.is_some() {
        Some(BidType::Audio)
    } else if imp.native.is_some() {
        Some(BidType::Native)
    } else {
        None
    }
}

// ---- local helpers (shared foundation untouched) ----

/// Go `jsonutil.Unmarshal(ext, &T)`: json-iterator matches keys case-insensitively; a non-object
/// (other than null) reports `expect { or n, but found X`; absent ext is empty input.
fn unmarshal_ext<T: serde::de::DeserializeOwned + Default>(ext: Option<&Ext>) -> Result<T, BidderError> {
    use sonic_rs::JsonValueTrait;
    let Some(ext) = ext else {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into()));
    };
    if ext.0.is_null() {
        return Ok(T::default());
    }
    let text = ext.to_json();
    if !ext.0.is_object() {
        let c = text.chars().next().unwrap_or('\u{0}');
        return Err(BidderError::FailedToUnmarshal(format!("expect {{ or n, but found {c}")));
    }
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text)
        .map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))?;
    let lowered: serde_json::Map<String, serde_json::Value> =
        map.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect();
    serde_json::from_value(serde_json::Value::Object(lowered))
        .map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}
