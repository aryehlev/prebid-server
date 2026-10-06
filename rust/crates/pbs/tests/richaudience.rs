use pbs::adapters::richaudience::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/richaudience", &Adapter::new("https://ortb.richaudience.com/ortb/?bidder=pbs"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
