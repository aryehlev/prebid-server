use pbs::adapters::adxcg::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/adxcg", &Adapter::new("http://localhost/prebid_server"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
