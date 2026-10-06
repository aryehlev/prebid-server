use pbs::adapters::compass::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/compass", &Adapter::new("http://sa-lb.deliverimp.com/pserver"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
