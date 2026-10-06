use pbs::adapters::vidazoo::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/vidazoo", &Adapter::new("http://prebid-server.cootlogix.com/openrtb/"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
