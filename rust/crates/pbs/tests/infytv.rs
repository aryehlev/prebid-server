use pbs::adapters::infytv::Adapter;
use pbs::testing::run_json_bidder_test;

#[test]
fn json_samples() {
    let failures = run_json_bidder_test("tests/fixtures/infytv", &Adapter::new("https://test.infy.tv/pbs/openrtb"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
