//! Port of Go `adapters/ucfunnel/ucfunnel_test.go` (the adapter has no runnable JSON fixtures:
//! its `ucfunneltest/exemplary/ucfunnel.json` is not a bidder-test spec and Go never loads it).

use pbs::adapters::ucfunnel::Adapter;
use pbs::bidder::{Bidder, ExtraRequestInfo, RequestData, ResponseData};
use pbs::ortb::openrtb2::{Audio, Banner, BidRequest, Imp, Native, Video};
use pbs::ortb::Ext;

fn imps() -> Vec<Imp> {
    let mk = |id: &str| Imp { id: id.into(), ..Default::default() };
    let mut a = mk("1234");
    a.banner = Some(Banner::default());
    let mut b = mk("1235");
    b.video = Some(Video::default());
    let mut c = mk("1236");
    c.audio = Some(Audio::default());
    let mut d = mk("1237");
    d.native = Some(Native::default());
    let e = d.clone();
    vec![a, b, c, d, e]
}

fn ext(s: &str) -> Option<Ext> {
    Some(Ext::from_slice(s.as_bytes()).unwrap())
}

fn with_exts() -> BidRequest {
    let mut req = BidRequest { imp: imps(), ..Default::default() };
    let ok = r#"{"bidder": {"adunitid": "ad-488663D474E44841E8A293379892348","partnerid": "par-7E6D2DB9A8922AB07B44A444D2BA67"}}"#;
    for i in 0..4 {
        req.imp[i].ext = ext(ok);
    }
    req.imp[4].ext = ext(r#"{"bidder": {"adunitid": "aa","partnerid": ""}}"#);
    req
}

#[test]
fn make_requests() {
    let bidder = Adapter::new("http://localhost/bid");
    let info = ExtraRequestInfo::default();

    let (reqs, errs) = bidder.make_requests(&BidRequest::default(), &info);
    assert_eq!((reqs.len(), errs.len()), (0, 1));

    let (reqs, errs) = bidder.make_requests(&BidRequest { imp: imps(), ..Default::default() }, &info);
    assert_eq!((reqs.len(), errs.len()), (0, 1));

    let (reqs, errs) = bidder.make_requests(&with_exts(), &info);
    assert_eq!(errs.len(), 0);
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].imp_ids, vec!["1234", "1235", "1236", "1237", "1237"]);
    assert_eq!(reqs[0].uri, "http://localhost/bid/par-7E6D2DB9A8922AB07B44A444D2BA67/request");
}

#[test]
fn make_bids() {
    let bidder = Adapter::new("http://localhost/bid");
    let req03 = with_exts();
    let mut req04 = BidRequest { imp: vec![imps().remove(0)], ..Default::default() };
    req04.imp[0].ext = ext(r#"{"bidder": {"adunitid": "0"}}"#);

    let rd01 = RequestData {
        method: "POST".into(),
        body: br#"{"imp":[{"id":"1234","banner":{}},{"id":"1235","video":{}},{"id":"1236","audio":{}},{"id":"1237","native":{}}]}"#.to_vec(),
        ..Default::default()
    };
    let rd02 = RequestData {
        method: "POST".into(),
        body: br#"{"imp":[{"id":"1234","banne"1235","video":{}},{"id":"1236","audio":{}},{"id":"1237","native":{}}]}"#.to_vec(),
        ..Default::default()
    };
    let resp = |status: u16, body: &str| ResponseData { status_code: status, body: body.as_bytes().to_vec(), ..Default::default() };
    let ok = r#"{"seatbid": [{"bid": [{"impid": "1234"}]},{"bid": [{"impid": "1235"}]},{"bid": [{"impid": "1236"}]},{"bid": [{"impid": "1237"}]}]}"#;
    let other = r#"{"seatbid":[{"bid":[{"impid":"1234"}]},{"bid":[{"impid":"1235"}]}]}"#;
    let bad = r#"{"seatbid":[{"bid":[{"im236"}],{"bid":[{"impid":"1237}]}"#;

    // (request, request data, response, want response, want errors)
    let cases: Vec<(&BidRequest, &RequestData, ResponseData, bool, bool)> = vec![
        (&req03, &rd01, resp(200, ok), true, false),
        (&req03, &rd01, resp(203, other), false, true),
        (&req03, &rd01, resp(204, other), false, false),
        (&req03, &rd01, resp(400, other), false, true),
        (&req03, &rd01, resp(200, bad), false, true),
        (&req04, &rd02, resp(200, ok), false, true),
    ];
    for (i, (req, rd, resp, want_resp, want_err)) in cases.into_iter().enumerate() {
        let (r, errs) = bidder.make_bids(req, rd, &resp);
        assert_eq!(r.is_some(), want_resp, "case {i} response");
        assert_eq!(!errs.is_empty(), want_err, "case {i} errors");
    }
}
