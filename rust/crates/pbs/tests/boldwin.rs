use pbs::adapters::boldwin::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/boldwin", &Adapter::new("http://ssp.videowalldirect.com/pserver"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
