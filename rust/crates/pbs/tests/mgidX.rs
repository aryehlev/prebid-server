use pbs::adapters::mgid_x::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/mgidX", &Adapter::new("http://example.com/pserver"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
