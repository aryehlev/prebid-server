use pbs::adapters::outbrain::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/outbrain", &Adapter::new("http://example.com/bid"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
