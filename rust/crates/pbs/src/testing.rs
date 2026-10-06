//! Port of Go `adapters/adapterstest/test_json.go`: replays a bidder's JSON fixtures
//! (`{bidder}test/{exemplary,supplemental,amp,video,videosupplemental}/*.json`) against a
//! [`Bidder`] and compares requests, bids and errors the way the Go runner does.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::value::RawValue;
use serde_json::Value;

use crate::bidder::{Bidder, ExtraRequestInfo, RequestData, ResponseData};
use crate::currency::Conversions;
use crate::header::Header;
use crate::ortb::openrtb2::BidRequest;

const SUPPORTED_DIRS: &[&str] = &["exemplary", "supplemental", "amp", "video", "videosupplemental"];

#[derive(Debug, Deserialize)]
struct TestSpec {
    #[serde(rename = "mockBidRequest")]
    bid_request: Box<RawValue>,
    #[serde(rename = "httpCalls", alias = "httpcalls", default)]
    http_calls: Vec<HttpCall>,
    #[serde(rename = "expectedBidResponses", default)]
    bid_responses: Vec<ExpectedBidResponse>,
    #[serde(rename = "expectedMakeRequestsErrors", default)]
    make_request_errors: Vec<ExpectedError>,
    #[serde(rename = "expectedMakeBidsErrors", default)]
    make_bids_errors: Vec<ExpectedError>,
}

#[derive(Debug, Deserialize)]
struct ExpectedError {
    value: String,
    #[serde(default)]
    comparison: String,
}

#[derive(Debug, Deserialize)]
struct HttpCall {
    #[serde(rename = "expectedRequest")]
    request: HttpRequest,
    #[serde(rename = "mockResponse", default)]
    response: HttpResponse,
}

#[derive(Debug, Deserialize)]
struct HttpRequest {
    #[serde(default)]
    body: Option<Box<RawValue>>,
    #[serde(default)]
    uri: String,
    #[serde(default)]
    headers: Option<Header>,
    #[serde(rename = "impIDs", default)]
    imp_ids: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct HttpResponse {
    #[serde(default)]
    status: u16,
    #[serde(default)]
    body: Option<Box<RawValue>>,
    #[serde(default)]
    headers: Option<Header>,
}

#[derive(Debug, Deserialize)]
struct ExpectedBidResponse {
    #[serde(default)]
    bids: Vec<ExpectedBid>,
    #[serde(alias = "Currency", default)]
    currency: String,
    #[serde(rename = "fledgeauctionconfigs", alias = "fledgeAuctionConfigs", default)]
    fledge: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ExpectedBid {
    #[serde(alias = "Bid")]
    bid: Value,
    #[serde(rename = "type", default)]
    bid_type: String,
    #[serde(default)]
    seat: String,
    #[serde(default)]
    video: Option<Value>,
}

/// Go `RunJSONBidderTest`: every `*.json` under `root_dir`'s supported sub-directories.
/// Returns the failures as `"{file}: {reason}"`, empty when all pass.
pub fn run_json_bidder_test(root_dir: impl AsRef<Path>, bidder: &dyn Bidder) -> Vec<String> {
    let mut files = Vec::new();
    collect_json(root_dir.as_ref(), &mut files);
    files.sort();
    let mut failures = Vec::new();
    let mut ran = 0;
    for path in files {
        let dir = path
            .parent()
            .and_then(Path::file_name)
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if !SUPPORTED_DIRS.contains(&dir) {
            continue;
        }
        ran += 1;
        if let Err(e) = run_spec_file(&path, dir, bidder) {
            failures.push(format!("{}: {e}", path.display()));
        }
    }
    if ran == 0 {
        failures.push(format!("{}: no fixtures found", root_dir.as_ref().display()));
    }
    failures
}

fn collect_json(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_json(&p, out);
        } else if p.extension().is_some_and(|e| e == "json") {
            out.push(p);
        }
    }
}

fn run_spec_file(path: &Path, dir: &str, bidder: &dyn Bidder) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|e| format!("read: {e}"))?;
    let spec: TestSpec = serde_json::from_str(&text).map_err(|e| format!("parse spec: {e}"))?;
    let allow_errors = dir != "exemplary" && dir != "video";
    let expects_errors = !spec.make_request_errors.is_empty() || !spec.make_bids_errors.is_empty();
    if !allow_errors && expects_errors {
        return Err("exemplary spec must not expect errors".into());
    }
    run_spec(&spec, bidder, dir == "amp", dir == "videosupplemental" || dir == "video")
}

fn run_spec(spec: &TestSpec, bidder: &dyn Bidder, is_amp: bool, is_video: bool) -> Result<(), String> {
    // Parsed from the raw text, as the seller does, so `ext` keeps its key order (Go reads
    // `imp.ext` in document order, which decides which decode error comes first).
    let request: BidRequest = parse_bid_request(spec.bid_request.get())
        .map_err(|e| format!("mockBidRequest does not parse as BidRequest: {e}"))?;

    let mut req_info = ExtraRequestInfo::default();
    if let Some(rates) = serde_json::from_str::<Value>(spec.bid_request.get()).ok().as_ref().and_then(custom_rates) {
        req_info.currency_conversions = Conversions::new(rates);
    }
    if is_amp {
        req_info.pbs_entry_point = "amp".into();
    } else if is_video {
        req_info.pbs_entry_point = "video".into();
    }

    let (requests, errs) = bidder.make_requests(&request, &req_info);
    assert_error_list("MakeRequests", &errs, &spec.make_request_errors)?;
    assert_make_requests(&requests, &spec.http_calls)?;

    let mut bids_errs = Vec::new();
    let mut responses = Vec::new();
    for i in 0..requests.len() {
        let call = &spec.http_calls[i];
        let ext_req = RequestData {
            method: "POST".into(),
            uri: call.request.uri.clone(),
            body: raw_body(&call.request.body),
            ..Default::default()
        };
        let resp = ResponseData {
            status_code: call.response.status,
            body: raw_body(&call.response.body),
            headers: call.response.headers.clone().unwrap_or_default(),
        };
        let (r, e) = bidder.make_bids(&request, &ext_req, &resp);
        bids_errs.extend(e);
        if let Some(r) = r {
            responses.push(r);
        }
    }
    assert_error_list("MakeBids", &bids_errs, &spec.make_bids_errors)?;

    if responses.len() != spec.bid_responses.len() {
        return Err(format!(
            "MakeBids len(bidResponses) = {} vs len(spec.BidResponses) = {}",
            responses.len(),
            spec.bid_responses.len()
        ));
    }
    for (actual, expected) in responses.iter().zip(&spec.bid_responses) {
        assert_make_bids(actual, expected)?;
    }
    Ok(())
}

/// `sonic_rs` rejects a duplicated key where Go's `encoding/json` lets the last one win (one
/// fixture, resetdigital/simple-audio, repeats `cur`). Only that case goes through
/// `serde_json::Value`, which also keeps the last value, so every other request keeps its
/// `ext` key order.
fn parse_bid_request(text: &str) -> Result<BidRequest, String> {
    match sonic_rs::from_str(text) {
        Ok(r) => Ok(r),
        Err(e) if e.to_string().contains("duplicate field") => {
            let v: Value = serde_json::from_str(text).map_err(|x| x.to_string())?;
            serde_json::from_value(v).map_err(|x| x.to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Go test spec bodies are `json.RawMessage`: absent is empty, anything else is its JSON text.
fn raw_body(body: &Option<Box<RawValue>>) -> Vec<u8> {
    body.as_ref().map_or_else(Vec::new, |v| v.get().as_bytes().to_vec())
}

fn custom_rates(req: &Value) -> Option<HashMap<String, HashMap<String, f64>>> {
    let rates = req.pointer("/ext/prebid/currency/rates")?;
    let rates: HashMap<String, HashMap<String, f64>> = serde_json::from_value(rates.clone()).ok()?;
    (!rates.is_empty()).then_some(rates)
}

fn assert_error_list(
    what: &str,
    actual: &[crate::errortypes::BidderError],
    expected: &[ExpectedError],
) -> Result<(), String> {
    if actual.len() != expected.len() {
        return Err(format!(
            "{what} had wrong error count. Expected {}, got {} ({:?})",
            expected.len(),
            actual.len(),
            actual.iter().map(ToString::to_string).collect::<Vec<_>>()
        ));
    }
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        let msg = a.to_string();
        let ok = match e.comparison.as_str() {
            "" | "literal" => e.value == msg,
            "regex" => regex::Regex::new(&e.value).map_err(|x| x.to_string())?.is_match(&msg),
            "startswith" => msg.starts_with(&e.value),
            other => return Err(format!("invalid comparison type \"{other}\"")),
        };
        if !ok {
            return Err(format!(
                "{what} error[{i}] had wrong message. Expected ({}) \"{}\", got \"{msg}\"",
                if e.comparison.is_empty() { "literal" } else { &e.comparison },
                e.value
            ));
        }
    }
    Ok(())
}

fn assert_make_requests(actual: &[RequestData], expected: &[HttpCall]) -> Result<(), String> {
    if actual.len() != expected.len() {
        return Err(format!(
            "MakeRequests had wrong request count. Expected {}, got {}",
            expected.len(),
            actual.len()
        ));
    }
    // Match without assuming order, as Go does.
    let mut actual_matched = vec![false; actual.len()];
    for (i, exp) in expected.iter().enumerate() {
        let mut last_err = String::new();
        let mut found = false;
        for (j, act) in actual.iter().enumerate() {
            match diff_http_request(act, &exp.request) {
                Ok(()) => {
                    actual_matched[j] = true;
                    found = true;
                    break;
                }
                Err(e) => last_err = e,
            }
        }
        if !found {
            return Err(format!("httpRequest[{i}] was not returned by make_requests: {last_err}"));
        }
    }
    if let Some(j) = actual_matched.iter().position(|m| !m) {
        return Err(format!("Actual RequestData[{j}] was not matched to a result"));
    }
    Ok(())
}

fn diff_http_request(actual: &RequestData, expected: &HttpRequest) -> Result<(), String> {
    if expected.uri != actual.uri {
        return Err(format!("uri \"{}\" does not match expected \"{}\"", actual.uri, expected.uri));
    }
    if let Some(exp_headers) = &expected.headers {
        let a = serde_json::to_value(&actual.headers).map_err(|e| e.to_string())?;
        let e = serde_json::to_value(exp_headers).map_err(|e| e.to_string())?;
        diff_json("headers", &a, &e)?;
    }
    if expected.imp_ids.is_empty() {
        return Err("expected.ImpIDs must contain at least one imp ID".into());
    }
    let (mut a, mut e) = (actual.imp_ids.clone(), expected.imp_ids.clone());
    a.sort();
    e.sort();
    if a != e {
        return Err(format!("actual.ImpIDs {a:?} do not match expected {e:?}"));
    }
    diff_body(&actual.body, expected.body.as_ref())
}

fn diff_body(actual: &[u8], expected: Option<&Box<RawValue>>) -> Result<(), String> {
    match (actual.is_empty(), expected) {
        (true, None) => Ok(()),
        (true, Some(_)) => Err("json diff failed. Expected body bytes, but got 0".into()),
        (false, None) => Err(format!("json diff failed. Expected 0 bytes in body, but got {}", actual.len())),
        (false, Some(exp)) => {
            let act: Value = serde_json::from_slice(actual).map_err(|e| format!("actual body is not JSON: {e}"))?;
            let exp: Value = serde_json::from_str(exp.get()).map_err(|e| format!("expected body is not JSON: {e}"))?;
            diff_json("body", &act, &exp)
        }
    }
}

/// Go decodes both sides into `interface{}` (float64), so `1` and `1.0` are the same number.
fn json_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| json_eq(p, q)),
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| json_eq(v, w)))
        }
        _ => a == b,
    }
}

fn diff_json(what: &str, actual: &Value, expected: &Value) -> Result<(), String> {
    if json_eq(actual, expected) {
        return Ok(());
    }
    Err(format!(
        "{what} json did not match expected.\n  actual:   {actual}\n  expected: {expected}"
    ))
}

fn assert_make_bids(actual: &crate::bidder::BidderResponse, expected: &ExpectedBidResponse) -> Result<(), String> {
    if !expected.currency.is_empty() && expected.currency != actual.currency {
        return Err(format!("Wrong currency. Got {}, expected {}", actual.currency, expected.currency));
    }
    if actual.bids.len() != expected.bids.len() {
        return Err(format!(
            "Wrong bids count. len(bidderResponse.Bids) = {} vs expected {}",
            actual.bids.len(),
            expected.bids.len()
        ));
    }
    for (i, (a, e)) in actual.bids.iter().zip(&expected.bids).enumerate() {
        if a.seat != e.seat {
            return Err(format!("typedBid[{i}].seat \"{}\" does not match expected \"{}\"", a.seat, e.seat));
        }
        if a.bid_type.as_str() != e.bid_type {
            return Err(format!(
                "typedBid[{i}].type \"{}\" does not match expected \"{}\"",
                a.bid_type.as_str(),
                e.bid_type
            ));
        }
        let actual_bid = serde_json::to_value(&a.bid).map_err(|x| x.to_string())?;
        diff_json(&format!("typedBid[{i}].bid"), &actual_bid, &e.bid)?;
        if let Some(exp_video) = &e.video {
            let actual_video = serde_json::to_value(&a.bid_video).map_err(|x| x.to_string())?;
            diff_json(&format!("typedBid[{i}].video"), &actual_video, exp_video)?;
        }
    }
    match (&expected.fledge, &actual.fledge_auction_configs) {
        (Some(e), Some(a)) => {
            let a: Value = serde_json::from_str(&a.to_json()).map_err(|x| x.to_string())?;
            diff_json("fledgeauctionconfigs", &a, e)?;
        }
        (Some(_), None) => return Err("expected fledgeauctionconfigs in bidderResponse".into()),
        (None, Some(_)) => return Err("unexpected fledgeauctionconfigs in bidderResponse".into()),
        (None, None) => {}
    }
    Ok(())
}
