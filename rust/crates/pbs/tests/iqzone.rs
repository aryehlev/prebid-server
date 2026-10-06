use pbs::adapters::iqzone::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/iqzone", &Adapter::new("http://smartssp-us-east.iqzone.com/pserver"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
