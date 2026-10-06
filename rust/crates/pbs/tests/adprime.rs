use pbs::adapters::adprime::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/adprime", &Adapter::new("http://delta.adprime.com/pserver"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
