use pbs::adapters::pubnative::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/pubnative", &Adapter::new("http://example.com/prebid"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
