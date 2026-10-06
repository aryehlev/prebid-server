use pbs::adapters::undertone::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/undertone", &Adapter::new("http://undertone-test/bid"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
