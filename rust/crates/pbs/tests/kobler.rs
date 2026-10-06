use pbs::adapters::kobler::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/kobler", &Adapter::new("http://fake.endpoint"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
