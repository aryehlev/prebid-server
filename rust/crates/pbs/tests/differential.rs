//! Differential test: the Rust adapters against the real Go Prebid Server v3.30.0 adapters
//! (the version rtb-seller-digital pins), on identical cases.
//!
//! `tools/gen_cases.py` writes `tools/cases.json` (fixture requests, mangled variants, garbage
//! responses). `tools/godump` runs the Go adapters on it. This test runs the Rust ones and
//! compares. It needs a Go toolchain; set `GODUMP_OUT=/path/out.json` to compare against an
//! existing dump instead of running Go.
//!
//! The comparison is by category so a known, deliberate difference does not hide a new one.

use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::process::{Command, Stdio};

use pbs::bidder::{ExtraRequestInfo, RequestData, ResponseData};
use pbs::ortb::openrtb2::BidRequest;
use pbs::{config, registry};
use serde_json::Value;

fn server() -> config::Server {
    config::Server { external_url: "http://hosturl.com".into(), gvl_id: 1, data_center: "2".into() }
}

fn go_output(cases: &str) -> Value {
    if let Ok(path) = std::env::var("GODUMP_OUT") {
        return serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    }
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tools/godump");
    let build = Command::new("go")
        .args(["build", "-o", "/tmp/pbs-godump", "."])
        .current_dir(dir)
        .env("GOFLAGS", "-mod=mod")
        .env("GOPROXY", "off")
        .output()
        .expect("go is required (or set GODUMP_OUT)");
    assert!(build.status.success(), "go build failed: {}", String::from_utf8_lossy(&build.stderr));
    let mut child = Command::new("/tmp/pbs-godump").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(cases.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    serde_json::from_slice(&out.stdout).unwrap()
}

/// JSON equality as Go's test runner does it: numbers by value.
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

#[derive(Default)]
struct Rust {
    panicked: bool,
    build_error: Option<String>,
    requests: Vec<RequestData>,
    errors: Vec<String>,
    bids_present: bool,
    currency: String,
    bids: Vec<(Value, String, String)>,
    bids_errors: Vec<String>,
}

fn run_rust(case: &Value) -> Rust {
    let mut out = Rust::default();
    let s = |k: &str| case[k].as_str().unwrap_or("").to_string();
    let adapter_cfg = config::Adapter {
        endpoint: s("endpoint"),
        extra_adapter_info: s("extra_info"),
        platform_id: s("platform_id"),
        app_secret: s("app_secret"),
        ..Default::default()
    };
    let bidder = match registry::build(&s("bidder"), &adapter_cfg, &server()) {
        Ok(b) => b,
        Err(e) => {
            out.build_error = Some(e);
            return out;
        }
    };
    // Go decodes into openrtb2.BidRequest; a request the model cannot read is a build-level miss.
    let Ok(request) = sonic_rs::from_str::<BidRequest>(&case["request"].to_string()) else {
        out.build_error = Some("request does not parse as BidRequest".into());
        return out;
    };
    let made = catch_unwind(AssertUnwindSafe(|| bidder.make_requests(&request, &ExtraRequestInfo::default())));
    let Ok((requests, errs)) = made else {
        out.panicked = true;
        return out;
    };
    out.errors = errs.iter().map(ToString::to_string).collect();
    out.requests = requests;
    if let (Some(resp), Some(first)) = (case.get("response").filter(|r| !r.is_null()), out.requests.first()) {
        let resp = ResponseData {
            status_code: resp["status"].as_u64().unwrap_or(0) as u16,
            body: resp["body"].as_str().unwrap_or("").as_bytes().to_vec(),
            ..Default::default()
        };
        match catch_unwind(AssertUnwindSafe(|| bidder.make_bids(&request, first, &resp))) {
            Err(_) => out.panicked = true,
            Ok((br, errs)) => {
                out.bids_errors = errs.iter().map(ToString::to_string).collect();
                if let Some(br) = br {
                    out.bids_present = true;
                    out.currency = br.currency;
                    out.bids = br
                        .bids
                        .iter()
                        .map(|b| (serde_json::to_value(&b.bid).unwrap(), b.bid_type.as_str().to_string(), b.seat.clone()))
                        .collect();
                }
            }
        }
    }
    out
}

#[test]
fn rust_adapters_match_go_v3_30_0() {
    std::panic::set_hook(Box::new(|_| {}));
    let cases_text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tools/cases.json"))
        .expect("run tools/gen_cases.py first");
    let cases: Vec<Value> = serde_json::from_str(&cases_text).unwrap();
    let go = go_output(&cases_text);
    let go = go.as_array().unwrap();
    assert_eq!(go.len(), cases.len());

    // category -> example ids
    let mut diffs: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    let mut same = 0usize;
    let mut go_panics = 0usize;

    for (case, g) in cases.iter().zip(go) {
        let id = case["id"].as_str().unwrap().to_string();
        let r = run_rust(case);
        let mut note = |cat: &'static str| diffs.entry(cat).or_default().push(id.clone());

        let g_panic = g.get("panic").is_some_and(|p| !p.is_null());
        let g_build = g.get("build_error").and_then(Value::as_str).filter(|s| !s.is_empty());

        if g_panic {
            go_panics += 1;
            // Go panics; the port returns an error instead (a documented deviation).
            if r.panicked {
                note("rust panicked where go panicked");
            } else {
                note("go panics, rust returns (documented)");
            }
            continue;
        }
        if r.panicked {
            note("RUST PANICKED, go did not");
            continue;
        }
        match (g_build, &r.build_error) {
            (Some(_), Some(_)) => {
                same += 1;
                continue;
            }
            (Some(_), None) => {
                note("go build error, rust built");
                continue;
            }
            (None, Some(e)) if e.starts_with("request does not parse") => {
                note("rust model rejects a request go reads");
                continue;
            }
            (None, Some(_)) => {
                note("rust build error, go built");
                continue;
            }
            (None, None) => {}
        }

        let g_errs: Vec<&str> = g["errors"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        let g_reqs = g["requests"].as_array().unwrap();
        let mut ok = true;

        if g_errs.len() != r.errors.len() {
            note("make_requests error count differs");
            ok = false;
        } else if g_errs.iter().zip(&r.errors).any(|(a, b)| a != b) {
            note("make_requests error text differs");
            ok = false;
        }
        if g_reqs.len() != r.requests.len() {
            note("request count differs");
            ok = false;
        } else {
            for (gq, rq) in g_reqs.iter().zip(&r.requests) {
                if gq["uri"].as_str().unwrap_or("") != rq.uri {
                    note("request uri differs");
                    ok = false;
                }
                if gq["method"].as_str().unwrap_or("") != rq.method {
                    note("request method differs");
                    ok = false;
                }
                let rbody: Value = serde_json::from_slice(&rq.body).unwrap_or(Value::Null);
                if !json_eq(&gq["body"], &rbody) {
                    note("request body differs");
                    ok = false;
                }
                let mut gids: Vec<&str> = gq["imp_ids"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
                let mut rids: Vec<&str> = rq.imp_ids.iter().map(String::as_str).collect();
                gids.sort_unstable();
                rids.sort_unstable();
                if gids != rids {
                    note("request imp ids differ");
                    ok = false;
                }
                let gh: BTreeMap<String, Vec<String>> = gq["headers"]
                    .as_object()
                    .map(|o| o.iter().map(|(k, v)| (k.clone(), v.as_array().unwrap().iter().filter_map(|x| x.as_str().map(String::from)).collect())).collect())
                    .unwrap_or_default();
                let rh: BTreeMap<String, Vec<String>> = rq.headers.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                if gh != rh {
                    note("request headers differ");
                    ok = false;
                }
            }
        }

        if case.get("response").is_some_and(|r| !r.is_null()) {
            let g_berrs: Vec<&str> = g["bids_errors"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
            if g_berrs.len() != r.bids_errors.len() {
                note("make_bids error count differs");
                ok = false;
            } else if g_berrs.iter().zip(&r.bids_errors).any(|(a, b)| a != b) {
                note("make_bids error text differs");
                ok = false;
            }
            if g["bids_present"].as_bool().unwrap_or(false) != r.bids_present {
                note("make_bids response presence differs");
                ok = false;
            } else if r.bids_present {
                if g["currency"].as_str().unwrap_or("") != r.currency {
                    note("make_bids currency differs");
                    ok = false;
                }
                let gb = g["bids"].as_array().unwrap();
                if gb.len() != r.bids.len() {
                    note("make_bids bid count differs");
                    ok = false;
                } else {
                    for (x, (bid, ty, seat)) in gb.iter().zip(&r.bids) {
                        if !json_eq(&x["bid"], bid) {
                            note("make_bids bid body differs");
                            ok = false;
                        }
                        if x["type"].as_str().unwrap_or("") != ty {
                            note("make_bids bid type differs");
                            ok = false;
                        }
                        if x["seat"].as_str().unwrap_or("") != seat {
                            note("make_bids seat differs");
                            ok = false;
                        }
                    }
                }
            }
        }
        if ok {
            same += 1;
        }
    }
    let _ = std::panic::take_hook();

    // Every difference, for a fix loop to read: DIFF_REPORT=/path/report.tsv
    if let Ok(path) = std::env::var("DIFF_REPORT") {
        let mut rows = String::new();
        for (cat, ids) in &diffs {
            for id in ids {
                rows.push_str(&format!("{cat}\t{id}\n"));
            }
        }
        std::fs::write(path, rows).unwrap();
    }

    println!("cases: {}, identical to go: {same}, go panics: {go_panics}", cases.len());
    for (cat, ids) in &diffs {
        let bidders: std::collections::BTreeSet<_> = ids.iter().map(|i| i.split('|').next().unwrap()).collect();
        println!("DIFF {:<45} cases {:>5}  bidders {:>3}  e.g. {}", cat, ids.len(), bidders.len(), ids[0]);
    }
    let real: usize = diffs
        .iter()
        .filter(|(k, _)| !k.starts_with("go panics, rust returns"))
        .map(|(_, v)| v.len())
        .sum();
    assert_eq!(real, 0, "{real} cases differ from go beyond the documented panics (see output)");
}
