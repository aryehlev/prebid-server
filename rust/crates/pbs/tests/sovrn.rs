use pbs::adapters::sovrn::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/sovrn", &Adapter::new("http://sovrn.com/test/endpoint"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
