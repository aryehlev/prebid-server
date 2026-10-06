use pbs::adapters::telaria::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/telaria", &Adapter::new(""));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
