use pbs::adapters::concert::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/concert", &Adapter::new("https://bids.concert.io/bids/openrtb"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
