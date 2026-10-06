use pbs::adapters::axis::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/axis", &Adapter::new("http://prebid.axis-marketplace.com/pbserver"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
