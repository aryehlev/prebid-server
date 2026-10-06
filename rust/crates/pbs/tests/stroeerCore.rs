use pbs::adapters::stroeer_core::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/stroeerCore", &Adapter::new("http://localhost/s2sdsh"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
