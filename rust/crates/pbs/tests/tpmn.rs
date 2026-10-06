use pbs::adapters::tpmn::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/tpmn", &Adapter::new("https://gat.tpmn.io/ortb/pbs_bidder"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
