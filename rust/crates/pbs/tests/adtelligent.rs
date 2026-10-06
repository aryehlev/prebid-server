use pbs::adapters::adtelligent::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/adtelligent", &Adapter::new("http://ghb.adtelligent.com/pbs/ortb"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
