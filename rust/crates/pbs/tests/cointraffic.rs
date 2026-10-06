use pbs::adapters::cointraffic::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/cointraffic", &Adapter::new("http://test-request.com/prebid"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
