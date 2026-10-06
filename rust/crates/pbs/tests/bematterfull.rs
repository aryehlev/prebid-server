use pbs::adapters::bematterfull::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/bematterfull", &Adapter::new("http://prebid-srv.mtflll-system.live/?pid={{.SourceId}}&host={{.Host}}").unwrap());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
