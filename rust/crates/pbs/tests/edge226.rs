use pbs::adapters::edge226::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/edge226", &Adapter::new("http://test.com/pserver"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
