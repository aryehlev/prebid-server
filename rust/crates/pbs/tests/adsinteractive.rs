use pbs::adapters::adsinteractive::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/adsinteractive", &Adapter::new("http://bid.adsinteractive.com/prebid"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
