use pbs::adapters::freewheelssp::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/freewheelssp", &Adapter::new("https://testjsonsample.com"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
