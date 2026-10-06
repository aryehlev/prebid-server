use pbs::adapters::connectad::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/connectad", &Adapter::new("http://bidder.connectad.io/API?src=pbs"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
