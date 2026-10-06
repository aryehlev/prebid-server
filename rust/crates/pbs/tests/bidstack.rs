use pbs::adapters::bidstack::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/bidstack", &Adapter::new("http://mock-adserver.url"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
