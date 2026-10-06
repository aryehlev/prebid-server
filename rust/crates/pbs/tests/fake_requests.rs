//! Robustness run over fake requests: every registered bidder, built from its real bidder-info
//! YAML, gets its own Go exemplary request (so the bidder params are valid) in many mangled
//! variants, and its `make_bids` gets garbage responses.
//!
//! This is not a parity check (the fixtures are). It looks for what fixtures never exercise:
//! a panic on input Go would also choke on, or a request body that is not valid JSON.

use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

use pbs::bidder::{Bidder, ExtraRequestInfo, RequestData, ResponseData};
use pbs::ortb::openrtb2::BidRequest;
use pbs::{bidder_info, config, registry};
use serde_json::{json, Value};

/// Bidders whose fixture directory is not named after the bidder.
fn fixture_dir(name: &str) -> &str {
    match name {
        "emx_digital" => "cadent_aperture_mx",
        "mediafuse" => "appnexus",
        other => other,
    }
}

/// The first exemplary request of the bidder's fixtures, if it has any.
fn exemplary_request(name: &str) -> Option<Value> {
    let dir: PathBuf = ["tests/fixtures", fixture_dir(name), "exemplary"].iter().collect();
    let mut files: Vec<_> = fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).collect();
    files.sort();
    files.into_iter().find_map(|p| {
        let v: Value = serde_json::from_str(&fs::read_to_string(p).ok()?).ok()?;
        v.get("mockBidRequest").cloned()
    })
}

/// `imp[0]` as a mutable object, or `None` when the base request has no imps to mangle.
fn imp0(r: &mut Value) -> Option<&mut serde_json::Map<String, Value>> {
    r.get_mut("imp")?.get_mut(0)?.as_object_mut()
}

/// One named mangling of a request.
type Variant = (&'static str, fn(&mut Value));

fn variants() -> Vec<Variant> {
    vec![
        ("as-is", |_| {}),
        ("no imps", |r| r["imp"] = json!([])),
        ("imp is null", |r| r["imp"] = Value::Null),
        ("imp[0].ext removed", |r| {
            imp0(r).map(|o| o.remove("ext"));
        }),
        ("imp[0].ext = {}", |r| { imp0(r).map(|o| o.insert("ext".into(), json!({}))); }),
        ("imp[0].ext = null", |r| { imp0(r).map(|o| o.insert("ext".into(), Value::Null)); }),
        ("imp[0].ext = string", |r| { imp0(r).map(|o| o.insert("ext".into(), json!("garbage"))); }),
        ("imp[0].ext.bidder = {}", |r| { if let Some(e) = imp0(r).and_then(|o| o.get_mut("ext")).and_then(Value::as_object_mut) { e.insert("bidder".into(), json!({})); } }),
        ("imp[0].ext.bidder = []", |r| { if let Some(e) = imp0(r).and_then(|o| o.get_mut("ext")).and_then(Value::as_object_mut) { e.insert("bidder".into(), json!([])); } }),
        ("imp duplicated", |r| {
            if let Some(first) = r["imp"].get(0).cloned() {
                let mut second = first;
                second["id"] = json!("second-imp");
                r["imp"].as_array_mut().unwrap().push(second);
            }
        }),
        ("no media type on imp[0]", |r| {
            if let Some(o) = imp0(r) {
                for k in ["banner", "video", "native", "audio"] {
                    o.remove(k);
                }
            }
        }),
        ("empty banner", |r| { imp0(r).map(|o| o.insert("banner".into(), json!({}))); }),
        ("empty video", |r| { imp0(r).map(|o| o.insert("video".into(), json!({}))); }),
        ("empty native", |r| { imp0(r).map(|o| o.insert("native".into(), json!({}))); }),
        ("site null", |r| {
            r.as_object_mut().map(|o| o.remove("site"));
        }),
        ("app and site both", |r| {
            r["site"] = json!({"id": "s", "page": "https://example.com/p", "publisher": {"id": "p"}});
            r["app"] = json!({"id": "a", "bundle": "com.example.app", "publisher": {"id": "p"}});
        }),
        ("device removed", |r| {
            r.as_object_mut().map(|o| o.remove("device"));
        }),
        ("user removed", |r| {
            r.as_object_mut().map(|o| o.remove("user"));
        }),
        ("regs gdpr + coppa", |r| {
            r["regs"] = json!({"coppa": 1, "ext": {"gdpr": 1, "us_privacy": "1YYY"}});
            r["user"] = json!({"ext": {"consent": "CPXxRfAPXxRfAAfKABENB-CgAAAAAAAAAAYgAAAAAAAA"}});
        }),
        ("test mode + tmax 0", |r| {
            r["test"] = json!(1);
            r["tmax"] = json!(0);
        }),
        ("currency not USD", |r| r["cur"] = json!(["EUR", "JPY"])),
        ("unicode and html in ids", |r| {
            r["id"] = json!("id-ü-<b>&\"'");
            imp0(r).map(|o| o.insert("id".into(), json!("imp-日本-</script>")));
        }),
    ]
}

/// Responses a buyer could send that no adapter should panic on.
fn garbage_responses() -> Vec<(&'static str, u16, &'static str)> {
    vec![
        ("empty 200", 200, ""),
        ("null", 200, "null"),
        ("{}", 200, "{}"),
        ("[]", 200, "[]"),
        ("not json", 200, "<html>oops</html>"),
        ("truncated", 200, r#"{"id":"1","seatbid":[{"bid":["#),
        ("empty seatbid", 200, r#"{"id":"1","seatbid":[]}"#),
        ("empty bid", 200, r#"{"id":"1","seatbid":[{"bid":[{}]}]}"#),
        ("null bid", 200, r#"{"id":"1","seatbid":[{"bid":[null]}]}"#),
        ("wrong types", 200, r#"{"id":1,"seatbid":"x","cur":5}"#),
        ("bid without impid", 200, r#"{"id":"1","seatbid":[{"bid":[{"id":"b","price":1.5,"adm":"<div/>"}]}]}"#),
        ("bid for unknown imp", 200, r#"{"id":"1","seatbid":[{"bid":[{"id":"b","impid":"nope","price":1.5}]}]}"#),
        ("negative price", 200, r#"{"id":"1","seatbid":[{"bid":[{"id":"b","impid":"x","price":-1}]}]}"#),
        ("204", 204, ""),
        ("400", 400, "bad"),
        ("500", 500, "boom"),
        ("0 status", 0, ""),
    ]
}

/// Bidders with no endpoint in the seller's bidder-info YAML; production supplies one through
/// `pbsBidderEndpointOverrides`, so the run does the same.
const NEEDS_ENDPOINT_OVERRIDE: &[&str] = &["adxcg", "avocet", "ix", "pangle"];

/// Go v3.30.0 `resetdigital` parses its endpoint template but never sets `endpointUri`, so every
/// request goes to `""` (the Go fixtures expect that). The port keeps it; not a failure here.
const GO_EMITS_EMPTY_URI: &[&str] = &["resetdigital"];

fn server() -> config::Server {
    config::Server { external_url: "http://hosturl.com".into(), gvl_id: 1, data_center: "2".into() }
}

/// Bodies the adapters send that start like JSON must be JSON.
fn body_problem(req: &RequestData) -> Option<String> {
    let first = req.body.iter().find(|b| !b" \t\r\n".contains(b))?;
    if (*first == b'{' || *first == b'[') && serde_json::from_slice::<Value>(&req.body).is_err() {
        return Some("request body looks like JSON but does not parse".into());
    }
    None
}

#[test]
fn fake_requests_never_panic_or_emit_broken_output() {
    // Keep the first line of every panic (adapter panics are caught below, but a bug in this
    // harness must stay visible and not be swallowed).
    std::panic::set_hook(Box::new(|info| {
        let loc = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let msg = info.payload().downcast_ref::<&str>().map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned()).unwrap_or_default();
        eprintln!("PANIC at {loc}: {}", msg.lines().next().unwrap_or(""));
    }));

    let mut panics: Vec<String> = Vec::new();
    let mut problems: Vec<String> = Vec::new();
    let (mut bidders_run, mut requests_made, mut skipped) = (0usize, 0usize, Vec::new());

    for name in registry::BIDDER_NAMES {
        let Some(base) = exemplary_request(name) else {
            skipped.push(*name);
            continue;
        };
        let info = bidder_info::load(name).unwrap();
        let mut adapter_cfg = info.adapter_config();
        if NEEDS_ENDPOINT_OVERRIDE.contains(name) {
            adapter_cfg.endpoint = format!("https://{name}.example.test/openrtb");
        }
        let Ok(bidder) = registry::build(name, &adapter_cfg, &server()) else {
            skipped.push(*name);
            continue;
        };
        bidders_run += 1;

        for (label, mangle) in variants() {
            let mut json_req = base.clone();
            mangle(&mut json_req);
            // A request the model cannot even parse is the seller's problem, not the adapter's.
            let Ok(request) = sonic_rs::from_str::<BidRequest>(&json_req.to_string()) else { continue };

            let ctx = format!("{name} / {label}");
            let made = catch_unwind(AssertUnwindSafe(|| bidder.make_requests(&request, &ExtraRequestInfo::default())));
            let (requests, _errs) = match made {
                Ok(r) => r,
                Err(_) => {
                    panics.push(format!("{ctx}: make_requests panicked"));
                    continue;
                }
            };
            requests_made += requests.len();
            for r in &requests {
                if r.uri.is_empty() && !GO_EMITS_EMPTY_URI.contains(name) {
                    problems.push(format!("{ctx}: request with an empty uri"));
                }
                if let Some(p) = body_problem(r) {
                    problems.push(format!("{ctx}: {p}"));
                }
            }

            // Garbage responses against the first request this variant produced.
            if let Some(first) = requests.first() {
                for (rlabel, status, body) in garbage_responses() {
                    let resp = ResponseData { status_code: status, body: body.as_bytes().to_vec(), ..Default::default() };
                    let got = catch_unwind(AssertUnwindSafe(|| bidder.make_bids(&request, first, &resp)));
                    match got {
                        Err(_) => panics.push(format!("{ctx} / response {rlabel}: make_bids panicked")),
                        Ok((Some(_), _)) => {}
                        Ok((None, _)) => {}
                    }
                }
            }
        }
    }

    let _ = std::panic::take_hook();
    println!("bidders run: {bidders_run}, requests produced: {requests_made}, skipped: {}", skipped.len());
    println!("skipped (no exemplary fixture or no build from yaml): {skipped:?}");

    // Collapse repeats so the report names each adapter once per kind of failure.
    panics.sort();
    panics.dedup();
    problems.sort();
    problems.dedup();
    println!("PANICS: {}", panics.len());
    for p in &panics {
        println!("  {p}");
    }
    println!("PROBLEMS: {}", problems.len());
    for p in &problems {
        println!("  {p}");
    }
    assert!(panics.is_empty() && problems.is_empty(), "{} panics, {} problems (see output)", panics.len(), problems.len());
}
